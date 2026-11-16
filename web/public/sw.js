// pic2gray4px Web service worker.
// The cache name carries the app version; activate deletes only this app's
// own older version caches. The share cache (pic2gray4px-share) is never
// touched here — it is the mailbox for share-target images: the fetch
// handler stashes a shared image there and the app consumes it once.

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

// Share-target intake: static hosting cannot take a POST, so the worker
// receives it instead. The image is stashed in the share cache under a
// per-share key (rapid successive shares cannot overwrite each other),
// stamped with its save time for the app's TTL sweep, and the browser is
// redirected to the app, which consumes the image once. The image never
// leaves the device — this cache is the whole "server" side.
async function handleShare(request) {
  const scope = self.registration.scope;
  try {
    const form = await request.formData();
    const file = form.get("image");
    if (file && typeof file === "object" && file.size > 0) {
      const id = crypto.randomUUID();
      const cache = await caches.open(SHARE_CACHE);
      await cache.put(new URL(`shared-image/${id}`, scope).href, new Response(file, {
        headers: {
          "Content-Type": file.type || "application/octet-stream",
          "x-pic2gray4px-saved-at": String(Date.now()),
        },
      }));
      return Response.redirect(new URL(`?share=${id}`, scope).href, 303);
    }
  } catch {
    // Malformed share: fall through to the plain redirect.
  }
  return Response.redirect(scope, 303);
}

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (
    request.method === "POST" &&
    new URL(request.url).pathname.endsWith("/share-target")
  ) {
    event.respondWith(handleShare(request));
    return;
  }
  if (request.method !== "GET") {
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
