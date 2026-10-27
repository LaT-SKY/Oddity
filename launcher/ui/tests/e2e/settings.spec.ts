import { test, expect } from "@playwright/test";
import { settingsCategory } from "./settings-helpers";

test("分类显示相关设置, 保存现有数据且切换时保留滚动位置", async ({ page }) => {
  await page.setViewportSize({ width: 1180, height: 480 });
  await page.goto("/#settings");
  const nav = page.getByRole("navigation", { name: "设置分类" });
  await expect(nav.getByRole("button")).toHaveCount(5);
  await expect(page.locator("#set-theme-trigger")).toBeVisible();
  await expect(page.locator("#set-install")).not.toBeVisible();
  const header = await page.locator(".settings-head").boundingBox();
  const panel = page.locator("#settings-panel-appearance");
  await panel.hover();
  await page.mouse.wheel(0, 500);
  await expect
    .poll(() => panel.evaluate((el) => el.scrollTop))
    .toBeGreaterThan(50);
  const top = await panel.evaluate((el) => el.scrollTop);
  await settingsCategory(page, "game");
  await page.locator("#set-install").fill("/tmp/settings-layout");
  await page.locator("#set-install").press("Tab");
  await expect(page.locator("#settings-form")).toHaveAttribute(
    "data-save-state",
    "saved",
  );
  await settingsCategory(page, "appearance");
  expect(await panel.evaluate((el) => el.scrollTop)).toBeCloseTo(top, 0);
  expect((await page.locator(".settings-head").boundingBox())!.y).toBe(
    header!.y,
  );
  await settingsCategory(page, "game");
  await expect(page.locator("#set-install")).toHaveValue(
    "/tmp/settings-layout",
  );
});

test("搜索名称和关键词, 键盘定位并聚焦设置项, Escape 先清空搜索", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/#settings");
  const search = page.getByRole("searchbox", { name: "搜索设置" });
  await search.fill("路径");
  await expect(page.locator(".settings-result")).toHaveCount(2);
  await search.press("ArrowDown");
  await expect(page.locator(".settings-result").first()).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.locator(".settings-result").last()).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await expect(page.locator(".settings-result").first()).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(search).toHaveValue("");
  await expect(page.locator('[data-category="game"]')).toHaveAttribute(
    "aria-current",
    "true",
  );
  await expect(page.locator("#set-install")).toBeFocused();
  await search.fill("不存在的配置项");
  await expect(page.locator("#settings-results")).toContainText("未找到");
  await search.press("Escape");
  await expect(search).toHaveValue("");
  await expect(page.locator("#page-settings")).toHaveClass(/is-active/);
  expect(errors).toEqual([]);
});

test("分类键盘切换, 运行环境入口直接定位, 窄面板不产生横向溢出", async ({
  page,
}) => {
  await page.goto("/");
  await page.locator("#launch-btn").click();
  await expect(page.locator('[data-category="runtime"]')).toHaveAttribute(
    "aria-current",
    "true",
  );
  await expect(
    page.getByRole("button", { name: "创建运行环境", exact: true }),
  ).toBeVisible();
  await page.locator('[data-category="runtime"]').focus();
  await page.keyboard.press("ArrowDown");
  await expect(page.locator('[data-category="account"]')).toBeFocused();
  await expect(page.locator("#attendance-action")).toBeVisible();
  await page.setViewportSize({ width: 640, height: 520 });
  await settingsCategory(page, "game");
  const bounds = await page.locator(".settings-categories").boundingBox();
  expect(bounds!.height).toBeLessThan(60);
  expect(
    await page
      .locator("#settings-panel-game")
      .evaluate((el) => el.scrollWidth - el.clientWidth),
  ).toBeLessThanOrEqual(1);
  await expect(page.locator("#set-install")).toBeVisible();
  await expect(page.locator("#settings-panel-game")).toHaveCSS("opacity", "1");
  await expect(page.locator(".notification-stack")).toHaveAttribute(
    "data-phase",
    "0",
  );
  for (
    let remaining = await page.locator(".notification-slot").count();
    remaining > 0;
    remaining--
  ) {
    await page
      .getByRole("button", { name: "关闭通知", exact: true })
      .first()
      .click();
    await expect(page.locator(".notification-slot")).toHaveCount(remaining - 1);
  }
  await page.screenshot({ path: "test-results/settings-compact.png" });
  await page.setViewportSize({ width: 1360, height: 860 });
  await settingsCategory(page, "appearance");
  await expect(page.locator("#settings-panel-appearance")).toHaveCSS(
    "opacity",
    "1",
  );
  await page.screenshot({ path: "test-results/settings-layout.png" });
});
