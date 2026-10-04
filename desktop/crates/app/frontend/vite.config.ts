import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  appType: 'spa',
  // Tauri expects a fixed port, fail if it's already in use
  server: {
    port: 5173,
    strictPort: true,
  },
  // Tries to prevent Vite from obscuring Rust errors
  clearScreen: false,
})
