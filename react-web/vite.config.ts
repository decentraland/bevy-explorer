import { createReadStream, statSync } from 'node:fs'
import { extname, join, normalize } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'

// Cross-origin isolation is REQUIRED by the engine (SharedArrayBuffer + WebGPU).
const crossOriginIsolation = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  // credentialless (not require-corp): the engine pulls scene assets from
  // cross-origin catalyst servers that don't send CORP. credentialless still
  // yields a cross-origin-isolated context (SharedArrayBuffer works) but loads
  // those no-CORP resources without credentials instead of blocking them. This
  // matches what the engine's own service worker sets.
  'Cross-Origin-Embedder-Policy': 'credentialless',
  'Cross-Origin-Resource-Policy': 'cross-origin'
}

const MIME: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.wasm': 'application/wasm',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.css': 'text/css; charset=utf-8',
  '.bin': 'application/octet-stream',
  '.data': 'application/octet-stream'
}

// Serve a locally-built static directory same-origin under a URL prefix, with
// the COOP/COEP headers WebGPU + wasm threads require. Used to host the engine
// bundle (../deploy/web) and the built bridge scene in a same-origin iframe —
// no npm download needed.
function serveStatic(prefix: string, dirFromConfig: string): Plugin {
  const root = fileURLToPath(new URL(dirFromConfig, import.meta.url))
  return {
    name: `serve-static:${prefix}`,
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (!req.url || !req.url.startsWith(prefix)) return next()
        const rel = decodeURIComponent(req.url.slice(prefix.length).split('?')[0])
        let file = normalize(join(root, rel))
        if (file !== root && !file.startsWith(root + '/')) {
          res.statusCode = 403
          return res.end('forbidden')
        }
        try {
          if (statSync(file).isDirectory()) file = join(file, 'index.html')
        } catch {
          /* fall through */
        }
        let stat
        try {
          stat = statSync(file)
        } catch {
          res.statusCode = 404
          return res.end('not found')
        }
        for (const [k, v] of Object.entries(crossOriginIsolation)) res.setHeader(k, v)
        res.setHeader('Content-Type', MIME[extname(file).toLowerCase()] ?? 'application/octet-stream')
        res.setHeader('Content-Length', stat.size)
        createReadStream(file).pipe(res)
      })
    }
  }
}

// Apply the cross-origin-isolation headers the engine needs to every response EXCEPT the
// proxied auth dapp (/auth). The auth site is a normal web page that signs in with popups /
// OAuth redirects, which `Cross-Origin-Opener-Policy: same-origin` would break — and it does
// not need SharedArrayBuffer. Excluding it lets local sign-in work while the engine stays
// isolated. (In production the app and /auth are genuinely same-origin and the deploy host
// sets headers per-path; this only governs the dev server.)
function coiHeadersExceptAuth(): Plugin {
  return {
    name: 'coi-headers-except-auth',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (!req.url || !req.url.startsWith('/auth')) {
          for (const [k, v] of Object.entries(crossOriginIsolation)) res.setHeader(k, v)
        }
        next()
      })
    }
  }
}

export default defineConfig({
  plugins: [
    react(),
    coiHeadersExceptAuth(),
    serveStatic('/engine/', '../deploy/web'),
    // Our headless super-user bridge scene (exported deployable). Pointed at by
    // the engine's systemScene so it loads as the trusted --ui scene.
    serveStatic('/bridge-scene/static/', './bridge-scene/static')
  ],
  build: {
    // The only chunk over the default 500KB is the isolated emoji dataset (a cached data blob,
    // not executable HUD code). Raise the limit so the build stays clean; revisit if a CODE chunk
    // approaches it.
    chunkSizeWarningLimit: 600,
    rollupOptions: {
      output: {
        // Split the single ~880KB bundle so chunks download in parallel and cache
        // independently (vendor + design system rarely change). The heavy full-screen
        // menu pages are grouped together and kept out of the core HUD chunk.
        manualChunks(id) {
          if (id.includes('node_modules')) return 'vendor'
          // The emoji dataset is ~716KB (78KB gz) — over half the JS. Pin it to its own chunk so
          // it caches independently and never bloats/busts the core HUD chunk. (Deferring it fully
          // behind the chat picker/autocomplete is a follow-up — see emojiData.ts.)
          if (id.includes('emojis_complete.json') || id.includes('/chat/emojiData')) return 'emoji'
          // Showcase is dev-only (?showcase=1) and lazy-loaded — leave it out of the design
          // chunk so its lazy boundary survives and it never ships in the prod HUD path.
          if (id.includes('/src/design/') && !id.includes('Showcase')) return 'design'
          if (/\/src\/features\/(map|backpack|communities|gallery|places)\//.test(id)) return 'menus'
          return undefined
        }
      }
    }
  },
  server: {
    // Proxy the auth dapp so it is served same-origin on localhost — sign in locally against
    // zone and the signed AuthIdentity lands in this origin's localStorage (the marketplace
    // does the same). Because it's same-origin, the auth site also accepts our localhost
    // `redirectTo`. Switch the target to decentraland.org/.today for other envs.
    proxy: {
      // Keep this env in sync with EngineHost REALM (currently mainnet/.org) so the account
      // you sign in as matches the realm the engine loads your profile/friends from. Switch
      // both to decentraland.zone together when testing against zone.
      '/auth': {
        target: 'https://decentraland.org',
        changeOrigin: true,
        secure: false,
        followRedirects: true,
        ws: true
      }
    }
  },
  preview: { headers: crossOriginIsolation }
})
