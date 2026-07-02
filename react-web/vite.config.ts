import { spawn } from 'node:child_process'
import { createReadStream, statSync } from 'node:fs'
import { createConnection } from 'node:net'
import { extname, join, normalize } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'

// Dev-only: run the bridge scene's live preview (`sdk-commands start` on :8100 — the engine's
// default systemScene in dev, with scene hot-reload) alongside vite, so `npm run dev` is the ONE
// command. If :8100 is already serving (your own terminal, or Playwright's webServer), leave it
// alone. The child is killed with the dev server (detached group so sdk-commands' own children
// don't survive as orphans).
function bridgeScenePreview(): Plugin {
  return {
    name: 'bridge-scene-preview',
    apply: 'serve',
    configureServer(server) {
      const probe = createConnection({ port: 8100, host: '127.0.0.1' })
      probe.once('connect', () => probe.destroy()) // already running — reuse it
      probe.once('error', () => {
        console.log('[bridge-scene] starting live preview on :8100')
        const child = spawn('npx', ['sdk-commands', 'start', '--no-browser', '--port', '8100'], {
          cwd: fileURLToPath(new URL('./bridge-scene', import.meta.url)),
          stdio: ['ignore', 'inherit', 'inherit'],
          detached: true
        })
        const stop = (): void => {
          if (child.pid != null) {
            try {
              process.kill(-child.pid, 'SIGTERM')
            } catch {
              /* already gone */
            }
          }
        }
        server.httpServer?.once('close', stop)
        process.once('exit', stop)
      })
    }
  }
}

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

export default defineConfig(({ command }) => ({
  // Production is served from a VERSIONED CDN subpath (cdn.decentraland.org/<pkg>/<version>/ —
  // see deploy/web/scripts/prebuild.js), so built asset URLs can't be origin-absolute. CI passes
  // PUBLIC_URL for an absolute CDN base; otherwise './' (relative → works from any path, e.g. a
  // local `serve deploy/web`). Dev keeps '/'.
  base: command === 'build' ? (process.env.PUBLIC_URL ? `${process.env.PUBLIC_URL}/` : './') : '/',
  plugins: [
    react(),
    bridgeScenePreview(),
    coiHeadersExceptAuth(),
    serveStatic('/engine/', '../deploy/web/engine'),
    // Our headless super-user bridge scene (exported deployable). Pointed at by
    // the engine's systemScene so it loads as the trusted --ui scene.
    serveStatic('/bridge-scene/static/', './bridge-scene/static')
  ],
  build: {
    // The app IS the production page: build straight into the npm-published deploy/web tree,
    // beside engine/ + bridge-scene/ (emptyOutDir would wipe them).
    outDir: '../deploy/web',
    emptyOutDir: false,
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
    // Allow Cloudflare quick-tunnel hosts (random *.trycloudflare.com per run) so the dev server
    // can be reached through a tunnel. The leading dot matches any subdomain; scoping to this
    // domain keeps Vite's DNS-rebinding protection on for everything else.
    allowedHosts: ['.trycloudflare.com'],
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
}))
