import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: 'e2e',
  webServer: {
    command: 'npx vite --port 5179 --strictPort',
    url: 'http://localhost:5179',
    reuseExistingServer: true,
  },
  use: {
    baseURL: 'http://localhost:5179',
    viewport: { width: 880, height: 640 },
    deviceScaleFactor: 2,
    reducedMotion: 'reduce',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 880, height: 640 }, deviceScaleFactor: 2 } }],
});
