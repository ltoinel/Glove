import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Forward backend routes to the Actix server. The frontend calls the API with
// relative URLs (e.g. `/api/...`), so both the dev server and the production
// `vite preview` server must proxy them — otherwise the static server answers
// `/api` requests with index.html and the backend appears unreachable.
const proxy = {
  '/api': {
    target: 'http://localhost:8080',
    changeOrigin: true,
  },
}

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: './src/test/setup.js',
  },
  server: {
    port: 3000,
    proxy,
  },
  preview: {
    port: 3000,
    proxy,
  },
})
