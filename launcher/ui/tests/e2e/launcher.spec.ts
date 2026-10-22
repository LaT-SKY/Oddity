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
  await page.locator(".switch-row").first().click();
  await expect(page.locator("#set-motion")).toBeChecked();
  await expect(page.locator("#set-minimize-to-tray")).toBeChecked();
  await expect(page.locator("#app")).toHaveClass(/reduce-motion/);
  await page.locator("#set-install").fill("/tmp/oddity-game");
  await page.keyboard.press("Control+s");
  await expect(page.locator("#toast")).toHaveText("设置已保存.");
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

test("多账号签到、统一保存和账号管理", async ({ page }) => {
  await page.goto("/#settings");
  const panel = page.locator(".attendance");
  const action = page.locator("#attendance-action");
  await expect(action).toHaveText("立即登录");
  await expect(page.locator("#attendance-enabled")).toBeDisabled();
  await action.click();
  const dialog = page.getByRole("dialog");
  async function login(phone: string) {
    await dialog.locator("#attendance-phone").fill(phone);
    await dialog.locator("#attendance-password").fill("preview-password");
    await dialog.locator("#attendance-login").click();
    await expect(dialog.getByRole("status")).toHaveText("账号已添加.");
  }
  await login("13800138000");
  await expect(page.locator("#attendance-manage")).toBeVisible();
  await expect(dialog.locator(".account-row")).toHaveCount(1);
  await expect(panel).not.toContainText("138****8000");
  await expect(dialog).toContainText("今日未签到");
  await dialog.getByRole("button", { name: "关闭弹窗" }).click();
  await page.locator("#attendance-enabled").check({ force: true });
  await page.locator("#attendance-trigger-trigger").click();
  await page.getByRole("option", { name: "开机时", exact: true }).click();
  await expect(panel).toContainText("用户登录后执行一次");
  await page.locator("#settings-save").click();
  await expect(page.locator("#toast")).toHaveText("设置已保存.");
  await page.locator("#attendance-trigger-trigger").click();
  await page.getByRole("option", { name: "每天定时" }).click();
  await page.locator("#attendance-time").fill("20:30");
  await page.locator("#settings-save").click();
  await expect(page.locator("#toast")).toHaveText("设置已保存.");
  await action.click();
  await expect(action).toHaveText("今日已签到");
  await expect(action).toBeDisabled();
  await page.locator("#attendance-manage").click();
  await expect(dialog).toContainText("138****8000");
  await expect(dialog).toContainText("今日已签到");
  await dialog.getByRole("button", { name: "添加账号", exact: true }).click();
  await dialog.getByRole("tab", { name: "短信验证码" }).click();
  await dialog.locator("#attendance-phone").fill("13900139000");
  await expect(dialog.locator(".attendance__remember")).toHaveCount(0);
  await dialog.getByRole("button", { name: "发送验证码", exact: true }).click();
  await expect(dialog.getByRole("status")).toHaveText("验证码已发送.");
  await dialog.locator("#attendance-code").fill("123456");
  await dialog.locator("#attendance-login").click();
  await expect(dialog.locator(".account-row")).toHaveCount(2);
  await dialog.getByRole("button", { name: "关闭弹窗" }).click();
  await expect(page.locator("#attendance-manage")).toBeVisible();
  await expect(action).toHaveText("立即签到");
  await action.click();
  await expect(action).toHaveText("今日已签到");
  await expect(action).toBeDisabled();
  await page.locator("#attendance-manage").click();
  for (let remaining = 1; remaining >= 0; remaining--) {
    await dialog
      .getByRole("button", { name: "移除", exact: true })
      .first()
      .click();
    await expect(dialog.locator(".account-row")).toHaveCount(remaining + 1);
    await dialog.getByRole("button", { name: "确认移除", exact: true }).click();
    await expect(dialog.locator(".account-row")).toHaveCount(remaining);
    await expect(page.locator("#attendance-manage")).toHaveCount(
      remaining ? 1 : 0,
    );
  }
  await dialog.getByRole("button", { name: "关闭弹窗" }).click();
  await expect(action).toHaveText("立即登录");
  await expect(panel).not.toContainText("签到已完成");
  await expect(panel).not.toContainText("社区签到:");
});
