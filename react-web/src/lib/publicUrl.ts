// The page's directory URL, treating the pathname as a directory even without a trailing slash —
// the production entry URL is decentraland.zone/bevy-web (no slash), which would otherwise make
// relative resolution escape to the origin root. Same-origin things (the engine, the
// bridge scene, the service worker) resolve against THIS, never against the asset CDN base.
export const PAGE_DIR = new URL(location.pathname.replace(/\/?$/, '/'), location.href).href

// Public-dir assets referenced at RUNTIME (JSX src strings) are not rebased by Vite the way
// index.html / CSS url()s are. Resolve them against the app's base (absolute CDN URL in prod
// builds, '/' in dev, './' in local builds — see vite.config.ts `base`).
const BASE = new URL(import.meta.env.BASE_URL, PAGE_DIR).href

export const publicUrl = (path: string): string => new URL(path, BASE).href
