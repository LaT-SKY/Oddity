import { settingsCategory } from "./settings-helpers";
import { test, expect } from "@playwright/test";

test("设置页安装目录均可选择、取消并自动保存", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(() => {
    let selection = 0;
    Object.assign(window, {
      __TAURI_INTERNALS__: {
        invoke: async (command: string) => {
          if (command !== "plugin:dialog|open") throw new Error(command);
          selection++;
          return selection % 2 === 1 ? "/tmp/异环 Test" : null;
        },
      },
    });
  });
  await page.goto("/");
  await page.locator('[data-page="settings"]').click();
  await expect(page.locator("#page-settings")).toBeVisible();
  await settingsCategory(page, "game");
  for (const [id, label] of [
    ["set-install", "国服安装目录"],
    ["set-global-install", "国际服安装目录"],
  ]) {
    const input = page.locator(`#${id}`);
    // 使用绝对路径, 使浏览器测试仅模拟目录选择 IPC.
    await input.fill("/tmp/original");
    await page
      .getByRole("button", { name: `浏览${label}`, exact: true })
      .click();
    await expect(input).toHaveValue("/tmp/异环 Test");
    await expect(page.locator("#settings-form")).toHaveAttribute(
      "data-save-state",
      "saved",
    );
    await page
      .getByRole("button", { name: `浏览${label}`, exact: true })
      .click();
    await expect(input).toHaveValue("/tmp/异环 Test");
    await expect(input).toBeEnabled();
    const bounds = await input.boundingBox();
    expect(bounds!.width).toBeGreaterThan(250);
  }
  await page.screenshot({ path: "test-results/directory-fields.png" });
  expect(errors).toEqual([]);
});

test("更新页目录选择按地区保存并与设置页同步", async ({ page }) => {
  await page.addInitScript(() => {
    let selection = 0;
    Object.assign(window, {
      __TAURI_INTERNALS__: {
        invoke: async (command: string) => {
          if (command !== "plugin:dialog|open") throw new Error(command);
          selection++;
          return selection === 1
            ? "/tmp/国服 Test"
            : selection === 2
              ? null
              : "/tmp/国际服 Test";
        },
      },
    });
  });
  await page.goto("/");
  await page.locator('[data-page="update"]').click();
  const input = page.locator("#download-install-root");
  const browse = page.getByRole("button", {
    name: "浏览安装目录",
    exact: true,
  });
  await input.fill("/tmp/original");
  await browse.click();
  await expect(input).toHaveValue("/tmp/国服 Test");
  await browse.click();
  await expect(input).toHaveValue("/tmp/国服 Test");
  await expect(input).toBeEnabled();
  await page.locator("#download-distribution-trigger").click();
  await page.getByRole("option", { name: "国际服", exact: true }).click();
  await input.fill("/tmp/global-original");
  await browse.click();
  await expect(input).toHaveValue("/tmp/国际服 Test");
  await page.locator('[data-page="settings"]').click();
  await settingsCategory(page, "game");
  await expect(page.locator("#set-install")).toHaveValue("/tmp/国服 Test");
  await expect(page.locator("#set-global-install")).toHaveValue(
    "/tmp/国际服 Test",
  );
  await expect(page.locator("#settings-form")).toHaveAttribute(
    "data-save-state",
    "saved",
  );
  await page.locator("#settings-redetect").click();
  await expect(page.locator("#set-global-install")).toHaveValue(
    "/tmp/国际服 Test",
  );
});
