import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config.ts";

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      include: ["tests/unit/**/*.test.ts"],
      setupFiles: ["./tests/unit/setup.ts"],
      coverage: {
        provider: "v8",
        include: ["src/**/*.ts", "src/**/*.vue"],
        exclude: ["src/types/**"],
        reporter: ["text", "html", "lcov", "json-summary"],
        reportOnFailure: true,
        thresholds: {
          "src/**/*.ts": { lines: 80, branches: 70, perFile: true },
        },
      },
    },
  }),
);
