// Dev server for the gates (playwright.gate.config.ts): the app's own config plus the editor's
// in-browser build tool, served same-origin under /web-build/, and a one-scene realm to start in.
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { defineConfig, mergeConfig, type Plugin } from 'vite'
import base, { serveStatic } from '../vite.config.ts'
import { GATE_PORTS } from './ports.ts'

const dist = join(process.env.WEB_EDITOR_DIR ?? '', 'packages/web-build/dist')
if (!existsSync(join(dist, 'index.js'))) {
  throw new Error(`WEB_EDITOR_DIR must be a dcl-editor checkout with web-build built (no ${dist}/index.js)`)
}

const HOME = '/gate-home/'
const HOME_ENTITY = 'gate-home-entity'
const HOME_BUNDLE = 'gate-home-bundle'
const HOME_CONTENT: Record<string, string> = {
  [HOME_BUNDLE]: [
    "const { crdtSendToRenderer } = require('~system/EngineApi')",
    "exports.onStart = async function () { console.log('GATE_HOME up') }",
    'exports.onUpdate = async function () { await crdtSendToRenderer({ data: new Uint8Array() }) }'
  ].join('\n'),
  [HOME_ENTITY]: JSON.stringify({
    id: HOME_ENTITY,
    type: 'scene',
    pointers: ['0,0'],
    timestamp: 0,
    content: [{ file: 'bin/index.js', hash: HOME_BUNDLE }],
    metadata: {
      ecs7: true,
      runtimeVersion: '7',
      display: { title: 'Gate home' },
      main: 'bin/index.js',
      scene: { parcels: ['0,0'], base: '0,0' }
    }
  })
}

// The realm the editor gate's player starts in and goes back to (e2e/editor.gate.spec.ts): the
// engine only takes absolute content urls, so /about names this server's own port, not the Host it was asked with.
function homeRealm(): Plugin {
  return {
    name: 'gate-home-realm',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const path = (req.url ?? '').split('?')[0]
        if (!path.startsWith(HOME)) return next()
        const origin = `http://localhost:${GATE_PORTS.page}`
        const contents = `${origin}${HOME}contents`
        const body =
          path === `${HOME}about`
            ? JSON.stringify({
                healthy: true,
                acceptingUsers: true,
                configurations: {
                  networkId: 0,
                  globalScenesUrn: [],
                  scenesUrn: [`urn:decentraland:entity:${HOME_ENTITY}?=&baseUrl=${contents}/`],
                  realmName: 'gate-home'
                },
                content: { healthy: true, publicUrl: contents },
                // answers 404, "no profile": a wallet can log in here and its default profile
                // is deployed to this realm's content url, which is nowhere
                lambdas: { healthy: true, publicUrl: `${origin}${HOME}lambdas` },
                comms: { healthy: true, protocol: 'v3', fixedAdapter: 'offline:offline' }
              })
            : HOME_CONTENT[path.slice(`${HOME}contents/`.length)]
        if (body == null) {
          res.statusCode = 404
          return res.end('not found')
        }
        res.setHeader('Content-Type', 'application/json')
        res.end(body)
      })
    }
  }
}

export default defineConfig((env) =>
  mergeConfig(base(env), {
    // its own dep cache, per page port: this server can run next to the everyday dev server and another gate run
    cacheDir: `.vite/gate-${GATE_PORTS.page}`,
    plugins: [serveStatic('/web-build/', dist), homeRealm()]
  })
)
