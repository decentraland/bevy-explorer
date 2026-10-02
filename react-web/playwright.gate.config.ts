import { defineConfig, devices } from '@playwright/test'

// The preview-realm gate (e2e/preview-realm.gate.spec.ts): a scene built in the browser, served
// by the service worker's preview realm and hot-reloaded in the REAL engine. Headed, like tier 2.
//   WEB_EDITOR_DIR=<dcl-editor checkout, web-build built> npx playwright test --config playwright.gate.config.ts
const PORT = Number(process.env.GATE_PORT ?? 5230)

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
      // the bridge scene on :8100 is the webServer below (see playwright.config.ts)
      env: { BRIDGE_SCENE_PREVIEW: '0' },
      reuseExistingServer: false,
      timeout: 120_000
    },
    {
      // built, then served: `start` alone runs npm install over a node_modules that is a symlink
      command:
        'npx --no-install sdk-commands build --skip-install && npx --no-install sdk-commands start --no-client --skip-build --no-watch --port 8100',
      cwd: 'bridge-scene',
      url: 'http://127.0.0.1:8100/about',
      reuseExistingServer: true,
      timeout: 120_000
    }
  ]
})
