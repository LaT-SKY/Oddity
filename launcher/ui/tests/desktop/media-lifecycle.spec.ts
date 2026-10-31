import { test, expect } from "@playwright/test";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);

test("原生隐藏、最小化和减少动效释放解码器并恢复播放", async () => {
  test.skip(
    Boolean(process.env.ODDITY_TEST_EXECUTABLE),
    "独立入口验证原生可见性, 避免自动化焦点模拟影响",
  );
  const temporary = await mkdtemp(join(tmpdir(), "oddity-media-"));
  await mkdir(join(temporary, "config/oddity"), { recursive: true });
  await mkdir(join(temporary, "data"));
  await writeFile(
    join(temporary, "config/oddity/settings.json"),
    JSON.stringify({
      theme: "dark",
      reduceMotion: false,
      installRoot: join(temporary, "game"),
    }),
  );
  const report = join(temporary, "media.json");
  const child = spawn(
    require("electron") as string,
    [resolve("tests/desktop/media-lifecycle.cjs")],
    {
      env: {
        ...process.env,
        HOME: temporary,
        XDG_CONFIG_HOME: join(temporary, "config"),
        XDG_DATA_HOME: join(temporary, "data"),
        ODDITY_TEST_MAIN: resolve("desktop-dist/main.cjs"),
        ODDITY_TEST_REPORT: report,
      },
      stdio: ["ignore", "ignore", "pipe"],
      timeout: 35000,
      killSignal: "SIGKILL",
    },
  );
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
    const samples: {
      stage: string;
      hidden: boolean;
      visible: boolean;
      videos: {
        src: string | null;
        ready: number;
        paused: boolean;
        time: number;
      }[];
    }[] = JSON.parse(await readFile(report, "utf8"));
    await test.info().attach("media-stages", {
      body: JSON.stringify(samples),
      contentType: "application/json",
    });
    expect(samples.map((s) => s.stage)).toEqual([
      "playing",
      "hidden",
      "visible",
      "minimized",
      "restored",
      "reduce-motion",
      "motion-restored",
    ]);
    for (const sample of samples) {
      if (["hidden", "minimized", "reduce-motion"].includes(sample.stage)) {
        expect(
          sample.videos.every((v) => v.src === null && v.ready === 0),
          sample.stage,
        ).toBe(true);
        if (sample.stage !== "reduce-motion")
          expect(sample.visible).toBe(false);
        if (sample.stage === "hidden") expect(sample.hidden).toBe(true);
      } else
        expect(
          sample.videos.some((v) => !v.paused && v.time > 0.1),
          sample.stage,
        ).toBe(true);
    }
  } finally {
    if (child.exitCode === null) child.kill("SIGKILL");
    await rm(temporary, { recursive: true, force: true });
  }
});
