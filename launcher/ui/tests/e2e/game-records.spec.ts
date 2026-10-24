import { test, expect, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";

async function addAccount(page: Page, phone: string, first = false) {
  if (first) await page.locator("#attendance-action").click();
  else {
    await page.locator("#attendance-manage").click();
    await page.getByRole("button", { name: "添加账号", exact: true }).click();
  }
  await page.locator("#attendance-phone").fill(phone);
  await page.locator("#attendance-password").fill("preview-only");
  await page.locator("#attendance-login").click();
  await expect(page.locator(".account-row")).toContainText([
    phone.slice(0, 3) + "****" + phone.slice(7),
  ]);
  await page.getByRole("button", { name: "关闭弹窗" }).click();
  await expect(page.locator(".modal")).toHaveCount(0);
}
async function screenshot(page: Page, name: string) {
  const directory = process.env.ODDITY_SCREENSHOT_DIR;
  if (!directory) return;
  await mkdir(directory, { recursive: true });
  await page.screenshot({
    path: join(directory, name),
    animations: "disabled",
  });
}

async function expectCenteredIcons(page: Page) {
  const offsets = await page
    .locator(".game-records__icon-button")
    .evaluateAll((buttons) =>
      buttons.map((button) => {
        const circle = button.getBoundingClientRect();
        const icon = button.querySelector("svg")!.getBoundingClientRect();
        return {
          label: button.getAttribute("aria-label"),
          x: Math.abs(icon.x + icon.width / 2 - circle.x - circle.width / 2),
          y: Math.abs(icon.y + icon.height / 2 - circle.y - circle.height / 2),
        };
      }),
    );
  expect(offsets).toHaveLength(3);
  for (const offset of offsets) {
    expect(offset.x, offset.label!).toBeLessThanOrEqual(0.5);
    expect(offset.y, offset.label!).toBeLessThanOrEqual(0.5);
  }
}

test("胶囊开合、账号切换、双向无滚动和移除同步", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await expect(page.locator("#app")).toHaveClass(/is-intro-done/);
  const capsule = page.locator(".game-records__capsule");
  const panel = page.locator(".game-records__panel");
  await expect(capsule).toHaveAttribute("aria-expanded", "false");
  await screenshot(page, "home-capsule-light.png");
  await capsule.click();
  await expect(page.locator(".game-records__empty")).toContainText(
    "登录塔吉多",
  );
  await screenshot(page, "home-empty-light.png");
  await page.getByRole("button", { name: /前往设置登录/ }).click();
  await expect(page.locator("#attendance-action")).toHaveText("立即登录");
  await page.locator("#set-hero-media").selectOption("photo");
  await addAccount(page, "13800138000", true);
  await addAccount(page, "13900139000");
  await expect(page.locator("#attendance-enabled")).not.toBeChecked();
  await page.locator('[data-page="home"]').click();
  await capsule.click();
  await expect(page.locator(".game-records__account")).toHaveCount(1);
  await expect(page.locator(".game-records__primary")).toContainText(
    "168 / 240",
  );
  await expectCenteredIcons(page);
  await screenshot(page, "home-panel-light.png");
  await page.getByRole("button", { name: "下一个账号" }).click();
  await expect(panel).toContainText("139****9000");
  await expect(page.locator(".game-records__primary")).toContainText(
    "周本剩余0 次",
  );
  await screenshot(page, "home-second-account-light.png");
  await page.getByRole("button", { name: "上一个账号" }).click();
  await expect(panel).toContainText("138****8000");
  await page.getByRole("button", { name: "刷新游戏状态" }).click();
  await expect(panel).toHaveAttribute("aria-busy", "false");
  await page.locator('[data-page="settings"]').click();
  await page.locator("#set-theme").selectOption("dark");
  await page.locator('[data-page="home"]').click();
  await capsule.click();
  await expect(panel).toBeVisible();
  await screenshot(page, "home-panel-dark.png");
  await page.setViewportSize({ width: 980, height: 640 });
  const positions = await page.evaluate(() => {
    const panel = document
      .querySelector(".game-records__panel")!
      .getBoundingClientRect();
    const stats = document
      .querySelector(".game-records__secondary")!
      .getBoundingClientRect();
    const footer = document
      .querySelector(".game-records__foot")!
      .getBoundingClientRect();
    const launch = document
      .querySelector("#launch-btn")!
      .getBoundingClientRect();
    const overflow = [
      ...document.querySelectorAll<HTMLElement>(
        ".game-records__panel, .game-records__account, #page-home",
      ),
    ].map((el) => ({
      x: el.scrollWidth - el.clientWidth,
      y: el.scrollHeight - el.clientHeight,
      overflowX: getComputedStyle(el).overflowX,
      overflowY: getComputedStyle(el).overflowY,
    }));
    return {
      contentGap: footer.top - stats.bottom,
      panelRight: panel.right,
      panelBottom: panel.bottom,
      panelLeft: panel.left,
      launchRight: launch.right,
      launchBottom: launch.bottom,
      overflow,
    };
  });
  expect(positions.contentGap).toBeGreaterThanOrEqual(12);
  expect(positions.contentGap).toBeLessThanOrEqual(20);
  expect(positions.panelRight).toBeLessThanOrEqual(980);
  expect(positions.panelBottom).toBeLessThanOrEqual(640);
  expect(positions.launchBottom).toBeLessThanOrEqual(640);
  expect(positions.launchRight).toBeLessThan(positions.panelLeft);
  for (const item of positions.overflow) {
    expect(item.x).toBeLessThanOrEqual(1);
    expect(item.y).toBeLessThanOrEqual(1);
    expect(["hidden", "clip"]).toContain(item.overflowX);
    expect(["hidden", "clip"]).toContain(item.overflowY);
  }
  await expectCenteredIcons(page);
  await screenshot(page, "home-panel-compact-dark.png");
  await page.getByRole("button", { name: "下一个账号" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(panel).toContainText("139****9000");
  await page.keyboard.press("Escape");
  await expect(panel).toHaveCount(0);
  await expect(capsule).toBeFocused();
  await capsule.click();
  await capsule.click();
  await expect(panel).toHaveCount(0);
  await capsule.click();
  await page.locator("#home-title").click();
  await expect(panel).toHaveCount(0);
  await page.locator('[data-page="settings"]').click();
  await page.locator("#attendance-manage").click();
  await page.getByRole("button", { name: "移除", exact: true }).first().click();
  await page.getByRole("button", { name: "确认移除", exact: true }).click();
  await expect(page.locator(".account-row")).toHaveCount(1);
  await page.getByRole("button", { name: "关闭弹窗" }).click();
  await page.locator('[data-page="home"]').click();
  await capsule.click();
  await expect(panel).not.toContainText("138****8000");
  await expect(panel).toContainText("139****9000");
  expect(errors).toEqual([]);
});

test("Motion 弹簧过冲、快速开合中断与减少动态效果", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.goto("/");
  await expect(page.locator("#app")).toHaveClass(/is-intro-done/);
  await page.evaluate(() => {
    const probe = { scales: [] as number[], done: false };
    (window as unknown as { recordMotion: typeof probe }).recordMotion = probe;
    const start = performance.now();
    function sample() {
      const el = document.querySelector(".game-records__panel");
      if (el)
        probe.scales.push(
          new DOMMatrixReadOnly(getComputedStyle(el).transform).a,
        );
      if (performance.now() - start < 1400) requestAnimationFrame(sample);
      else probe.done = true;
    }
    requestAnimationFrame(sample);
  });
  const capsule = page.locator(".game-records__capsule");
  const panel = page.locator(".game-records__panel");
  await capsule.click();
  await page.waitForFunction(
    () =>
      (window as unknown as { recordMotion: { done: boolean } }).recordMotion
        .done,
  );
  const scales = await page.evaluate(
    () =>
      (window as unknown as { recordMotion: { scales: number[] } }).recordMotion
        .scales,
  );
  expect(scales.length).toBeGreaterThan(10);
  expect(Math.min(...scales)).toBeLessThan(0.98);
  expect(Math.max(...scales)).toBeGreaterThan(1.001);
  expect(scales.at(-1)).toBeCloseTo(1, 2);
  for (let i = 0; i < 7; i++) await capsule.click();
  await expect(capsule).toHaveAttribute("aria-expanded", "false");
  await expect(panel).toHaveCount(0);
  await capsule.click();
  await expect(panel).toHaveCount(1);
  await expect(panel).not.toHaveAttribute("inert");
  await capsule.click();
  await expect(panel).toHaveCount(0);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await capsule.click();
  await expect(panel).toHaveCount(1);
  await expect
    .poll(() =>
      panel.evaluate(
        (el) => new DOMMatrixReadOnly(getComputedStyle(el).transform).a,
      ),
    )
    .toBe(1);
});
