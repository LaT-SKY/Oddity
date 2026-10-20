import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  testMatch: "**/*.spec.ts",
  use: {
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1180, height: 760 },
  },
  webServer: {
    command: "pnpm run preview --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: !process.env.CI,
  },
});
