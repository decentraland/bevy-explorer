import { defineConfig, devices } from '@playwright/test'
import { GATE_PORTS } from './e2e/ports'

// The scene editor's gates, in the REAL engine (headed, like tier 2); e2e/README.md has the setup.
//   preview-realm: a scene built in the browser, served by the service worker and hot-reloaded
//   editor:        what the page owes the editor package: Create, preview, its privileged scene,
//                  reload by id, Play/Stop without restarting the HUD scene, leaving
//   WEB_EDITOR_DIR=<dcl-editor checkout> npx playwright test --config playwright.gate.config.ts [name]
// every port comes from e2e/ports.ts; a bridge scene of its own, off the everyday dev :8100, that
// the specs pass as ?bridgePort (e2e/gate.ts)
const { page: PORT, bridge: BRIDGE_PORT, service: SERVICE_PORT } = GATE_PORTS
const EDITOR = JSON.stringify(process.env.WEB_EDITOR_DIR ?? '')
// starts empty on every run; per port set, so a run beside it in this checkout keeps its own
const PROJECTS_DATA = `.vite/gate-projects-${SERVICE_PORT}`

export default defineConfig({
  testDir: './e2e',
  testMatch: '**/*.gate.spec.ts',
  outputDir: `test-results/gate-${PORT}`,
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
      command: `rm -rf ${PROJECTS_DATA} && node ${EDITOR}/packages/service/dev-server.ts --port ${SERVICE_PORT} --data ${PROJECTS_DATA} --origin http://localhost:${PORT}`,
      url: `http://127.0.0.1:${SERVICE_PORT}/health`,
      reuseExistingServer: false,
      timeout: 60_000
    }
  ]
})
