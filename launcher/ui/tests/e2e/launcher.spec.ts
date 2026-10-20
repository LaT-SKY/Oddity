import { test, expect } from "@playwright/test";

test("导航、更新、设置、快捷键与启动关闭流程", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("response", (response) => {
    if (response.status() >= 400)
      errors.push(`${response.status()} ${response.url()}`);
  });
  await page.goto("/");
  await expect(page.locator("#launch-btn")).toHaveText(/启动游戏/);
  await expect(page.locator("#app")).toHaveClass(/is-intro-done/);
  await page.locator('[data-page="update"]').click();
  await expect(page.locator("#page-update")).toBeVisible();
  await expect(page.locator("#update-state-title")).toHaveText("已是最新");
  await page.locator("#update-full").click();
  await expect(page.locator("#update-state-title")).toHaveText("正在检查");
  await expect(page.locator("#update-state-title")).toHaveText("已是最新");
  await page.locator('[data-page="bulletin"]').click();
  await expect(page.locator("#page-bulletin")).toBeVisible();
  await page.keyboard.press("Control+4");
  await expect(page.locator("#page-settings")).toBeVisible();
  await expect(page.locator(".check-row")).toHaveCount(6);
  await page.locator("#set-theme").selectOption("dark");
  await expect(page.locator("#app")).toHaveAttribute("data-theme", "dark");
  await page.locator("#set-hero-media").selectOption("photo");
  await page.locator(".switch-row").click();
  await expect(page.locator("#set-motion")).toBeChecked();
  await expect(page.locator("#app")).toHaveClass(/reduce-motion/);
  await page.locator("#set-install").fill("/tmp/oddity-game");
  await page.keyboard.press("Control+s");
  await expect(page.locator("#toast")).toHaveText("设置已保存。");
  await page.locator("#settings-redetect").click();
  await expect(page.locator("#set-install")).toHaveValue("/tmp/oddity-game");
  await page.keyboard.press("Control+1");
  await expect(page.locator("#page-home")).toBeVisible();
  await page.locator("#launch-btn").click();
  await expect(page.locator("#launch-btn")).toHaveText(/正在启动/);
  await expect(page.locator("#launch-btn")).toHaveText(/关闭游戏/);
  await page.locator("#launch-btn").click();
  await expect(page.locator("#launch-btn")).toHaveText(/正在关闭/);
  await expect(page.locator("#launch-btn")).toHaveText(/启动游戏/);
  expect(errors).toEqual([]);
});

test("深链接与减少动态效果的首次加载", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/#settings");
  await expect(page.locator("#page-settings")).toBeVisible();
  await expect(page.locator('[data-page="settings"]')).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.locator("#page-home")).toBeHidden();
  await expect(page.locator("#hero-canvas")).not.toHaveClass(/is-visible/);
});

test("视频长时间循环无黑帧, 离页后从头播放", async ({ page }) => {
  test.setTimeout(240_000);
  await page.goto("/");
  const video = page.locator("#hero-video");
  const canvas = page.locator("#hero-canvas");
  await expect(canvas).toHaveClass(/is-visible/);
  await expect
    .poll(() => video.evaluate((v: HTMLVideoElement) => v.duration))
    .toBeCloseTo(13, 2);
  await page.evaluate(() => {
    const source = document.querySelector<HTMLVideoElement>("#hero-video")!;
    const display = document.querySelector<HTMLCanvasElement>("#hero-canvas")!;
    const monitor = document.createElement("div");
    monitor.id = "video-probe";
    monitor.dataset.loops = "0";
    monitor.dataset.blackFrames = "0";
    document.body.append(monitor);
    source.addEventListener("seeking", () => {
      monitor.dataset.loops = String(Number(monitor.dataset.loops) + 1);
    });
    setInterval(() => {
      if (!display.classList.contains("is-visible")) return;
      const pixel = display
        .getContext("2d")!
        .getImageData(display.width / 2, display.height / 2, 1, 1).data;
      if (pixel[0] + pixel[1] + pixel[2] < 5)
        monitor.dataset.blackFrames = String(
          Number(monitor.dataset.blackFrames) + 1,
        );
    }, 33);
  });
  const probe = page.locator("#video-probe");
  for (const cycles of [5, 3]) {
    const loops = Number(await probe.getAttribute("data-loops"));
    await expect
      .poll(async () => Number(await probe.getAttribute("data-loops")), {
        timeout: cycles * 15_000,
      })
      .toBeGreaterThanOrEqual(loops + cycles);
    await expect
      .poll(() => video.evaluate((v: HTMLVideoElement) => v.currentTime), {
        timeout: 12_000,
      })
      .toBeGreaterThan(5);
    await expect(probe).toHaveAttribute("data-black-frames", "0");
    await page.locator('[data-page="settings"]').click();
    await expect(canvas).not.toHaveClass(/is-visible/);
    await expect
      .poll(() =>
        video.evaluate(
          (v: HTMLVideoElement) => v.paused && v.currentTime === 0,
        ),
      )
      .toBe(true);
    await page.waitForTimeout(15_000);
    await expect
      .poll(() => video.evaluate((v: HTMLVideoElement) => v.currentTime))
      .toBe(0);
    await page.locator('[data-page="home"]').click();
    await expect(canvas).toHaveClass(/is-visible/);
    expect(
      await video.evaluate((v: HTMLVideoElement) => v.currentTime),
    ).toBeLessThan(1);
  }
  const loops = Number(await probe.getAttribute("data-loops"));
  await expect
    .poll(async () => Number(await probe.getAttribute("data-loops")), {
      timeout: 45_000,
    })
    .toBeGreaterThanOrEqual(loops + 3);
  await expect(probe).toHaveAttribute("data-black-frames", "0");
  await page.locator('[data-page="settings"]').click();
  await page.locator("#set-hero-media").selectOption("photo");
  await expect(canvas).not.toHaveClass(/is-visible/);
  await page.locator("#set-hero-media").selectOption("video");
  await expect
    .poll(() => video.evaluate((v: HTMLVideoElement) => v.paused))
    .toBe(true);
  await page.locator('[data-page="home"]').click();
  await expect(canvas).toHaveClass(/is-visible/);
});
