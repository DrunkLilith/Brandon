import { defineConfig, devices } from '@playwright/test'

import { artifactDirectory } from './tests/e2e/support/artifacts'

export default defineConfig({
  testDir: './tests/e2e',
  testIgnore: '**/support/**/*.test.ts',
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  expect: { timeout: 15_000 },
  globalSetup: './tests/e2e/support/global-setup.ts',
  globalTeardown: './tests/e2e/support/global-teardown.ts',
  outputDir: artifactDirectory(),
  reporter: [['list']],
  use: {
    ...devices['Desktop Chrome'],
    channel: process.env.CI ? undefined : 'chrome',
    trace: 'off',
    screenshot: 'off',
    video: 'off',
  },
  projects: [
    { name: 'auth', testMatch: 'auth.spec.ts' },
    { name: 'e2e', testIgnore: 'auth.spec.ts', dependencies: ['auth'] },
  ],
})
