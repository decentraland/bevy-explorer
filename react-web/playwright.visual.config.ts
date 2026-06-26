import { defineConfig, devices } from '@playwright/test'

// Tier 1.5 — VISUAL REGRESSION against the mock HUD (`?mock=1`): no engine, no GPU, no bridge
// scene, so it runs HEADLESS and deterministically. It renders every DOM domain with fixed mock
// data, a frozen clock, stubbed external images and disabled animations, then diffs each panel
// against a committed baseline PNG. This is the "screenshot everything + compare" safety net; the
// real-engine round-trips live in playwright.config.ts (tier 2). See e2e/README.md + review.md.
export default defineConfig({
  testDir: './e2e',
  testMatch: '**/visual.spec.ts',
  fullyParallel: true,
  workers: process.env.CI ? 1 : undefined,
  retries: 0,
  timeout: 60_000,
  expect: {
    timeout: 15_000,
    // Small tolerance for sub-pixel antialiasing/font-hinting differences; a real layout/colour
    // regression moves far more than this.
    toHaveScreenshot: { maxDiffPixelRatio: 0.01, animations: 'disabled', scale: 'css' }
  },
  reporter: [['list'], ['html', { open: 'never' }]],
  // One canonical viewport so baselines are stable. (HUD --ui-scale is viewport-relative.)
  use: {
    baseURL: process.env.E2E_URL ?? 'http://localhost:5173',
    headless: true,
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
    trace: 'retain-on-failure'
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 1600, height: 900 } } }],
  // Mock mode needs only the Vite dev server (no engine, no :8100 bridge).
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:5173',
    reuseExistingServer: true,
    timeout: 120_000
  }
})
