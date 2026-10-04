// The scene editor's preview realm: a Decentraland realm answered out of Cache Storage, so a
// project built in the browser loads in the engine with no server. Contract: PREVIEW_REALM.md.
// A classic script: service_worker.js loads it with importScripts, the tests import it for its
// side effect.
(() => {
    const PROJECT_ID = /^[a-z0-9][a-z0-9-]{0,63}$/;
    const LOCAL_HASH_PREFIX = 'b64-';
    const PARCEL = /^-?\d+,-?\d+$/;

    const JSON_TYPE = 'application/json';
    const BYTES_TYPE = 'application/octet-stream';

    // Project bytes are untrusted and same-origin with the page: never sniffed, never a document.
    function respond(body, status, contentType) {
        return new Response(body, {
            status,
            headers: {
                'Content-Type': contentType,
                'X-Content-Type-Options': 'nosniff',
                'Content-Security-Policy': 'sandbox',
                'Cross-Origin-Resource-Policy': 'same-origin',
                'Cache-Control': 'no-store',
            },
        });
    }

    const json = (value, status = 200) => respond(JSON.stringify(value), status, JSON_TYPE);
    const notFound = () => respond(null, 404, BYTES_TYPE);
    const noContent = () => respond(null, 204, BYTES_TYPE);

    // Scenes run in sandbox workers on this origin and learn the preview's url from their realm
    // info; a project's files are the engine's and the page's alone. `client` is the url of the
    // service worker client that sent the request, undefined when it has none.
    function fromScene(client) {
        return typeof client !== 'string' || new URL(client).pathname.endsWith('/sandbox_worker.bundle.js');
    }
    const forbidden = () => respond(null, 403, BYTES_TYPE);

    // `<previewRoot><projectId>/<path>` -> { realm, path }, or null for anything else.
    function parse(url, previewRoot) {
        const { origin, pathname } = new URL(url);
        const bare = origin + pathname;
        if (!bare.startsWith(previewRoot)) return null;
        const rest = bare.slice(previewRoot.length);
        const slash = rest.indexOf('/');
        if (slash < 0 || !PROJECT_ID.test(rest.slice(0, slash))) return null;
        return {
            realm: previewRoot + rest.slice(0, slash + 1),
            // the engine appends `/about` to the realm it was given: a trailing slash doubles up
            path: rest.slice(slash).replace(/^\/+/, ''),
        };
    }

    async function readEntity(store, realm) {
        const stored = await store.match(realm + '__manifest');
        if (!stored) return null;
        try {
            const { entity } = await stored.json();
            const valid =
                entity &&
                typeof entity.id === 'string' &&
                entity.id.startsWith(LOCAL_HASH_PREFIX) &&
                Array.isArray(entity.pointers) &&
                // they become the realm's parcels in /about
                entity.pointers.length > 0 &&
                entity.pointers.every((p) => typeof p === 'string' && PARCEL.test(p)) &&
                Array.isArray(entity.content);
            return valid ? entity : null;
        } catch {
            return null;
        }
    }

    function about(realm, entity) {
        return {
            healthy: true,
            acceptingUsers: true,
            configurations: {
                networkId: 0,
                globalScenesUrn: [],
                // empty: the engine then asks entities/active for the parcels below
                scenesUrn: [],
                localSceneParcels: entity.pointers,
                realmName: 'LocalPreview',
            },
            content: { healthy: true, publicUrl: realm + 'content' },
            comms: { healthy: true, protocol: 'v3', fixedAdapter: 'offline:offline' },
        };
    }

    // The catalyst of the deployment this page is on, as the engine composes it (`peer.<base>`).
    function catalystContent(realm) {
        const { hostname } = new URL(realm);
        const zone = hostname === 'decentraland.zone' || hostname.endsWith('.decentraland.zone');
        return `https://peer.decentraland.${zone ? 'zone' : 'org'}/content`;
    }

    // Parcels are the project's. Everything else the engine asks its realm for (wearables and
    // emotes, by urn) is a catalyst's to answer: without them the avatar has no body.
    async function activeEntities(request, realm, entity) {
        let pointers;
        try {
            ({ pointers } = await request.json());
        } catch {
            pointers = null;
        }
        if (!Array.isArray(pointers)) return json([]);
        const local = pointers.some((p) => entity.pointers.includes(p)) ? [entity] : [];
        const remote = pointers.filter((p) => typeof p === 'string' && !PARCEL.test(p));
        if (remote.length === 0) return json(local);
        try {
            const res = await fetch(catalystContent(realm) + '/entities/active', {
                method: 'POST',
                headers: { 'Content-Type': JSON_TYPE },
                body: JSON.stringify({ pointers: remote }),
            });
            const found = res.ok ? await res.json() : null;
            // an error, not an empty list: the engine asks again instead of calling them missing
            if (!Array.isArray(found)) return respond(null, 502, BYTES_TYPE);
            // the parcels are the project's alone
            return json(local.concat(found.filter((e) => e?.type !== 'scene')));
        } catch {
            return respond(null, 502, BYTES_TYPE);
        }
    }

    async function contents(store, realm, entity, hash) {
        if (!hash.startsWith(LOCAL_HASH_PREFIX)) return notFound();
        // the engine loads the scene entity itself by id, through the contents route
        if (hash === entity.id) return respond(JSON.stringify(entity), 200, BYTES_TYPE);
        const stored = await store.match(realm + 'content/contents/' + hash);
        return stored ? respond(stored.body, 200, BYTES_TYPE) : notFound();
    }

    // The storage the dev server (sdk-commands start) serves a scene's server, per realm, in one
    // JSON document like its server-storage.json: { env, world, players }. Only the server copy
    // the page is running may reach it: the engine adds the page's secret to that copy's storage
    // requests, and the page keeps a `{ token }` entry under `<realm>/__server` while it previews.
    const STORAGE_HEADER = 'x-dcl-local-server';
    const STORAGE_ROUTE = /^(?:values(?:\/(.*))?|players\/([^/]+)\/values(?:\/(.*))?|env\/(.*))$/;
    const writes = new Map();

    // one read-modify-write at a time per realm
    function serialized(key, task) {
        const next = (writes.get(key) || Promise.resolve()).then(task, task);
        const settled = next.catch(() => {});
        writes.set(key, settled);
        settled.then(() => {
            if (writes.get(key) === settled) writes.delete(key);
        });
        return next;
    }

    async function allowed(request, realm, storage) {
        const token = request.headers.get(STORAGE_HEADER);
        const stored = token ? await storage.match(realm + '__server') : null;
        if (!stored) return false;
        try {
            const access = await stored.json();
            return typeof access.token === 'string' && access.token === token;
        } catch {
            return false;
        }
    }

    // null-prototype, so a key such as `__proto__` is just a key
    const dict = (value) =>
        Object.assign(Object.create(null), value && typeof value === 'object' && !Array.isArray(value) ? value : {});

    async function readStorage(storage, key) {
        const stored = await storage.match(key);
        let data = null;
        try {
            data = stored ? await stored.json() : null;
        } catch {
            data = null;
        }
        return { env: dict(data && data.env), world: dict(data && data.world), players: dict(data && data.players) };
    }

    function page(values, url) {
        const params = new URL(url).searchParams;
        const prefix = params.get('prefix');
        let entries = Object.entries(values).map(([key, value]) => ({ key, value }));
        if (prefix) entries = entries.filter((entry) => entry.key.startsWith(prefix));
        const total = entries.length;
        const offset = params.has('offset') ? Math.max(0, parseInt(params.get('offset'), 10) || 0) : 0;
        const limit = params.has('limit') ? Math.max(1, parseInt(params.get('limit'), 10) || 0) : entries.length;
        return json({ data: entries.slice(offset, offset + limit), pagination: { offset, total } });
    }

    async function bodyValue(request) {
        try {
            return { value: JSON.parse(await request.text()).value };
        } catch {
            return null;
        }
    }

    // `/values[/key]`, `/players/<address>/values[/key]`, `/env/<key>`, answered as the dev server does
    const decode = (part) => (part === undefined ? undefined : decodeURIComponent(part));

    function scopeOf(data, env, address) {
        if (env !== undefined) return data.env;
        if (address === undefined) return data.world;
        data.players[address] = dict(data.players[address]);
        return data.players[address];
    }

    function missing(name, env, address) {
        if (env !== undefined) return `Environment variable '${name}' not found`;
        if (address !== undefined) return `Player storage key '${name}' not found for '${address}'`;
        return `Storage key '${name}' not found`;
    }

    async function handleStorage(request, realm, route, storage) {
        if (!storage || !(await allowed(request, realm, storage))) return forbidden();
        let key, address, env;
        try {
            key = decode(route[1] ?? route[3]);
            address = decode(route[2]);
            env = decode(route[4]);
        } catch {
            return json({ message: 'Malformed key' }, 400);
        }
        if (key === '' || env === '') return json({ message: 'Key is required' }, 400);
        if (address === '') return json({ message: 'Address is required' }, 400);
        const docKey = realm + '__storage';
        const method = request.method;
        const name = env ?? key;

        if (method === 'GET') {
            const values = scopeOf(await readStorage(storage, docKey), env, address);
            if (name === undefined) return page(values, request.url);
            if (!Object.hasOwn(values, name)) return json({ message: missing(name, env, address) }, 404);
            return json({ value: values[name] });
        }
        if (name === undefined || (method !== 'PUT' && method !== 'DELETE')) return notFound();
        const body = method === 'PUT' ? await bodyValue(request) : null;
        if (method === 'PUT' && body === null) return json({ message: `Failed to set '${name}'` }, 500);
        return serialized(docKey, async () => {
            const data = await readStorage(storage, docKey);
            const values = scopeOf(data, env, address);
            if (method === 'PUT') values[name] = body.value;
            else delete values[name];
            await storage.put(docKey, new Response(JSON.stringify(data), { headers: { 'Content-Type': JSON_TYPE } }));
            if (method === 'DELETE' || env !== undefined) return noContent();
            return json({ value: body.value });
        });
    }

    // `store` is the preview Cache (anything with `match(url)`), `storage` the storage cache
    // (`match`, `put`). Always answers, and from the stores alone, but for the pointers a
    // catalyst owns (activeEntities). Storage routes take any client: their token is the gate.
    async function handle(request, previewRoot, store, storage, client) {
        const target = parse(request.url, previewRoot);
        if (!target) return notFound();
        const route = STORAGE_ROUTE.exec(target.path);
        if (route) return handleStorage(request, target.realm, route, storage);
        if (fromScene(client)) return forbidden();
        const entity = await readEntity(store, target.realm);
        if (!entity) return notFound();

        const { realm, path } = target;
        if (request.method === 'POST') {
            return path === 'content/entities/active' ? activeEntities(request, realm, entity) : notFound();
        }
        if (request.method !== 'GET') return notFound();
        if (path === 'about') return json(about(realm, entity));
        if (path === 'scene.json') return entity.metadata ? json(entity.metadata) : notFound();
        if (path.startsWith('content/contents/')) {
            return contents(store, realm, entity, path.slice('content/contents/'.length));
        }
        return notFound();
    }

    // The editor package's own scene (react-web host/editorScene.ts): the page stores `about` and
    // the files it checked against their hashes, under `<root><entityId>/`; this serves them as stored.
    async function handleEditorScene(request, root, store, client) {
        if (fromScene(client)) return forbidden();
        const { origin, pathname } = new URL(request.url);
        const path = (origin + pathname).slice(root.length);
        if (request.method !== 'GET' || !/^baf[a-z2-7]+\/(about|contents\/baf[a-z2-7]+)$/.test(path)) return notFound();
        const stored = await store.match(root + path);
        if (!stored) return notFound();
        return respond(stored.body, 200, path.endsWith('/about') ? JSON_TYPE : BYTES_TYPE);
    }

    globalThis.dclPreviewRealm = { handle, handleEditorScene };
})();
