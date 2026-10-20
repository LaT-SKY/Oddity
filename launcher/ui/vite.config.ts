import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(
      JSON.parse(
        readFileSync(new URL("./package.json", import.meta.url), "utf8"),
      ).version,
    ),
  },
  plugins: [vue({ template: { transformAssetUrls: false } })],
  base: "./",
  publicDir: "public",
  clearScreen: false,
  build: { target: "es2022" },
});
