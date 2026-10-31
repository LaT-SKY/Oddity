import { test, expect } from "@playwright/test";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);

test("启动加载期间退出不会弹出启动失败", async () => {
  test.skip(
    Boolean(process.env.ODDITY_TEST_EXECUTABLE),
    "入口注入使用开发构建验证",
  );
  const temporary = await mkdtemp(join(tmpdir(), "oddity-startup-"));
  await mkdir(join(temporary, "config"));
  await mkdir(join(temporary, "data"));
  const report = join(temporary, "events.jsonl");
  const entry = join(temporary, "entry.cjs");
  await writeFile(
    entry,
    `
    const {app,BrowserWindow,dialog} = require('electron');
    const fs = require('node:fs');
    const record = event => fs.appendFileSync(${JSON.stringify(report)}, JSON.stringify(event)+'\\n');
    dialog.showErrorBox = (...args) => record({dialog:args});
    const load = BrowserWindow.prototype.loadURL;
    BrowserWindow.prototype.loadURL = function(...args) {
      const pending = load.apply(this,args);
      pending.catch(error => record({cancelled:error.code}));
      record({loading:true});
      setImmediate(() => app.quit());
      return pending;
    };
    require(${JSON.stringify(resolve("desktop-dist/main.cjs"))});
  `,
  );
  const child = spawn(require("electron") as string, [entry], {
    env: {
      ...process.env,
      HOME: temporary,
      XDG_CONFIG_HOME: join(temporary, "config"),
      XDG_DATA_HOME: join(temporary, "data"),
    },
    stdio: ["ignore", "ignore", "pipe"],
    timeout: 15000,
    killSignal: "SIGKILL",
  });
  let stderr = "";
  child.stderr.on("data", (chunk) => {
    stderr += String(chunk);
  });
  try {
    const code = await new Promise<number | null>((resolve, reject) => {
      child.on("error", reject);
      child.on("exit", resolve);
    });
    expect(code, stderr).toBe(0);
    const events = (await readFile(report, "utf8"))
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line));
    expect(events.some((event) => event.loading)).toBe(true);
    expect(events.some((event) => event.cancelled)).toBe(true);
    expect(events.filter((event) => event.dialog)).toEqual([]);
  } finally {
    if (child.exitCode === null) child.kill("SIGKILL");
    await rm(temporary, { recursive: true, force: true });
  }
});
