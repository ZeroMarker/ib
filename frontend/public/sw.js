const CACHE_NAME = 'ib-shell-v3'
const ASSET_VERSION = '20260825-ux3'
const SHELL = [
  './',
  `./manifest.webmanifest?v=${ASSET_VERSION}`,
  `./assets/app.js?v=${ASSET_VERSION}`,
  `./assets/index.css?v=${ASSET_VERSION}`,
  `./icons/icon-192.png?v=${ASSET_VERSION}`,
  `./icons/icon-512.png?v=${ASSET_VERSION}`,
]

self.addEventListener('install', (event) => {
  event.waitUntil(caches.open(CACHE_NAME).then((cache) => cache.addAll(SHELL)).then(() => self.skipWaiting()))
})

self.addEventListener('activate', (event) => {
  event.waitUntil(caches.keys().then((keys) => Promise.all(keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key)))).then(() => self.clients.claim()))
})

self.addEventListener('fetch', (event) => {
  const request = event.request
  if (request.method !== 'GET' || new URL(request.url).pathname.includes('/api/')) return

  if (request.mode === 'navigate') {
    event.respondWith(fetch(request).catch(() => caches.match('./')))
    return
  }

  event.respondWith(fetch(request).then((response) => {
    const copy = response.clone()
    caches.open(CACHE_NAME).then((cache) => cache.put(request, copy))
    return response
  }).catch(() => caches.match(request)))
})
