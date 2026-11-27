import { defineConfig } from 'vite'
import { resolve } from 'node:path'

// Two pages: the main window and the click-through overlay drawn over the game.
export default defineConfig({
  base: './',
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: 'es2022',
    rollupOptions: {
      input: { main: resolve(__dirname, 'index.html'), overlay: resolve(__dirname, 'overlay.html') },
    },
  },
})
