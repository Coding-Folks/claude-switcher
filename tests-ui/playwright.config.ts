import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  reporter: 'line',
  use: {
    channel: 'msedge',                 // installed Edge; no browser download on the full C: drive
    viewport: { width: 320, height: 480 },
    baseURL: 'http://127.0.0.1:4317',
  },
  webServer: {
    command: 'node serve.mjs',
    url: 'http://127.0.0.1:4317/index.html',
    reuseExistingServer: true,
  },
});
