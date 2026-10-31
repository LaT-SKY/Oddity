import { build } from "esbuild";
await build({
  entryPoints: ["electron/main.ts", "electron/preload.ts"],
  outdir: "desktop-dist",
  outExtension: { ".js": ".cjs" },
  bundle: true,
  platform: "node",
  target: "node24",
  external: ["electron"],
  sourcemap: true,
});
