import { test, expect } from "@playwright/test";

test("更新摘要、暂停继续与窄面板布局", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/#update");
  const update = page.locator("#page-update");
  const primary = page.locator("#game-download-primary");
  const budget = update.locator(".disk-budget");
  await expect(primary).toHaveText("检查通过");
  await expect(budget).toHaveCount(0);
  await update.getByRole("button", { name: "完整校验", exact: true }).click();
  await expect(primary).toHaveText("更新游戏");
  await expect(update.locator("progress")).toHaveCount(0);
  await expect(budget).toContainText("预计需要 17.2 MiB");
  await expect(budget).toContainText("可用 256 GiB");
  await expect(budget).not.toHaveClass(/is-short/);
  await primary.click();
  await expect(budget).toHaveCount(0);
  await update.getByRole("button", { name: "暂停下载", exact: true }).click();
  await expect(
    update.getByRole("heading", { name: "下载已暂停" }),
  ).toBeVisible();
  await expect(primary).toHaveText("继续更新游戏");
  await expect(update.locator("progress")).toHaveCount(1);
  await expect(update.locator("#download-install-root")).toBeEnabled();
  await expect(
    update.locator(
      ".update-status-view > .view-stack__page:not([aria-hidden])",
    ),
  ).toHaveCSS("opacity", "1");
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
  await page.screenshot({ path: "test-results/update-paused.png" });
  await primary.click();
  await expect(primary).toHaveText("检查通过");
  await expect(
    update.locator(
      ".update-status-view > .view-stack__page:not([aria-hidden])",
    ),
  ).toHaveCSS("opacity", "1");
  await update.locator("#update-quick").hover();
  await page.screenshot({ path: "test-results/update-ready.png" });
  await page.setViewportSize({ width: 640, height: 520 });
  await expect(update.locator("#download-install-root")).toBeVisible();
  const overflow = await update.locator(".update-content").evaluate((el) => ({
    gutter: (el as HTMLElement).offsetWidth - el.clientWidth,
    horizontal: el.scrollWidth - el.clientWidth,
    scrollbar: getComputedStyle(el).scrollbarWidth,
  }));
  expect(overflow.gutter).toBe(0);
  expect(overflow.horizontal).toBeLessThanOrEqual(1);
  expect(overflow.scrollbar).toBe("none");
  await page.screenshot({ path: "test-results/update-compact.png" });
  expect(errors).toEqual([]);
});
