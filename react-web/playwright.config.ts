import { defineConfig, devices } from '@playwright/test'

// Tier 2 real-engine e2e. The bevy engine needs WebGPU + cross-origin isolation, so
// this runs HEADED against a real GPU (no headless) — like dcl-editor's validate.mjs.
// Boots two servers: the Vite dev server (engine in an iframe) and the super-user
// bridge scene on :8100. See e2e/README.md.
export default defineConfig({
  testDir: './e2e',
  testIgnore: '**/visual.spec.ts', // tier 1.5 visual regression runs via playwright.visual.config.ts
  fullyParallel: false,
  workers: 1, // the engine is heavy — one world at a time
  retries: 0,
  timeout: 180_000,
  expect: { timeout: 30_000 },
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: process.env.E2E_URL ?? 'http://localhost:5173',
    headless: false, // WebGPU requires a real GPU context
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    launchOptions: {
      args: ['--enable-unsafe-webgpu', '--ignore-gpu-blocklist', '--enable-features=Vulkan']
    }
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      command: 'npm run dev',
      url: 'http://localhost:5173',
      reuseExistingServer: true,
      timeout: 120_000
    },
    {
      command: 'npx sdk-commands start --no-browser --port 8100',
      cwd: 'bridge-scene',
      url: 'http://localhost:8100',
      reuseExistingServer: true,
      timeout: 120_000
    }
  ]
})
