import { defineConfig } from 'vite'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const { version } = JSON.parse(readFileSync(resolve(__dirname, 'package.json'), 'utf8')) as { version: string }

// Two pages: the main window and the click-through overlay drawn over the game.
export default defineConfig({
  base: './',
  clearScreen: false,
  define: { __APP_VERSION__: JSON.stringify(version) },
  server: { port: 1420, strictPort: true },
  build: {
    target: 'es2022',
    rollupOptions: {
      input: { main: resolve(__dirname, 'index.html'), overlay: resolve(__dirname, 'overlay.html') },
    },
  },
})
