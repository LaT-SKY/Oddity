import { test, expect, type Page } from "@playwright/test";

interface ModalFrame {
  present: boolean;
  backdrop: number;
  opacity: number;
  scale: number;
}
async function recordModal(
  page: Page,
  action: () => Promise<void>,
  duration: number,
) {
  await page.evaluate((duration) => {
    const state = { frames: [] as ModalFrame[], done: false };
    (window as unknown as { modalProbe: typeof state }).modalProbe = state;
    const start = performance.now();
    function sample() {
      const surface = document.querySelector<HTMLElement>(".modal__panel");
      const backdrop = document.querySelector<HTMLElement>(".modal__backdrop");
      state.frames.push(
        surface && backdrop
          ? {
              present: true,
              backdrop: Number(getComputedStyle(backdrop).opacity),
              opacity: Number(getComputedStyle(surface).opacity),
              scale: new DOMMatrixReadOnly(getComputedStyle(surface).transform)
                .a,
            }
          : { present: false, backdrop: 0, opacity: 0, scale: 0 },
      );
      if (performance.now() - start < duration) requestAnimationFrame(sample);
      else state.done = true;
    }
    requestAnimationFrame(sample);
  }, duration);
  await action();
  await page.waitForFunction(
    () =>
      (window as unknown as { modalProbe: { done: boolean } }).modalProbe.done,
  );
  return page.evaluate(
    () =>
      (window as unknown as { modalProbe: { frames: ModalFrame[] } }).modalProbe
        .frames,
  );
}
function expectFade(frames: ModalFrame[], entering: boolean) {
  const visible = frames.filter((frame) => frame.present);
  expect(visible.length).toBeGreaterThan(3);
  for (let i = 1; i < visible.length; i++) {
    for (const key of ["backdrop", "opacity"] as const) {
      const delta = visible[i][key] - visible[i - 1][key];
      expect(entering ? delta : -delta).toBeGreaterThanOrEqual(-0.002);
    }
  }
}

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
  await page.locator("#set-install").press("Tab");
  await expect(page.locator("#settings-form")).toHaveAttribute(
    "data-save-state",
    "saved",
  );
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

test("预览模式完整校验发现资源损坏并完成修复", async ({ page }) => {
  await page.goto("/#update");
  const update = page.locator("#page-update");
  const primary = update.locator("#game-download-primary");
  // 预览资源大小一致, 快速检查显示就绪。
  await expect(primary).toHaveText("检查通过");
  // 完整校验暴露模拟的内容损坏。
  await update.getByRole("button", { name: "完整校验", exact: true }).click();
  await expect(
    update.getByRole("heading", { name: "发现待下载的游戏资源" }),
  ).toBeVisible();
  await expect(primary).toHaveText("更新游戏");
  await primary.click();
  await expect(
    update.getByRole("button", { name: "暂停下载", exact: true }),
  ).toBeVisible();
  await expect(
    update.getByRole("heading", { name: "游戏资源已就绪" }),
  ).toBeVisible();
  await expect(primary).toHaveText("检查通过");
  await expect(
    update.getByRole("button", { name: "暂停下载", exact: true }),
  ).toHaveCount(0);
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

test("视频双缓冲循环无黑帧, 离页暂停并保留进度", async ({ page }) => {
  test.setTimeout(240_000);
  await page.goto("/");
  const video = page.locator("#hero-video");
  const canvas = page.locator("#hero-canvas");
  await expect(canvas).toHaveClass(/is-visible/);
  await expect
    .poll(() => video.evaluate((v: HTMLVideoElement) => v.duration))
    .toBeCloseTo(13, 2);
  await page.evaluate(() => {
    const videos = Array.from(
      document.querySelectorAll<HTMLVideoElement>("video.hero__source"),
    );
    const display = document.querySelector<HTMLCanvasElement>("#hero-canvas")!;
    const monitor = document.createElement("div");
    monitor.id = "video-probe";
    monitor.dataset.plays = "0";
    monitor.dataset.blackFrames = "0";
    document.body.append(monitor);
    for (const source of videos)
      source.addEventListener("play", () => {
        monitor.dataset.plays = String(Number(monitor.dataset.plays) + 1);
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
  await expect
    .poll(async () => Number(await probe.getAttribute("data-plays")), {
      timeout: 45_000,
    })
    .toBeGreaterThanOrEqual(3);
  await expect(probe).toHaveAttribute("data-black-frames", "0");

  const clock = () =>
    page.evaluate(() =>
      Math.max(
        ...Array.from(
          document.querySelectorAll<HTMLVideoElement>("video.hero__source"),
        ).map((source) => source.currentTime),
      ),
    );
  const before = await clock();
  expect(before).toBeGreaterThan(0);
  await page.locator('[data-page="settings"]').click();
  await expect(canvas).not.toHaveClass(/is-visible/);
  await expect
    .poll(() =>
      page.evaluate(() =>
        Array.from(
          document.querySelectorAll<HTMLVideoElement>("video.hero__source"),
        ).every((source) => source.paused),
      ),
    )
    .toBe(true);
  const paused = await clock();
  expect(paused).toBeGreaterThanOrEqual(before);
  await page.waitForTimeout(1500);
  const stopped = await clock();
  expect(stopped).toBeGreaterThanOrEqual(paused);
  expect(stopped).toBeLessThan(paused + 0.05);
  await page.locator('[data-page="home"]').click();
  await expect(canvas).toHaveClass(/is-visible/);
  await expect.poll(clock, { timeout: 15_000 }).toBeGreaterThan(stopped);

  await page.locator('[data-page="settings"]').click();
  await page.locator("#set-hero-media").selectOption("photo");
  await expect(canvas).not.toHaveClass(/is-visible/);
  await page.locator("#set-hero-media").selectOption("video");
  await expect
    .poll(() =>
      page.evaluate(() =>
        Array.from(
          document.querySelectorAll<HTMLVideoElement>("video.hero__source"),
        ).every((source) => source.paused),
      ),
    )
    .toBe(true);
  await page.locator('[data-page="home"]').click();
  await expect(canvas).toHaveClass(/is-visible/);
});

test("多账号签到、自动保存和账号管理", async ({ page }) => {
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
  await expect(page.locator("#settings-form")).toHaveAttribute(
    "data-save-state",
    "saved",
  );
  await page.locator("#attendance-trigger-trigger").click();
  await page.getByRole("option", { name: "每天定时" }).click();
  await page.locator("#attendance-time").fill("20:30");
  await page.locator("#attendance-time").press("Tab");
  await expect(page.locator("#settings-form")).toHaveAttribute(
    "data-save-state",
    "saved",
  );
  await action.click();
  await expect(action).toHaveText("今日已签到");
  await expect(action).toBeDisabled();
  await page.locator("#attendance-manage").click();
  await expect(dialog).toContainText("138****8000");
  await expect(dialog).toContainText("今日已签到");
  const taskResults = dialog.getByRole("list", { name: "已完成任务" });
  await expect(dialog.locator("details")).toHaveCount(0);
  await expect(taskResults.locator("li")).toHaveText([
    "社区签到",
    "异环签到",
    "社区杂活",
    "云异环签到",
  ]);
  expect(
    await taskResults
      .locator("li")
      .evaluateAll(
        (items) =>
          new Set(
            items.map((item) =>
              item.querySelector("use")?.getAttribute("href"),
            ),
          ).size,
      ),
  ).toBe(4);
  await taskResults
    .getByRole("button", { name: "异环签到", exact: true })
    .hover();
  await expect(
    page.getByRole("tooltip").filter({ hasText: "角色 预览角色" }),
  ).toBeVisible();
  await taskResults
    .getByRole("button", { name: "社区杂活", exact: true })
    .hover();
  await expect(
    page.getByRole("tooltip").filter({ hasText: "社区杂活" }),
  ).toContainText("模拟今日金币 110/150");
  await taskResults
    .getByRole("button", { name: "云异环签到", exact: true })
    .hover();
  await expect(
    page.getByRole("tooltip").filter({ hasText: "云异环签到" }),
  ).toContainText("模拟本次赠送 15 分钟, 剩余 120 分钟");
  await dialog.getByRole("button", { name: "云异环签到", exact: true }).focus();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("tooltip")).toHaveCount(0);
  await expect(dialog).toBeVisible();
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

test("Motion 弹窗入场、快速重开与减少动态效果", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.goto("/#settings");
  const action = page.locator("#attendance-action");
  const dialog = page.getByRole("dialog");
  const entry = await recordModal(page, () => action.click(), 1100);
  expectFade(entry, true);
  const scales = entry
    .filter((frame) => frame.present)
    .map((frame) => frame.scale);
  expect(Math.min(...scales)).toBeLessThan(0.999);
  expect(Math.max(...scales)).toBeLessThan(1.01);
  expect(scales.at(-1)).toBeCloseTo(1, 4);
  expect(
    entry
      .slice(-10)
      .every((frame) => frame.backdrop === 1 && frame.opacity === 1),
  ).toBe(true);
  await expect
    .poll(() =>
      dialog.evaluate((element) => getComputedStyle(element).transform),
    )
    .toBe("matrix(1, 0, 0, 1, 0, 0)");
  await expect(dialog.getByRole("button", { name: "关闭弹窗" })).toBeFocused();
  await dialog.press("Escape");
  await expect(action).toBeFocused();
  await action.click();
  await expect(dialog).toHaveCount(1);
  await expect(dialog.getByRole("button", { name: "关闭弹窗" })).toBeFocused();
  await expect
    .poll(() =>
      dialog.evaluate((element) => getComputedStyle(element).transform),
    )
    .toBe("matrix(1, 0, 0, 1, 0, 0)");
  const exit = await recordModal(
    page,
    () => dialog.getByRole("button", { name: "关闭弹窗" }).click(),
    600,
  );
  expectFade(exit, false);
  const removed = exit.findIndex(
    (frame, index) =>
      !frame.present &&
      exit.slice(0, index).some((previous) => previous.present),
  );
  expect(removed).toBeGreaterThan(0);
  expect(exit.slice(removed).every((frame) => !frame.present)).toBe(true);
  await expect(page.locator(".modal")).toHaveCount(0);
  await page.locator("#set-motion").check({ force: true });
  await action.click();
  expect(
    await dialog.evaluate((element) => element.getAnimations().length),
  ).toBe(0);
  expect(
    await dialog.evaluate((element) => getComputedStyle(element).transform),
  ).toBe("none");
  await dialog.press("Escape");
  await page.locator("#set-motion").uncheck({ force: true });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await action.click();
  expect(
    await dialog.evaluate((element) => element.getAnimations().length),
  ).toBe(0);
  expect(
    await dialog.evaluate((element) => getComputedStyle(element).transform),
  ).toBe("none");
});
