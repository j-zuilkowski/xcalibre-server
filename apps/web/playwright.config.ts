import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  globalSetup: "./e2e/global-setup.ts",
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  timeout: 30_000,
  expect: {
    timeout: 10_000,
  },
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:5173",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: [
    {
      command:
        "rm -f test_e2e.db && rm -rf storage_e2e && env APP_BIND_ADDR=127.0.0.1:8083 XCS_DISABLE_METRICS=1 APP_DATABASE_URL=sqlite://test_e2e.db APP_LLM_ENABLED=false APP_STORAGE_PATH=./storage_e2e APP_AUTH_RATE_LIMIT_PER_MINUTE=10000 cargo run -p backend",
      url: "http://localhost:8083/health",
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
      cwd: "../..",
    },
    {
      command: "pnpm --filter @xs/web dev",
      url: "http://localhost:5173",
      reuseExistingServer: !process.env.CI,
      cwd: "../..",
    },
  ],
});
