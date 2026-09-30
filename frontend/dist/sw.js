// 20260930-swver is substituted at build time by the
// `versioned-service-worker` plugin in vite.config.ts, which reads
// version.json. version.json is the single source of truth shared with the
// `?v=` query strings on the shell assets. Do not hardcode the value here:
// Vite copies this file verbatim and never resolves an import.
const ASSET_VERSION = '20260930-swver'

// Bump on any change to the cached shell. Cleared on activate.
const CACHE_NAME = 'ib-shell-v6'

const SHELL = [
  './',
  `./manifest.webmanifest?v=${ASSET_VERSION}`,
  `./assets/app.js?v=${ASSET_VERSION}`,
  `./assets/index.css?v=${ASSET_VERSION}`,
  `./icons/icon-192.png?v=${ASSET_VERSION}`,
  `./icons/icon-512.png?v=${ASSET_VERSION}`,
]

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE_NAME)
      .then((cache) => cache.addAll(SHELL))
      .then(() => self.skipWaiting()),
  )
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key))),
      )
      .then(() => self.clients.claim()),
  )
})

self.addEventListener('fetch', (event) => {
  const request = event.request
  if (request.method !== 'GET' || new URL(request.url).pathname.includes('/api/')) return

  if (request.mode === 'navigate') {
    event.respondWith(fetch(request).catch(() => caches.match('./')))
    return
  }

  event.respondWith(
    fetch(request)
      .then((response) => {
        const copy = response.clone()
        caches.open(CACHE_NAME).then((cache) => cache.put(request, copy))
        return response
      })
      .catch(() => caches.match(request)),
  )
})
