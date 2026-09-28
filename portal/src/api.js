// Base URL of the Glove API. Empty means same origin: `npm run dev` proxies
// /api to the backend (vite.config.js). Behind Caddy the portal and the API
// live on separate domains, so the build bakes in VITE_API_URL
// (e.g. https://api.glove) — see bin/build.sh.
export const API_URL = (import.meta.env.VITE_API_URL ?? '').replace(/\/+$/, '')

/** Absolute URL of an API path such as `/api/places`. */
export const apiUrl = (path) => `${API_URL}${path}`
