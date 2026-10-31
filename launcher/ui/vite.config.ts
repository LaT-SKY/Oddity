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
  build: {
    target: "es2022",
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: "motion",
              test: /node_modules\/.*(?:motion-dom|motion-v|motion-utils|framer-motion)/,
            },
            { name: "vue", test: /node_modules\/.*(?:@vue|vue@)/ },
            { name: "animation", test: /node_modules\/.*animejs/ },
          ],
        },
      },
    },
  },
});
