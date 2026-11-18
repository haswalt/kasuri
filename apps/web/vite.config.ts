import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8787',
        changeOrigin: true,
        configure(proxy) {
          proxy.on('proxyReq', (forwarded, incoming) => {
            // Translate only this dev server's own Origin; foreign origins still reach
            // the backend unchanged and are rejected by its production CSRF checks.
            if (incoming.headers.origin === 'http://' + incoming.headers.host) {
              forwarded.setHeader('Origin', 'http://127.0.0.1:8787')
            }
          })
        }
      }
    }
  }
})
