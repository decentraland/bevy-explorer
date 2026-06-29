// Bumped v1 → v2 so the `activate` handler deletes the old cache once — it held stale local-preview
// scene builds (see the localhost bypass below) that pinned the first build forever.
const CACHE_NAME = 'ipfs-path-cache-v2';
const CUSTOM_HEADER = 'X-IPFS';

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

    // For same-origin requests, add COOP/COEP headers to enable SharedArrayBuffer
    if (request.mode === 'navigate' || request.destination === 'document') {
        event.respondWith(addCrossOriginIsolationHeaders(request));
    }
});

/**
 * Fetches a request and adds Cross-Origin-Isolation headers to enable SharedArrayBuffer.
 */
async function addCrossOriginIsolationHeaders(request) {
    const response = await fetch(request);

    // Only modify same-origin responses
    if (response.type === 'basic') {
        const newHeaders = new Headers(response.headers);
        newHeaders.set('Cross-Origin-Opener-Policy', 'same-origin');
        newHeaders.set('Cross-Origin-Embedder-Policy', 'credentialless');

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

    //Generate a cache key from the path only
    const cacheKey = getCacheKey(request);

    //Open the cache
    const cache = await caches.open(CACHE_NAME);

    //Try to find a response in the cache
    const cachedResponse = await cache.match(cacheKey);
    if (cachedResponse) {
        return cachedResponse;
    }

    //If not in cache, fetch from network
    const networkRequest = stripCustomHeader(request);
    const networkResponse = await fetch(networkRequest);
    
    if (networkResponse.ok) {
        // Store the new response in the cache
        const responseToCache = networkResponse.clone();
        await cache.put(cacheKey, responseToCache);
    }
    
    return networkResponse;
}

function getCacheKey(request) {
    const url = new URL(request.url);
    return url.pathname + url.search;
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
