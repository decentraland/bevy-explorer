// Dev server for the preview-realm gate (playwright.gate.config.ts): the app's own config plus
// the editor's in-browser build tool, served same-origin under /web-build/.
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { defineConfig, mergeConfig } from 'vite'
import base, { serveStatic } from '../vite.config.ts'

const dist = join(process.env.WEB_EDITOR_DIR ?? '', 'packages/web-build/dist')
if (!existsSync(join(dist, 'index.js'))) {
  throw new Error(`WEB_EDITOR_DIR must be a dcl-editor checkout with web-build built (no ${dist}/index.js)`)
}

export default defineConfig((env) =>
  mergeConfig(base(env), {
    // its own dep cache: this server can run next to the everyday dev server
    cacheDir: '.vite/gate',
    plugins: [serveStatic('/web-build/', dist)]
  })
)
