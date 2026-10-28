import { pauseClock } from "./clock";
import { test, expect } from "@playwright/test";

test("公告加载骨架与刷新保留内容", async ({ page }) => {
  await pauseClock(page);
  await page.goto("/#bulletin");
  const panel = page.locator("#page-bulletin");
  await expect(panel.locator(".bulletin-skeleton")).toBeAttached();
  await expect(panel.locator(".bulletin-entry")).toHaveCount(0);
  await page.clock.runFor(1500);
  await expect(panel.locator(".bulletin-entry")).toHaveCount(3);
  await expect(panel.locator(".bulletin-skeleton")).toHaveCount(0);
  await panel.getByRole("button", { name: "刷新公告" }).click();
  await expect(panel.locator(".bulletin-refreshing")).toHaveText(
    "正在读取最新公告",
  );
  await expect(panel.locator(".bulletin-entry")).toHaveCount(3);
  await page.clock.runFor(1000);
  await expect(panel.locator(".bulletin-refreshing")).toHaveCount(0);
});

test("配图保留比例, 分类与窄面板保持完整布局", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/assets/hero-keyart.jpg", (route) =>
    route.fulfill({
      contentType: "image/svg+xml",
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="200" height="400"><rect width="200" height="400" fill="#8f9caa"/></svg>',
    }),
  );
  await page.goto("/#bulletin");
  const panel = page.locator("#page-bulletin");
  await expect(panel.locator(".bulletin-entry")).toHaveCount(3);
  const image = panel.locator(".bulletin-media img");
  await expect(image).toHaveCSS("opacity", "1");
  await expect
    .poll(async () => (await image.boundingBox())!.height)
    .toBeCloseTo(112, 0);
  const box = (await image.boundingBox())!;
  expect(box.width / box.height).toBeCloseTo(0.5, 2);
  expect(box.width).toBeLessThanOrEqual(200);
  const close = page.getByRole("button", { name: "关闭通知", exact: true });
  if (await close.count()) {
    await close.click();
    await expect(page.locator(".notification-slot")).toHaveCount(0);
  }
  await page.screenshot({ path: "test-results/bulletin-wide.png" });
  await panel.getByRole("tab", { name: "官方动态", exact: true }).click();
  await expect(
    panel.locator(".view-stack__page:not([aria-hidden]) .bulletin-entry"),
  ).toHaveCount(1);
  await expect(
    panel.locator(".view-stack__page:not([aria-hidden]) .bulletin-entry"),
  ).toContainText("B站官方动态");
  await panel.getByRole("tab", { name: "全部", exact: true }).click();
  await expect(
    panel.locator(".view-stack__page:not([aria-hidden]) .bulletin-entry"),
  ).toHaveCount(3);
  await page.setViewportSize({ width: 640, height: 560 });
  await image.scrollIntoViewIfNeeded();
  const entry = panel.locator(
    ".view-stack__page:not([aria-hidden]) .bulletin-entry.has-image",
  );
  await expect(entry).toHaveCSS("grid-template-columns", /\d/);
  await expect
    .poll(async () => {
      const media = (await entry.locator(".bulletin-media").boundingBox())!;
      const copy = (await entry
        .locator(".bulletin-entry__copy")
        .boundingBox())!;
      return copy.x >= media.x + media.width && Math.abs(copy.y - media.y) < 45;
    })
    .toBe(true);
  const overflow = await panel.locator(".bulletin-content").evaluate((el) => ({
    gutter: (el as HTMLElement).offsetWidth - el.clientWidth,
    horizontal: el.scrollWidth - el.clientWidth,
    scrollbar: getComputedStyle(el).scrollbarWidth,
  }));
  expect(overflow.gutter).toBe(0);
  expect(overflow.horizontal).toBeLessThanOrEqual(1);
  expect(overflow.scrollbar).toBe("none");
  await page.screenshot({ path: "test-results/bulletin-compact.png" });
  expect(errors).toEqual([]);
});

test("图片加载失败后仍可阅读文字公告", async ({ page }) => {
  await page.route("**/assets/hero-keyart.jpg", (route) => route.abort());
  await page.goto("/#bulletin");
  const entry = page.locator(".bulletin-entry").filter({ hasText: "情报回顾" });
  await expect(entry).toHaveCount(1);
  await expect(entry.locator("img")).toHaveCount(0);
  await expect(entry).not.toHaveClass(/has-image/);
  await expect(entry).toContainText("查看原文");
});

test("同来源公告按需堆叠, 展开与收回保持连续布局", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem(
      "nte.bulletins.v2.cn",
      JSON.stringify({
        savedAt: Date.now(),
        feed: {
          items: [
            ...Array.from({ length: 4 }, (_, index) => ({
              kind: "bilibili",
              source: "B站官方动态",
              title: [
                "《异环》新版本情报",
                "城市探索活动预告",
                "角色展示与全新委托",
                "开发者通讯",
              ][index],
              summary: "最新官方消息与活动情报, 点击查看完整公告.",
              publishedAt: `${index + 1} 小时前`,
              imageUrl: index === 0 ? "/assets/hero-keyart.jpg" : null,
              url: `https://t.bilibili.com/${index + 100}`,
            })),
            {
              kind: "preview",
              source: "版本前瞻",
              title: "版本前瞻特别节目",
              summary: "版本情报回顾",
              publishedAt: "",
              imageUrl: "/assets/hero-keyart.jpg",
              url: "https://yh.wanmei.com/preview",
            },
          ],
          errors: [],
        },
      }),
    );
  });
  await page.goto("/#bulletin");
  const panel = page.locator("#page-bulletin");
  await expect(panel.locator(".bulletin-entry")).toHaveCount(5);
  await expect(panel.locator(".bulletin-group")).toHaveCount(0);
  await expect(
    panel.getByRole("button", { name: "堆叠公告", exact: true }),
  ).toHaveCount(0);
  await page.locator('[data-page="settings"]').click();
  await page.locator('[data-category="appearance"]').click();
  await expect(page.locator("#set-stack-bulletins")).not.toBeChecked();
  await page.locator("#set-stack-bulletins").check();
  await expect(page.locator("#settings-form")).toHaveAttribute(
    "data-save-state",
    "saved",
  );
  await page.locator('[data-page="bulletin"]').click();
  const group = panel.locator(
    ".view-stack__page:not([aria-hidden]) .bulletin-group",
  );
  const toggle = group.locator(".bulletin-group__toggle");
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  const slots = group.locator(".bulletin-stack__slot");
  await expect(slots.nth(1)).toHaveAttribute("inert", "");
  await expect(
    group.getByRole("button", { name: "展开 4 条公告", exact: true }),
  ).toBeVisible();
  await expect
    .poll(async () => {
      const a = (await slots.nth(0).boundingBox())!;
      const b = (await slots.nth(1).boundingBox())!;
      return Math.abs(b.y - a.y - 10);
    })
    .toBeLessThan(0.5);
  await expect(panel.locator(".bulletin-feed > .view-stack__page")).toHaveCount(
    1,
  );
  await expect(group.locator(".bulletin-media img")).toHaveCSS("opacity", "1");
  const close = page.getByRole("button", { name: "关闭通知", exact: true });
  if (await close.count()) {
    await close.click();
    await expect(page.locator(".notification-slot")).toHaveCount(0);
  }
  await expect
    .poll(async () => {
      const stacked = (await group
        .locator(".bulletin-media")
        .first()
        .boundingBox())!;
      const single = (await panel
        .locator(
          ".view-stack__page:not([aria-hidden]) > .bulletin-entry .bulletin-media",
        )
        .boundingBox())!;
      return Math.abs(stacked.x - single.x);
    })
    .toBeLessThan(0.5);
  await page.screenshot({ path: "test-results/bulletin-stacked.png" });
  await group
    .getByRole("button", { name: "展开 4 条公告", exact: true })
    .click();
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await expect(slots.nth(1)).not.toHaveAttribute("inert", "");
  await expect
    .poll(async () => {
      const a = (await slots.nth(0).boundingBox())!;
      const b = (await slots.nth(1).boundingBox())!;
      return Math.abs(b.y - a.y - a.height - 12);
    })
    .toBeLessThan(0.5);
  await page.screenshot({ path: "test-results/bulletin-expanded.png" });
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect
    .poll(async () => {
      const a = (await slots.nth(0).boundingBox())!;
      const b = (await slots.nth(1).boundingBox())!;
      return Math.abs(b.y - a.y - 10);
    })
    .toBeLessThan(0.5);
  // 以一帧为间隔检查分类切换, 防止新卡片从零高度裁开配图。
  await pauseClock(page);
  await panel.getByRole("tab", { name: "官方动态", exact: true }).click();
  for (let frame = 0; frame < 24; frame++) {
    await page.clock.runFor(16);
    const bounds = await panel
      .locator(".view-stack__page:not([aria-hidden]) .bulletin-stack__slot")
      .first()
      .evaluate((element) => {
        const image = element.querySelector("img")!.getBoundingClientRect();
        const card = element.getBoundingClientRect();
        return {
          frameHeight: element
            .querySelector(".bulletin-media__frame")!
            .getBoundingClientRect().height,
          clipped: image.bottom - card.bottom,
        };
      });
    expect(bounds.frameHeight).toBeCloseTo(112, 1);
    expect(bounds.clipped).toBeLessThanOrEqual(0);
  }
  await page.clock.resume();
  await page.setViewportSize({ width: 640, height: 560 });
  const overflow = await panel
    .locator(".bulletin-content")
    .evaluate((el) => el.scrollWidth - el.clientWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  expect(errors).toEqual([]);
});
