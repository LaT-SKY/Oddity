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
  await expect(page.locator("#hero-video-a")).not.toHaveClass(/is-visible/);
});

test("主页视频循环与静态照片切换", async ({ page }) => {
  await page.goto("/");
  const first = page.locator("#hero-video-a");
  const second = page.locator("#hero-video-b");
  await expect(first).toHaveClass(/is-front/);
  await expect
    .poll(() => first.evaluate((video: HTMLVideoElement) => video.currentTime))
    .toBeGreaterThan(0);
  await first.dispatchEvent("ended");
  await expect(second).toHaveClass(/is-front/);
  await expect(first).not.toHaveClass(/is-visible/);
  await page.locator('[data-page="settings"]').click();
  await page.locator("#set-hero-media").selectOption("photo");
  await expect(second).not.toHaveClass(/is-visible/);
  await expect
    .poll(() => second.evaluate((video: HTMLVideoElement) => video.paused))
    .toBe(true);
  await page.locator("#set-hero-media").selectOption("video");
  await expect(first).toHaveClass(/is-front/);
});
