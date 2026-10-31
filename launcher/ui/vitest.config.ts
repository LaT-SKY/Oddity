import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config.ts";

export default mergeConfig(
  viteConfig,
  defineConfig({
    resolve: {
      dedupe: ["vue", "vitest", "@vue/test-utils", "animejs"],
    },
    server: { fs: { allow: [new URL("../..", import.meta.url).pathname] } },
    test: {
      environment: "jsdom",
      include: [
        "tests/unit/**/*.test.ts",
        "../../internal/gamepad/acceptance/*.test.ts",
      ],
      setupFiles: ["./tests/unit/setup.ts"],
      coverage: {
        provider: "v8",
        include: ["src/**/*.ts", "src/**/*.vue"],
        exclude: ["src/types/**"],
        reporter: ["text", "html", "lcov", "json-summary"],
        reportOnFailure: true,
        thresholds: {
          // DOM 动效按 UI 参考目标统计, 核心逻辑仍逐文件强制检查。
          "src/**/!(useNotificationArrival|usePlayerMotion|useScenePalette).ts":
            {
              lines: 80,
              branches: 70,
              perFile: true,
            },
        },
      },
    },
  }),
);
