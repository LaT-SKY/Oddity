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
