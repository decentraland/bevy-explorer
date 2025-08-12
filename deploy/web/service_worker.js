const CACHE_NAME = 'ipfs-path-cache-v1';
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
    }
});

async function cacheFirstStrategy(request) {
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
