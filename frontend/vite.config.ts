/// <reference types="vitest/config" />
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import { ASSET_VERSION } from './version.json'

// Where `ib serve` is listening. Override with IB_API_TARGET when the service
// runs elsewhere. Keep the default in sync with SERVER_ADDR.
const API_TARGET = process.env.IB_API_TARGET ?? 'http://127.0.0.1:8081'

// `public/sw.js` is copied to dist verbatim, so it cannot import the shared
// version constant. Injecting it at build time keeps one source of truth
// (version.json) instead of two hand-maintained constants that drift apart.
//
// This runs in `closeBundle` rather than `generateBundle` because Vite copies
// `public/` outside the rollup bundle: at `generateBundle` time the emitted
// `sw.js` is not in the bundle object, so the substitution would silently
// no-op and ship the raw placeholder to production.
const versionedServiceWorker = (): Plugin => ({
  name: 'versioned-service-worker',
  closeBundle() {
    const worker = resolve(import.meta.dirname, 'dist', 'sw.js')
    const source = readFileSync(worker, 'utf8')
    // Fail the build loudly: shipping the raw placeholder to production would
    // give every asset the literal cache key `?v=__ASSET_VERSION__`, so all
    // builds would share one cache entry and never invalidate.
    if (!source.includes('__ASSET_VERSION__')) {
      throw new Error(
        'dist/sw.js is missing the __ASSET_VERSION__ placeholder. Either public/sw.js ' +
          'was changed to hardcode a version, or the versioned-service-worker plugin was removed.',
      )
    }
    writeFileSync(worker, source.replaceAll('__ASSET_VERSION__', ASSET_VERSION))
  },
})

// The service worker is versioned by the same constant as the assets it
// caches; bump ASSET_VERSION in version.json so a new build invalidates both
// the cached page shell and the static assets.
const versionedStaticAssets = (): Plugin => ({
  name: 'version-static-assets',
  transformIndexHtml(html: string) {
    return html
      .replace('./assets/app.js', `./assets/app.js?v=${ASSET_VERSION}`)
      .replace('./assets/index.css', `./assets/index.css?v=${ASSET_VERSION}`)
      .replace('./manifest.webmanifest', `./manifest.webmanifest?v=${ASSET_VERSION}`)
      .replace('./icons/icon-192.png', `./icons/icon-192.png?v=${ASSET_VERSION}`)
      .replace('./icons/icon.svg', `./icons/icon.svg?v=${ASSET_VERSION}`)
  },
})

export default defineConfig({
  plugins: [react(), versionedStaticAssets(), versionedServiceWorker()],
  base: './',
  server: {
    // The app fetches `api/...` relative to the page. In production the Rust
    // binary serves both the shell and the API from one origin, so the
    // relative path just works. In `vite dev` the page comes from :5173
    // instead, and without this proxy every request would 404.
    proxy: {
      '/api': { target: API_TARGET, changeOrigin: false },
    },
  },
  build: {
    rollupOptions: {
      output: {
        entryFileNames: 'assets/app.js',
        chunkFileNames: 'assets/[name].js',
        assetFileNames: 'assets/[name][extname]',
      },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    // Reuse one jsdom per worker: constructing five of them dominated the
    // run time. Per-file isolation is preserved.
    pool: 'vmThreads',
  },
})
