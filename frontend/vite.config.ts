/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const assetVersion = '20260825-ux5'

// The service worker is versioned by this same string; bump both together so a
// new build invalidates the cached page shell and static assets.
const versionedStaticAssets = () => ({
  name: 'version-static-assets',
  transformIndexHtml(html: string) {
    return html
      .replace('./assets/app.js', `./assets/app.js?v=${assetVersion}`)
      .replace('./assets/index.css', `./assets/index.css?v=${assetVersion}`)
      .replace('./manifest.webmanifest', `./manifest.webmanifest?v=${assetVersion}`)
      .replace('./icons/icon-192.png', `./icons/icon-192.png?v=${assetVersion}`)
      .replace('./icons/icon.svg', `./icons/icon.svg?v=${assetVersion}`)
  },
})

export default defineConfig({
  plugins: [react(), versionedStaticAssets()],
  base: './',
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
