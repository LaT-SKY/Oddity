import { defineConfig } from "@playwright/test";

const port = process.env.ODDITY_PREVIEW_PORT || "4173";
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "./tests/e2e",
  testMatch: "**/*.spec.ts",
  use: {
    baseURL,
    viewport: { width: 1180, height: 760 },
  },
  webServer: {
    command: `pnpm run preview --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
  },
});
