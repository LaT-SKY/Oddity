import { settingsCategory } from "./settings-helpers";
import { test, expect } from "@playwright/test";

test("选择器方向键预览、Escape 取消、Enter 确认与暗色浮层", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/#settings");
  const trigger = page.locator("#set-theme-trigger");
  await trigger.focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(page.locator("#set-theme")).toHaveValue("light");
  await expect(trigger).toHaveAttribute("aria-activedescendant", /.+-1/);
  await page.keyboard.press("Escape");
  await expect(trigger).toBeFocused();
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await expect(page.locator(".app")).toHaveAttribute("data-theme", "dark");
  await trigger.click();
  await expect(page.getByRole("listbox")).toHaveAttribute("data-theme", "dark");
  await page.keyboard.press("Escape");
  await settingsCategory(page, "runtime");
  await page.getByRole("button", { name: "创建运行环境", exact: true }).click();
  const dialog = page.getByRole("dialog", {
    name: "创建运行环境",
    exact: true,
  });
  await dialog.locator("#runtime-environment-distribution-trigger").click();
  const menu = dialog.getByRole("listbox");
  await expect(menu).toBeVisible();
  await expect(menu).toHaveAttribute("data-theme", "dark");
  expect(await menu.evaluate((el) => el.closest(".modal__panel"))).toBeNull();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("分段控件保持键盘焦点, 登录表单跟随选择切换", async ({ page }) => {
  await page.goto("/#settings");
  await settingsCategory(page, "account");
  await page.locator("#attendance-action").click();
  const dialog = page.getByRole("dialog");
  const password = dialog.getByRole("tab", { name: "密码登录" });
  const sms = dialog.getByRole("tab", { name: "短信验证码" });
  await password.focus();
  await page.keyboard.press("ArrowRight");
  await expect(sms).toBeFocused();
  await expect(sms).toHaveAttribute("aria-selected", "true");
  await expect(dialog.locator("#attendance-code")).toBeVisible();
  await page.keyboard.press("ArrowLeft");
  await expect(password).toBeFocused();
  await expect(dialog.locator("#attendance-password")).toBeVisible();
});

test("登录字段横向交接、弹窗高度连续变化与快速切换", async ({ page }) => {
  await page.clock.install();
  await page.clock.pauseAt(new Date());
  await page.goto("/#settings");
  await page.clock.runFor(1500);
  await settingsCategory(page, "account");
  await page.locator("#attendance-action").click();
  await page.clock.runFor(1000);
  const dialog = page.getByRole("dialog");
  const panel = dialog.locator(".modal__panel");
  await expect(panel).toHaveCSS("opacity", "1");
  const phone = await dialog.locator("#attendance-phone").boundingBox();
  const password = await dialog.locator("#attendance-password").boundingBox();
  const tabs = await dialog.getByRole("tablist").boundingBox();
  const phoneLabel = await dialog
    .locator("label", { has: page.locator("#attendance-phone") })
    .boundingBox();
  expect(phoneLabel!.y - tabs!.y - tabs!.height).toBeGreaterThanOrEqual(17);
  expect(password!.y).toBeGreaterThan(phone!.y + phone!.height);
  await page.evaluate(() => {
    const probe = {
      frames: [] as { height: number; x: number; opacity: number }[],
      done: false,
    };
    Object.assign(window, { modalSwitchProbe: probe });
    const start = performance.now();
    function frame() {
      const panel = document.querySelector<HTMLElement>(".modal__panel")!;
      const current = panel.querySelector<HTMLElement>(
        'form .view-stack > fieldset:not([aria-hidden="true"])',
      )!;
      probe.frames.push({
        height: panel.getBoundingClientRect().height,
        x: new DOMMatrixReadOnly(getComputedStyle(current).transform).m41,
        opacity: Number(getComputedStyle(current).opacity),
      });
      if (performance.now() - start < 1000) requestAnimationFrame(frame);
      else probe.done = true;
    }
    requestAnimationFrame(frame);
  });
  await dialog.getByRole("tab", { name: "短信验证码" }).click();
  await page.clock.runFor(1100);
  await page.waitForFunction(
    () =>
      (window as unknown as { modalSwitchProbe: { done: boolean } })
        .modalSwitchProbe.done,
  );
  const frames = await page.evaluate(
    () =>
      (
        window as unknown as {
          modalSwitchProbe: {
            frames: { height: number; x: number; opacity: number }[];
          };
        }
      ).modalSwitchProbe.frames,
  );
  const heights = frames.map((frame) => frame.height);
  expect(
    new Set(heights.map((height) => Math.round(height))).size,
  ).toBeGreaterThan(5);
  expect(
    frames.some(
      (frame) => frame.x > 1 && frame.opacity > 0 && frame.opacity < 1,
    ),
  ).toBe(true);
  const distance = Math.abs(heights.at(-1)! - heights[0]);
  expect(distance).toBeGreaterThan(10);
  expect(
    Math.max(
      ...heights
        .slice(1)
        .map((height, index) => Math.abs(height - heights[index])),
    ),
  ).toBeLessThan(distance * 0.7);
  await page.clock.resume();
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowLeft");
  await expect(dialog.locator("#attendance-phone")).toHaveCount(1);
  await expect(dialog.locator("#attendance-password")).toBeVisible();
  await expect(
    dialog.locator('[aria-hidden="true"].view-stack__page'),
  ).toHaveCount(0);
  await dialog.getByRole("button", { name: "返回账号列表" }).click();
  await expect(dialog.getByRole("button", { name: "添加账号" })).toBeVisible();
});
