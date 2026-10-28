import { test, expect } from "@playwright/test";

test("游玩记录展示统计、热力图和可保存的海报", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/#records");
  const panel = page.locator("#page-records");
  await expect(panel).toBeVisible();
  await expect(
    panel.getByRole("heading", { name: "游玩记录", exact: true }),
  ).toBeVisible();
  await expect(panel.locator(".playtime-heatmap__cell")).toHaveCount(371);
  await expect(panel.locator(".playtime-chart")).toBeVisible();
  await expect(panel.locator(".playtime-chart__month")).toHaveCount(12);
  await page.waitForTimeout(600);
  await page.screenshot({ path: "test-results/playtime-light.png" });

  await panel.getByRole("button", { name: "生成海报", exact: true }).click();
  const modal = page.getByRole("dialog");
  await expect(modal).toBeVisible();
  const preview = modal.getByRole("img", { name: "游玩记录海报预览" });
  await expect(preview).toBeVisible();
  const download = page.waitForEvent("download");
  await modal.getByRole("button", { name: "保存图片", exact: true }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(
    /^异环游玩记录-\d{4}-\d{2}-\d{2}\.png$/,
  );
  await file.saveAs("test-results/playtime-poster.png");
  await modal.getByRole("button", { name: "取消", exact: true }).click();
  await expect(modal).toBeHidden();

  await page.locator('[data-page="home"]').click();
  const summary = page.locator(".hero__playtime");
  await expect(summary).toContainText("上次游玩");
  await expect(summary).toContainText(/小时|分钟/);
  expect(errors).toEqual([]);
});

test("游玩记录页支持深链接和键盘导航", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Control+5");
  await expect(page.locator("#page-records")).toBeVisible();
  await expect(page.locator('[data-page="records"]')).toHaveAttribute(
    "aria-current",
    "page",
  );
  await page.keyboard.press("Control+6");
  await expect(page.locator("#page-settings")).toBeVisible();
});
test("热力图和折线图使用自定义提示与动效", async ({ page }) => {
  await page.goto("/#records");
  const panel = page.locator("#page-records");
  await expect(panel.locator(".playtime-chart")).toBeVisible();
  await page.waitForTimeout(600);
  const cell = panel.locator(".playtime-heatmap__cell.is-level-4").first();
  await expect(cell).toBeVisible();
  const cellBox = (await cell.boundingBox())!;
  await page.mouse.move(
    cellBox.x + cellBox.width / 2,
    cellBox.y + cellBox.height / 2,
  );
  const heatTip = page.locator('.playtime-tooltip[data-tooltip="heatmap"]');
  await expect(heatTip).toBeVisible();
  await expect(heatTip).toContainText(/小时|分钟/);
  await expect
    .poll(async () => {
      const box = await heatTip.boundingBox();
      return box
        ? Math.abs(box.x + box.width / 2 - (cellBox.x + cellBox.width / 2))
        : 999;
    })
    .toBeLessThan(80);
  await expect
    .poll(() => cell.evaluate((element) => getComputedStyle(element).transform))
    .toContain("1.45");
  await page.screenshot({ path: "test-results/playtime-heatmap-tooltip.png" });
  await page.mouse.move(12, 12);
  await expect(heatTip).toHaveCount(0);

  const chart = panel.locator(".playtime-chart");
  const chartBox = (await chart.boundingBox())!;
  await page.mouse.move(
    chartBox.x + chartBox.width * 0.2,
    chartBox.y + chartBox.height * 0.5,
  );
  const chartTip = page.locator('.playtime-tooltip[data-tooltip="chart"]');
  await expect(chartTip).toBeVisible();
  await expect(chartTip).toContainText("次游玩");
  await page.screenshot({ path: "test-results/playtime-chart-tooltip.png" });
  const cursor = panel.locator(".playtime-chart__cursor");
  const initial = Number(await cursor.getAttribute("cx"));
  await page.mouse.move(
    chartBox.x + chartBox.width * 0.8,
    chartBox.y + chartBox.height * 0.5,
  );
  await expect
    .poll(async () => Number(await cursor.getAttribute("cx")))
    .toBeGreaterThan(initial + 20);
});
