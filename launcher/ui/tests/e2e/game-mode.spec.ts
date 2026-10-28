import { test, expect } from "@playwright/test";

test("游戏模式独立导航、能力反馈与相邻页面", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/#game-mode");
  const panel = page.locator("#page-game-mode");
  await expect(panel).toBeVisible();
  await expect(
    panel.getByRole("heading", { name: "游戏模式", exact: true }),
  ).toBeVisible();
  await expect(panel.getByRole("checkbox")).toHaveCount(4);
  expect(
    await page
      .locator(".rail__nav [data-page]")
      .evaluateAll((items) =>
        items.map((item) => (item as HTMLElement).dataset.page),
      ),
  ).toEqual(["home", "update", "game-mode", "bulletin", "settings"]);
  for (const checkbox of await panel.getByRole("checkbox").all())
    await expect(checkbox).toBeDisabled();
  for (const row of await panel.locator(".setting-row").all())
    await expect(row).toHaveCSS("opacity", "0.45");
  await panel.getByRole("button", { name: "重新检测" }).click();
  await page.getByRole("button", { name: "关闭通知", exact: true }).click();
  await expect(page.locator(".notification-card")).toHaveCount(0);
  await page.screenshot({ path: "test-results/game-mode-light.png" });
  const title = await panel.locator("h2").boundingBox();
  await page.keyboard.press("Control+5");
  await expect(page.locator("#page-settings")).toBeVisible();
  expect((await page.locator("#settings-title").boundingBox())!.y).toBe(
    title!.y,
  );
  await page.locator("#set-theme-trigger").click();
  await page.getByRole("option", { name: "暗色", exact: true }).click();
  await page.keyboard.press("Control+3");
  await expect(panel).toBeVisible();
  await expect(page.locator('[data-page="game-mode"]')).toHaveAttribute(
    "aria-current",
    "page",
  );
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
  await panel.locator(".game-mode-content").hover();
  await page.mouse.wheel(0, 600);
  await expect
    .poll(() =>
      panel
        .locator(".game-mode-content")
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
