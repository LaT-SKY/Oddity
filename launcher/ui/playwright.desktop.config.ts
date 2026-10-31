import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests/desktop",
  outputDir: "./test-results-desktop",
  workers: 1,
  timeout: 45000,
  use: { trace: "retain-on-failure" },
});
