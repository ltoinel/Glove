import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import process from 'node:process'

// Host name the portal is served under behind Caddy (bin/start.sh). Vite
// rejects requests for unknown hosts, so the dev server must be told about it.
const portalHost = process.env.GLOVE_PORTAL_HOST || 'portal.glove'

// Forward backend routes to the Actix server. The frontend calls the API with
// relative URLs (e.g. `/api/...`), so both the dev server and the production
// `vite preview` server must proxy them — otherwise the static server answers
// `/api` requests with index.html and the backend appears unreachable.
// Behind Caddy the build targets the API domain instead (VITE_API_URL), and
// the proxy only serves a bare `npm run dev`.
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
    allowedHosts: [portalHost],
    proxy,
  },
  preview: {
    port: 3000,
    proxy,
  },
})
