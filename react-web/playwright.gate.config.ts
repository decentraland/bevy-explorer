import { defineConfig, devices } from '@playwright/test'

// The scene editor's gates, in the REAL engine (headed, like tier 2); e2e/README.md has the setup.
//   preview-realm: a scene built in the browser, served by the service worker and hot-reloaded
//   editor:        the editor package opened from Create: create, edit, Play, Stop, Exit, back in
//   sync:          one wallet on two devices against the project storage service, and what it refuses
//   publish:       a scene published to a local Worlds server, then entered by the engine
//   npm:           an imported scene whose code needs a package from npm
//   WEB_EDITOR_DIR=<dcl-editor checkout> npx playwright test --config playwright.gate.config.ts [name]
const PORT = Number(process.env.GATE_PORT ?? 5230)
// a bridge scene of its own, off the everyday dev :8100; the specs pass it as ?bridgePort (e2e/gate.ts)
const BRIDGE_PORT = Number(process.env.GATE_BRIDGE_PORT ?? 8110)
process.env.GATE_BRIDGE_PORT = String(BRIDGE_PORT)
const EDITOR = JSON.stringify(process.env.WEB_EDITOR_DIR ?? '')
// starts empty on every run
const PROJECTS_DATA = '.vite/gate-projects'

export default defineConfig({
  testDir: './e2e',
  testMatch: '**/*.gate.spec.ts',
  workers: 1,
  retries: 0,
  timeout: 600_000,
  reporter: [['list']],
  use: {
    baseURL: `http://localhost:${PORT}`,
    headless: false,
    launchOptions: {
      args: [
        '--enable-unsafe-webgpu',
        '--ignore-gpu-blocklist',
        '--enable-features=Vulkan',
        // the engine must keep running while the window is hidden behind others
        '--disable-backgrounding-occluded-windows',
        '--disable-renderer-backgrounding',
        '--disable-background-timer-throttling'
      ]
    }
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      command: `npx vite --config e2e/vite.gate.config.ts --configLoader native --port ${PORT} --strictPort`,
      url: `http://localhost:${PORT}`,
      // the bridge scene is the webServer below, not vite's own on :8100
      env: { BRIDGE_SCENE_PREVIEW: '0' },
      reuseExistingServer: false,
      timeout: 120_000
    },
    {
      // built, then served: `start` alone runs npm install over a node_modules that is a symlink
      command:
        `npx --no-install sdk-commands build --skip-install && npx --no-install sdk-commands start --no-client --skip-build --no-watch --port ${BRIDGE_PORT}`,
      cwd: 'bridge-scene',
      url: `http://127.0.0.1:${BRIDGE_PORT}/about`,
      reuseExistingServer: false,
      timeout: 120_000
    },
    {
      // the editor's project storage service (services.projects on a loopback page)
      command: `rm -rf ${PROJECTS_DATA} && node ${EDITOR}/packages/service/dev-server.ts --port 8787 --data ${PROJECTS_DATA} --origin http://localhost:${PORT}`,
      url: 'http://127.0.0.1:8787/health',
      reuseExistingServer: false,
      timeout: 60_000
    },
    {
      // a Worlds content server in memory; `gate.eth` goes to the first wallet that deploys to it
      command: `node ${EDITOR}/packages/web/validate/fake-worlds-server.mjs --port 8799 --claim gate.eth`,
      url: 'http://127.0.0.1:8799/status',
      reuseExistingServer: false,
      timeout: 60_000
    }
  ]
})
