import { test, expect } from "@playwright/test";

test("游戏模式分类、搜索、禁用状态与设置页布局一致", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/#game-mode");
  const panel = page.locator("#page-game-mode");
  await expect(panel).toBeVisible();
  await expect(
    panel.getByRole("heading", { name: "游戏模式", exact: true }),
  ).toBeVisible();
  expect(
    await page
      .locator(".rail__nav [data-page]")
      .evaluateAll((items) =>
        items.map((item) => (item as HTMLElement).dataset.page),
      ),
  ).toEqual(["home", "update", "game-mode", "bulletin", "records", "settings"]);
  await panel.getByRole("button", { name: "重新检测", exact: true }).click();
  await page.getByRole("button", { name: "关闭通知", exact: true }).click();
  await expect(page.locator(".notification-card")).toHaveCount(0);
  const title = await panel.locator("h2").boundingBox();
  const search = await panel.getByRole("searchbox").boundingBox();
  await page.screenshot({ path: "test-results/game-mode-light.png" });
  for (const category of ["general", "cpu", "gpu", "gamescope"]) {
    await panel.locator(`[data-category="${category}"]`).click();
    const content = panel.locator(`#game-mode-panel-${category}`);
    await expect(content).toBeVisible();
    await expect(content).toHaveCSS("opacity", "1");
    await expect(
      panel.locator(`[data-category="${category}"]`),
    ).toHaveAttribute("aria-current", "true");
    for (const control of await content
      .locator(
        'input[type="checkbox"], .select-trigger, .game-mode-gpu-actions .btn',
      )
      .all())
      await expect(control).toBeDisabled();
    for (const row of await content.locator(".setting-row").all()) {
      const opacity = await row.evaluate((element) => {
        let opacity = 1;
        for (
          let node: Element | null = element;
          node && !node.classList.contains("settings-panel");
          node = node.parentElement
        )
          opacity *= Number(getComputedStyle(node).opacity);
        return opacity;
      });
      expect(opacity).toBeCloseTo(0.45);
    }
    if (category === "cpu")
      await page.screenshot({ path: "test-results/game-mode-cpu.png" });
  }
  await panel.getByRole("searchbox").fill("显存");
  await panel
    .locator(".settings-result")
    .filter({ hasText: "GPU 超频" })
    .click();
  await expect(panel.locator('[data-category="gpu"]')).toHaveAttribute(
    "aria-current",
    "true",
  );
  await expect(panel.getByRole("searchbox")).toHaveValue("");
  await panel.getByRole("searchbox").fill("不存在的设置");
  await expect(panel.locator("#game-mode-results")).toContainText(
    "未找到相关设置",
  );
  await panel.getByRole("searchbox").press("Escape");
  await expect(panel).toBeVisible();
  await expect(panel.getByRole("searchbox")).toHaveValue("");
  await panel.locator('[data-category="gpu"]').focus();
  await page.keyboard.press("Home");
  await expect(panel.locator('[data-category="general"]')).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(panel.locator('[data-category="cpu"]')).toBeFocused();
  await page.keyboard.press("Control+6");
  const settings = page.locator("#page-settings");
  await expect(settings).toBeVisible();
  expect((await settings.locator("#settings-title").boundingBox())!.y).toBe(
    title!.y,
  );
  const settingsSearch = await settings.getByRole("searchbox").boundingBox();
  expect(settingsSearch!.y).toBe(search!.y);
  expect(settingsSearch!.width).toBeCloseTo(search!.width, 2);
  await page.locator("#set-theme-trigger").click();
  await page.getByRole("option", { name: "暗色", exact: true }).click();
  await page.keyboard.press("Control+3");
  await expect(panel).toBeVisible();
  await expect(panel).toHaveCSS("opacity", "1");
  await expect(settings).toHaveCSS("opacity", "0");
  await page.screenshot({ path: "test-results/game-mode-dark.png" });
  await page.setViewportSize({ width: 900, height: 600 });
  await expect(
    panel.getByRole("heading", { name: "游戏模式", exact: true }),
  ).toBeVisible();
  expect(
    await panel.evaluate(
      (element) => element.scrollWidth <= element.clientWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: "test-results/game-mode-compact.png" });
  await page.setViewportSize({ width: 900, height: 420 });
  const headingTop = (await panel.locator("h2").boundingBox())!.y;
  await panel.locator("#game-mode-panel-cpu").hover();
  await page.mouse.wheel(0, 600);
  await expect
    .poll(() =>
      panel
        .locator("#game-mode-panel-cpu")
        .evaluate((element) => element.scrollTop),
    )
    .toBeGreaterThan(0);
  expect((await panel.locator("h2").boundingBox())!.y).toBe(headingTop);
  await page.keyboard.press("Escape");
  await expect(page.locator("#page-home")).toBeVisible();
  await page.locator('[data-page="game-mode"]').click();
  await expect(panel).toBeVisible();
  await page.locator('[data-page="bulletin"]').click();
  await expect(page.locator("#page-bulletin")).toBeVisible();
  expect(errors).toEqual([]);
});
