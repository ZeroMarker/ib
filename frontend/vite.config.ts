import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const assetVersion = '20260825-ux4'

export default defineConfig({
  plugins: [react(), {
    name: 'version-static-assets',
    transformIndexHtml(html) {
      return html
        .replace('./assets/app.js', `./assets/app.js?v=${assetVersion}`)
        .replace('./assets/index.css', `./assets/index.css?v=${assetVersion}`)
        .replace('./manifest.webmanifest', `./manifest.webmanifest?v=${assetVersion}`)
        .replace('./icons/icon-192.png', `./icons/icon-192.png?v=${assetVersion}`)
        .replace('./icons/icon.svg', `./icons/icon.svg?v=${assetVersion}`)
    },
  }],
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
})
