// pic2gray4px Web service worker.
// The cache name carries the app version; activate deletes only this app's
// own older version caches. The share cache (pic2gray4px-share) is never
// touched here — it holds in-flight share-target images (intake lands in M3).

const VERSION = "__APP_VERSION__";
const CACHE = `pic2gray4px-${VERSION}`;
const SHARE_CACHE = "pic2gray4px-share";
const CORE_ASSETS = [
  "./",
  "./index.html",
  "./manifest.webmanifest",
  "./icons/icon-192.png",
  "./icons/icon-512.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(CORE_ASSETS))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys
            .filter(
              (key) =>
                key.startsWith("pic2gray4px-") &&
                key !== CACHE &&
                key !== SHARE_CACHE,
            )
            .map((key) => caches.delete(key)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") {
    // Share-target POST intake is added in M3.
    return;
  }
  if (request.mode === "navigate") {
    // Navigations: network-first, fall back to the cached page offline.
    event.respondWith(
      fetch(request)
        .then((response) => {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
          return response;
        })
        .catch(() => caches.match(request, { ignoreSearch: true })),
    );
    return;
  }
  // Static assets: cache-first (the cache is versioned).
  event.respondWith(
    caches.match(request, { ignoreSearch: true }).then(
      (hit) =>
        hit ??
        fetch(request).then((response) => {
          if (response.ok && new URL(request.url).origin === self.location.origin) {
            const copy = response.clone();
            caches.open(CACHE).then((cache) => cache.put(request, copy));
          }
          return response;
        }),
    ),
  );
});
