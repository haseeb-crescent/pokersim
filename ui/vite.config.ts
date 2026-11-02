import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The Rust server (crates/server) listens on 8787 and serves ui/dist in production.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:8787',
      '/ws': { target: 'ws://127.0.0.1:8787', ws: true },
    },
  },
})
