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

    const json = (value) => respond(JSON.stringify(value), 200, JSON_TYPE);
    const notFound = () => respond(null, 404, BYTES_TYPE);

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

    // `store` is the preview Cache (anything with `match(url)`). Always answers, and from the
    // store alone, but for the pointers a catalyst owns (activeEntities).
    async function handle(request, previewRoot, store) {
        const target = parse(request.url, previewRoot);
        if (!target) return notFound();
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
    async function handleEditorScene(request, root, store) {
        const { origin, pathname } = new URL(request.url);
        const path = (origin + pathname).slice(root.length);
        if (request.method !== 'GET' || !/^baf[a-z2-7]+\/(about|contents\/baf[a-z2-7]+)$/.test(path)) return notFound();
        const stored = await store.match(root + path);
        if (!stored) return notFound();
        return respond(stored.body, 200, path.endsWith('/about') ? JSON_TYPE : BYTES_TYPE);
    }

    globalThis.dclPreviewRealm = { handle, handleEditorScene };
})();
