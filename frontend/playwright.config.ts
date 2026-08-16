import { defineConfig, devices } from '@playwright/test';

// WSL mirrored networking can forward 127.0.0.1 to Windows instead of the guest.
const previewHost = process.env.PIXIV_PLAYWRIGHT_HOST ?? '127.0.0.2';
const previewPort = process.env.PIXIV_PLAYWRIGHT_PORT ?? '41730';
const previewUrl = `http://${previewHost}:${previewPort}`;

export default defineConfig({
  testDir: './tests',
  timeout: 60_000,
  fullyParallel: true,
  workers: 4,
  reporter: 'list',
  use: {
    baseURL: previewUrl,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure'
  },
  projects: [
    {
      name: 'chrome',
      use: { ...devices['Desktop Chrome'], channel: 'chrome' }
    },
    {
      name: 'edge',
      use: { ...devices['Desktop Edge'], channel: 'msedge' }
    },
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'] }
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'] }
    }
  ],
  webServer: {
    command: `pnpm preview --host ${previewHost} --port ${previewPort} --strictPort`,
    url: previewUrl,
    reuseExistingServer: false,
    timeout: 120_000
  }
});
