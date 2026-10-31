import { createServer } from "vite";
import { createRequire } from "node:module";
import { spawn } from "node:child_process";
import "./build.mjs";
const server = await createServer({
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  plugins: [
    {
      name: "desktop-dev-csp",
      transformIndexHtml(html) {
        return html.replace(
          "connect-src 'self'",
          "connect-src 'self' ws://127.0.0.1:1420",
        );
      },
    },
  ],
});
await server.listen();
const child = spawn(
  createRequire(import.meta.url)("electron"),
  [".", ...process.argv.slice(2)],
  {
    stdio: "inherit",
    env: { ...process.env, ODDITY_DEV_SERVER: "http://127.0.0.1:1420/" },
  },
);
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => child.kill(signal));
child.once("error", async (error) => {
  console.error(error);
  await server.close();
  process.exitCode = 1;
});
child.once("exit", async (code) => {
  await server.close();
  process.exitCode = code ?? 1;
});
