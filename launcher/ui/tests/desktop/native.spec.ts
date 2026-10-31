import { test, expect, _electron as electron } from "@playwright/test";
import {
  mkdtemp,
  mkdir,
  rm,
  writeFile,
  readdir,
  readFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { execFileSync, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
test("真实 Rust 命令、窗口、导航、手柄与退出", async ({}, info) => {
  const temporary = await mkdtemp(join(tmpdir(), "oddity-electron-"));
  await mkdir(join(temporary, "config", "oddity"), { recursive: true });
  await writeFile(
    join(temporary, "config", "oddity", "settings.json"),
    JSON.stringify({ theme: "dark", reduceMotion: false }),
  );
  await mkdir(join(temporary, "data/oddity/logs"), { recursive: true });
  for (let n = 1; n <= 9; n++)
    await writeFile(
      join(temporary, `data/oddity/logs/oddity-startup-${n}-999.log`),
      "old",
    );
  await writeFile(join(temporary, "data/oddity/logs/unrelated.log"), "keep");
  const executablePath =
    process.env.ODDITY_TEST_EXECUTABLE || (require("electron") as string);
  const args = process.env.ODDITY_TEST_EXECUTABLE ? [] : [resolve(".")];
  const env = {
    ...process.env,
    HOME: temporary,
    XDG_CONFIG_HOME: join(temporary, "config"),
    XDG_DATA_HOME: join(temporary, "data"),
  };
  const app = await electron.launch({ executablePath, args, env });
  const stderr: string[] = [];
  app.process().stderr?.on("data", (chunk) => stderr.push(String(chunk)));
  let backendPid = 0;
  try {
    const page = await app.firstWindow();
    page.on("console", (message) =>
      stderr.push("RENDERER " + message.type() + " " + message.text() + "\n"),
    );
    page.on("requestfailed", (request) =>
      stderr.push(
        "REQUEST " +
          request.url() +
          " " +
          JSON.stringify(request.failure()) +
          "\n",
      ),
    );
    await page.waitForURL("oddity://app/**");
    await expect(page.locator("#app")).toBeVisible();
    const logs = await readdir(join(temporary, "data/oddity/logs"));
    expect(
      logs.filter((name) => name.startsWith("oddity-startup-")).length,
    ).toBe(5);
    expect(
      await readFile(join(temporary, "data/oddity/logs/unrelated.log"), "utf8"),
    ).toBe("keep");
    const security = await page.evaluate(async () => {
      const response = await fetch("oddity://app/index.html");
      const script = document.createElement("script");
      script.textContent = "window.__cspProbe=true";
      document.head.append(script);
      script.remove();
      return {
        csp: response.headers.get("Content-Security-Policy"),
        executed: Reflect.get(window, "__cspProbe") === true,
      };
    });
    expect(security.csp).toContain("script-src 'self'");
    expect(security.executed).toBe(false);
    const bootstrap = await page.evaluate(() =>
      window.oddity!.invoke("bootstrap"),
    );
    expect(bootstrap.os).not.toContain("browser preview");
    expect(bootstrap.settings.theme).toBe("dark");
    await expect(page.locator(".app")).toHaveAttribute("data-theme", "dark");
    expect(await page.evaluate(() => localStorage.getItem("nte.theme"))).toBe(
      "dark",
    );
    expect(bootstrap.settings.runtimeEnvironments).toEqual([]);
    const latency = await page.evaluate(async () => {
      const samples: number[] = [];
      for (let i = 0; i < 100; i++) {
        const start = performance.now();
        await window.oddity!.invoke("gamepad_poll");
        samples.push(performance.now() - start);
      }
      samples.sort((a, b) => a - b);
      return { median: samples[50], p95: samples[95], max: samples[99] };
    });
    await info.attach("backend-roundtrip-ms", {
      body: JSON.stringify(latency),
      contentType: "application/json",
    });
    await expect(page.locator("#hero-canvas")).toHaveClass(/is-visible/);
    await expect
      .poll(() =>
        page.evaluate(() =>
          [
            ...document.querySelectorAll<HTMLVideoElement>(
              "video.hero__source",
            ),
          ].some((video) => video.currentTime > 0.1 && video.readyState >= 2),
        ),
      )
      .toBe(true);
    const mainPid = await app.evaluate(() => process.pid);
    const children = execFileSync("pgrep", ["-a", "-P", String(mainPid)], {
      encoding: "utf8",
    });
    backendPid = Number(
      children
        .split("\n")
        .find((line) => line.includes("oddity-backend"))!
        .split(" ")[0],
    );
    expect(backendPid).toBeGreaterThan(0);
    const install = join(temporary, "异环 Test");
    const background = join(
      install,
      "NTELauncher",
      "ResFilesM",
      "1289",
      "bgimgs",
    );
    await mkdir(background, { recursive: true });
    await mkdir(join(install, "NTELauncher", "Config"));
    await writeFile(
      join(install, "NTELauncher", "Config", "Config.ini"),
      "[General]\nGameID=1289\n",
    );
    await writeFile(
      join(background, "bg.mp4"),
      Buffer.from(Array.from({ length: 256 }, (_, i) => i)),
    );
    await page.evaluate(async (installRoot) => {
      const data = await window.oddity!.invoke("bootstrap");
      await window.oddity!.invoke("save_settings", {
        settings: {
          ...data.settings,
          installRoot,
          theme: "dark",
          heroMedia: "photo",
        },
      });
    }, install);
    const media = await page.evaluate(async () => {
      const media = await window.oddity!.invoke("launcher_background");
      const response = await fetch(media.video!, {
        headers: { Range: "bytes=10-19" },
      });
      return {
        headers: Object.fromEntries(response.headers),
        url: media.video,
        status: response.status,
        bytes: [...new Uint8Array(await response.arrayBuffer())],
      };
    });
    expect(media).toMatchObject({
      url: expect.stringMatching(/^oddity:\/\/media\/video\?v=\d+$/),
      status: 206,
      bytes: [10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
    });
    const head = await page.evaluate(async () => {
      const source = await window.oddity!.invoke("launcher_background");
      const response = await fetch(source.video!, { method: "HEAD" });
      return {
        status: response.status,
        length: response.headers.get("Content-Length"),
        bytes: (await response.arrayBuffer()).byteLength,
      };
    });
    expect(head).toEqual({ status: 200, length: "256", bytes: 0 });
    const suffix = await page.evaluate(async () => {
      const source = await window.oddity!.invoke("launcher_background");
      const response = await fetch(source.video!, {
        headers: { Range: "bytes=-3" },
      });
      return {
        status: response.status,
        range: response.headers.get("Content-Range"),
        bytes: [...new Uint8Array(await response.arrayBuffer())],
      };
    });
    expect(suffix).toEqual({
      status: 206,
      range: "bytes 253-255/256",
      bytes: [253, 254, 255],
    });
    const invalid = await page.evaluate(async () => {
      try {
        await window.oddity!.invoke("save_settings", {
          settings: false,
        } as never);
      } catch (error) {
        return error;
      }
    });
    expect(invalid).toContain("invalid type");
    const snapshot = await page.evaluate(() =>
      window.oddity!.invoke("gamepad_poll"),
    );
    expect(Array.isArray(snapshot.devices)).toBe(true);
    for (const name of [
      "update",
      "game-mode",
      "bulletin",
      "records",
      "settings",
      "home",
    ]) {
      await page.locator(`[data-page="${name}"]`).click();
      await expect(page).toHaveURL(new RegExp(`#${name}$`));
    }
    const failure = await page.evaluate(async () => {
      const data = await window.oddity!.invoke("bootstrap");
      await window.oddity!.invoke("save_settings", {
        settings: { ...data.settings, distribution: "global" },
      });
      try {
        await window.oddity!.invoke("attendance_run");
      } catch (error) {
        return error;
      }
    });
    expect(failure).toMatchObject({
      kind: "service",
      message: expect.stringContaining("仅支持国服"),
    });
    await page.evaluate(() => window.oddity!.invoke("win_toggle_maximize"));
    await expect
      .poll(() => page.evaluate(() => window.oddity!.windowState()))
      .toEqual({ maximized: true, visible: true });
    await page.evaluate(() => window.oddity!.invoke("win_toggle_maximize"));
    await expect
      .poll(() => page.evaluate(() => window.oddity!.windowState()))
      .toEqual({ maximized: false, visible: true });
    await page.evaluate(() => window.oddity!.invoke("win_close"));
    expect(
      await app.evaluate(({ BrowserWindow }) =>
        BrowserWindow.getAllWindows()[0]!.isVisible(),
      ),
    ).toBe(false);
    const second = spawn(executablePath, args, { env, stdio: "ignore" });
    await new Promise<void>((resolve, reject) => {
      second.on("error", reject);
      second.on("exit", (code) =>
        code === 0 ? resolve() : reject(new Error(`second instance: ${code}`)),
      );
    });
    await expect
      .poll(() =>
        app.evaluate(({ BrowserWindow }) =>
          BrowserWindow.getAllWindows()[0]!.isVisible(),
        ),
      )
      .toBe(true);
    await expect(page.locator("#app")).toBeVisible();
  } finally {
    await app.close();
    await info.attach("electron-stderr", {
      body: stderr.join(""),
      contentType: "text/plain",
    });
    if (backendPid)
      await expect.poll(() => existsSync(`/proc/${backendPid}`)).toBe(false);
    await rm(temporary, { recursive: true, force: true });
  }
});
