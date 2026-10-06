// MUST stay in sync with crates/ipfs/src/web_cache.rs (cache name, both keys, the cached header):
// the engine moves entries it has checked to the shared key, and its asset processor reads the
// raw responses this worker caches and writes the processed bytes back over the same key.
// v2: v1 entries were shared across content servers on the path alone, with nothing checked.
const CACHE_NAME = 'ipfs-path-cache-v2';
const CUSTOM_HEADER = 'X-IPFS';
// Stand-in origin for entries shared across content servers. `.invalid` is reserved, so no real
// server's own entries can land on these keys.
const SHARED_ORIGIN = 'https://shared.invalid';
// Set on responses answered from the cache.
const CACHED_HEADER = 'X-IPFS-Cached';

self.addEventListener('install', (event) => {
    // Force the waiting service worker to become the active service worker.
    self.skipWaiting();
});

self.addEventListener('activate', (event) => {
    console.log('[IPFS Cache Service Worker]: Active');
    
    // An array of cache names that are "allowed" to exist.
    const cacheWhitelist = [CACHE_NAME];

    event.waitUntil(
        // Get all the cache keys (names) that exist.
        caches.keys().then(cacheNames => {
            return Promise.all(
                cacheNames.map(cacheName => {
                    // If a cache name is NOT in our whitelist...
                    if (cacheWhitelist.indexOf(cacheName) === -1) {
                        console.log(`SW: Deleting old cache: ${cacheName}`);
                        // ...delete it.
                        return caches.delete(cacheName);
                    }
                })
            );
        }).then(() => {
            // After cleanup is done, claim the clients.
            return self.clients.claim();
        })
    );
});

self.addEventListener('fetch', (event) => {
    const request = event.request;

    // Check if the request has our custom header.
    if (request.headers.has(CUSTOM_HEADER)) {
        // If it does, use our caching strategy.
        event.respondWith(cacheFirstStrategy(request));
        return;
    }

    // Same-origin page loads: isolate the app, and keep the shell that embeds it unisolated
    if (request.mode === 'navigate' || request.destination === 'document') {
        event.respondWith(addCrossOriginIsolationHeaders(request));
    }
});

/**
 * Fetches a page and sets its isolation headers. The app (app.html: HUD + engine) gets
 * Document-Isolation-Policy, which makes it cross-origin isolated (SharedArrayBuffer) in a process
 * of its own. The shell that embeds it (the scope's index page) loses the host's COEP: an isolated
 * top-level page would take the app's process back in with it. Any other page (another host's
 * engine page, say) gets COOP/COEP credentialless to enable SharedArrayBuffer.
 */
async function addCrossOriginIsolationHeaders(request) {
    const response = await fetch(request);

    // Only modify same-origin responses
    if (response.type === 'basic') {
        const newHeaders = new Headers(response.headers);
        const path = new URL(request.url).pathname;
        const scope = new URL(self.registration.scope).pathname;
        newHeaders.set('Cross-Origin-Opener-Policy', 'same-origin');
        // the scene server's frame (engine/headless.js) takes the app's policy: the app scripts it,
        // which only documents with the same policy may do
        if (path === scope + 'app.html' || path === scope + 'engine/headless.html') {
            newHeaders.delete('Cross-Origin-Embedder-Policy');
            newHeaders.set('Document-Isolation-Policy', 'isolate-and-credentialless');
        } else if (path === scope || path === scope + 'index.html') {
            newHeaders.delete('Cross-Origin-Embedder-Policy');
        } else {
            newHeaders.set('Cross-Origin-Embedder-Policy', 'credentialless');
        }

        return new Response(response.body, {
            status: response.status,
            statusText: response.statusText,
            headers: newHeaders
        });
    }

    return response;
}

async function cacheFirstStrategy(request) {
    // DEV: never cache LOCAL preview content. sdk-commands' `start` server hashes mutable files by a
    // CONSTANT id (the file path), so cache-first would serve the first build of the local bridge scene
    // forever — every rebuild would be invisible. Always go to network for localhost so scene edits show.
    const reqUrl = new URL(request.url);
    if (reqUrl.hostname === 'localhost' || reqUrl.hostname === '127.0.0.1') {
        return fetch(stripCustomHeader(request));
    }

    // Cache entries are keyed by url alone, so only a GET may fill or be answered from one.
    if (request.method !== 'GET') {
        return fetch(stripCustomHeader(request));
    }

    // Two keys. The shared one is the hash alone (the last path segment), so every content server
    // serving a hash hits the same entry. This worker never writes it: any client (a scene
    // included) can make it fetch and store a response, so what it stores stays under the url it
    // came from. The engine moves an entry to the shared key once it has checked the bytes
    // against the hash.
    const sharedKey = SHARED_ORIGIN + '/' + reqUrl.pathname.split('/').pop();
    const originKey = reqUrl.origin + reqUrl.pathname + reqUrl.search;

    //Open the cache
    const cache = await caches.open(CACHE_NAME);

    //Try to find a response in the cache
    const sharedResponse = await cache.match(sharedKey);
    if (sharedResponse) {
        // an origin's own copy is dead weight once the shared entry exists (one can be left behind
        // when the engine never got to check it). Not awaited: nothing depends on it.
        cache.delete(originKey).catch(() => {});
    }
    const cachedResponse = sharedResponse ?? (await cache.match(originKey));
    if (cachedResponse) {
        const headers = new Headers(cachedResponse.headers);
        headers.set(CACHED_HEADER, '1');
        return new Response(cachedResponse.body, {
            status: cachedResponse.status,
            statusText: cachedResponse.statusText,
            headers
        });
    }

    //If not in cache, fetch from network
    const networkRequest = stripCustomHeader(request);
    const networkResponse = await fetch(networkRequest);
    
    if (networkResponse.ok) {
        // Store the new response in the cache
        const responseToCache = networkResponse.clone();
        await cache.put(originKey, responseToCache);
    }
    
    return networkResponse;
}

function stripCustomHeader(request) {
    const newHeaders = new Headers(request.headers);
    newHeaders.delete(CUSTOM_HEADER);
    
    const newRequest = new Request(request.url, {
        method: request.method,
        headers: newHeaders,
        body: request.body,
        mode: request.mode,
        credentials: request.credentials,
        cache: request.cache,
        redirect: request.redirect,
        referrer: request.referrer,
        integrity: request.integrity
    });

    return newRequest;
}
