import { pauseClock } from "./clock";
import { settingsCategory } from "./settings-helpers";
import { test, expect } from "@playwright/test";

test("应用名称下方生长、通知堆叠、展开与单主要动作", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  const stack = page.getByRole("region", { name: "通知", exact: true });
  await expect(stack).toBeVisible();
  await page.mouse.move(500, 480);
  await expect(stack).toHaveAttribute("data-phase", "0");
  const anchor = await page.locator(".titlebar__brand").boundingBox();
  const box = await stack.boundingBox();
  expect(Math.abs(box!.x - anchor!.x)).toBeLessThanOrEqual(1);
  expect(box!.y).toBeGreaterThan(anchor!.y + anchor!.height);
  expect(box!.y).toBeLessThan(80);
  await page.locator("#launch-btn").click();
  const cards = page.locator(".notification-card");
  await expect(cards).toHaveCount(2);
  await expect(stack).toHaveAttribute("data-phase", "0");
  const front = cards.first();
  await expect(front.locator(".notification-action--primary")).toHaveCount(1);
  await expect(
    front.locator(".notification-action--primary svg use"),
  ).toHaveAttribute("href", "#i-gear-six");
  await page.locator("#settings-redetect").click();
  await expect(cards).toHaveCount(3);
  await expect(stack).toHaveAttribute("data-phase", "0");
  const depths = await page
    .locator(".notification-depth")
    .evaluateAll((elements) =>
      elements.map((el) => {
        const matrix = new DOMMatrixReadOnly(getComputedStyle(el).transform);
        return { scale: matrix.a, y: matrix.m42 };
      }),
    );
  expect(depths[1].scale).toBeLessThan(1);
  await page.getByRole("button", { name: "展开 3 条通知" }).click();
  await expect(stack).toHaveClass(/is-expanded/);
  await expect
    .poll(async () => {
      const rects = await cards.evaluateAll((elements) =>
        elements.map((el) => el.getBoundingClientRect().toJSON()),
      );
      return (
        rects[1].top >= rects[0].bottom + 8 &&
        rects[2].top >= rects[1].bottom + 8
      );
    })
    .toBe(true);
  await page.getByRole("button", { name: "收起", exact: true }).click();
  await expect(stack).not.toHaveClass(/is-expanded/);
  await page
    .getByRole("button", { name: "关闭通知", exact: true })
    .first()
    .click();
  await expect(cards).toHaveCount(2);
  await expect(
    cards.first().locator(".notification-action--primary"),
  ).toHaveText("打开设置");
  await cards
    .first()
    .getByRole("button", { name: "打开设置", exact: true })
    .click();
  await expect(page.locator("#page-settings")).toHaveClass(/is-active/);
  await expect(cards).toHaveCount(1);
  await page.getByRole("button", { name: "关闭通知", exact: true }).click();
  await expect(stack).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("减少动效、重复通知合并与键盘阅读", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/#settings");
  await page.locator("#settings-redetect").click();
  await page.locator("#settings-redetect").click();
  await expect(page.locator(".notification-repeat").first()).toHaveText("×2");
  const count = await page.locator(".notification-slot").count();
  await page.getByRole("button", { name: `展开 ${count} 条通知` }).click();
  await page.getByRole("button", { name: "全部清除", exact: true }).focus();
  await page.keyboard.press("Escape");
  await expect(page.locator(".notification-stack")).not.toHaveClass(
    /is-expanded/,
  );
  await page.getByRole("button", { name: `展开 ${count} 条通知` }).click();
  await page.getByRole("button", { name: "全部清除", exact: true }).click();
  await expect(page.locator(".notification-stack")).toHaveCount(0);
});

test("圆球、正方形过冲与矩形归位", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  // 评测机负载抖动会跳过中间帧, 固定时钟后逐帧推进动画。
  await pauseClock(page);
  await page.goto("/");
  await page.clock.runFor(2500);
  const stack = page.locator(".notification-stack");
  await page.locator("#launch-btn").click();
  const frames: {
    phase: number;
    width: number;
    height: number;
    x: number;
    y: number;
  }[] = [];
  for (let step = 0; step < 220; step++) {
    await page.clock.runFor(16);
    const frame = await page.evaluate(() => {
      const root = document.querySelector<HTMLElement>(".notification-stack");
      const shell = root?.querySelector<HTMLElement>(
        ".is-arriving .notification-surface",
      );
      if (!root || !shell) return null;
      const box = shell.getBoundingClientRect();
      return {
        phase: Number(root.dataset.phase),
        width: box.width,
        height: box.height,
        x: box.left + box.width / 2,
        y: box.top + box.height / 2,
      };
    });
    if (frame) frames.push(frame);
    else if (frames.length) break;
  }
  const ball = frames.find((frame) => frame.width <= 25);
  expect(ball).toBeTruthy();
  expect(Math.abs(ball!.width - ball!.height)).toBeLessThan(1);
  const squares = frames.filter((frame) => frame.phase === 2);
  expect(squares.length).toBeGreaterThanOrEqual(3);
  expect(
    squares.every((frame) => Math.abs(frame.width - frame.height) < 1),
  ).toBe(true);
  expect(Math.min(...squares.map((frame) => frame.x))).toBeLessThan(
    ball!.x - 16,
  );
  expect(Math.max(...squares.map((frame) => frame.y))).toBeGreaterThan(
    ball!.y + 25,
  );
  expect(Math.min(...frames.map((frame) => frame.x))).toBeLessThan(
    ball!.x - 24,
  );
  await expect(stack).toHaveAttribute("data-phase", "0");
  // 指针停在通知上暂停自动关闭计时, 后续尺寸检查期间通知不会消失。
  await stack.hover();
  const front = page.locator(
    '.notification-slot[data-front="true"] .notification-surface',
  );
  const rect = await front.boundingBox();
  expect(rect!.width).toBeCloseTo(352, 0);
  expect(rect!.height).toBeLessThan(rect!.width);
  expect(rect!.y + rect!.height / 2).toBeLessThan(
    Math.max(...frames.map((frame) => frame.y)) - 2,
  );
  const button = front.locator(".notification-action--primary");
  const compact = await button.boundingBox();
  expect(compact!.width).toBeLessThanOrEqual(34);
  await button.hover();
  await page.clock.runFor(600);
  const expanded = await button.boundingBox();
  expect(expanded!.width).toBeGreaterThan(70);
  expect(Math.abs(expanded!.x - compact!.x)).toBeLessThan(0.6);
  await page.mouse.move(600, 500);
  await page.clock.runFor(600);
  expect((await button.boundingBox())!.width).toBeLessThanOrEqual(34);
  await page.clock.resume();
});
test("运动中切换减少动效立即归位, 清空后可以再次入场", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.goto("/");
  const stack = page.locator(".notification-stack");
  await expect(stack.locator(".is-arriving")).toHaveCount(1);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await expect(stack).toHaveAttribute("data-phase", "0");
  await expect(stack.locator(".is-arriving")).toHaveCount(0);
  const surface = stack.locator(".notification-surface").first();
  const geometry = await surface.evaluate((el) => ({
    width: el.getBoundingClientRect().width,
    x: new DOMMatrixReadOnly(getComputedStyle(el).transform).m41,
    y: new DOMMatrixReadOnly(getComputedStyle(el).transform).m42,
  }));
  expect(geometry.width).toBeCloseTo(352, 0);
  expect(geometry.x).toBe(0);
  expect(geometry.y).toBe(0);
  await page.getByRole("button", { name: "关闭通知", exact: true }).click();
  await expect(stack).toHaveCount(0);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.locator("#launch-btn").click();
  await expect(stack).toHaveAttribute("data-phase", "0");
  await expect(stack.locator(".notification-surface")).toHaveCount(1);
  await expect(
    stack.getByRole("button", { name: "打开设置", exact: true }),
  ).toBeVisible();
});

test("最后一条通知沿弧线缩回图标, 动画结束后再移除", async ({ page }) => {
  await page.goto("/");
  const stack = page.locator(".notification-stack");
  await expect(stack).toHaveAttribute("data-phase", "0");
  await page.evaluate(() => {
    const probe = {
      frames: [] as {
        width: number;
        height: number;
        x: number;
        y: number;
        opacity: number;
      }[],
      done: false,
    };
    (window as unknown as { departureProbe: typeof probe }).departureProbe =
      probe;
    function sample() {
      const shell = document.querySelector<HTMLElement>(
        ".is-retiring .notification-surface",
      );
      if (shell) {
        const box = shell.getBoundingClientRect();
        probe.frames.push({
          width: box.width,
          height: box.height,
          x: box.left + box.width / 2,
          y: box.top + box.height / 2,
          opacity: Number(getComputedStyle(shell).opacity),
        });
      }
    }
    // 按 DOM 变更采样可以记录动画结束前的最后一帧, 卸载前的位置才是真实终点。
    const observer = new MutationObserver(sample);
    observer.observe(document.body, {
      attributes: true,
      attributeFilter: ["class", "style"],
      subtree: true,
    });
    setTimeout(() => {
      observer.disconnect();
      probe.done = true;
    }, 1600);
  });
  await stack.getByRole("button", { name: "关闭通知", exact: true }).click();
  await page.waitForFunction(
    () =>
      (window as unknown as { departureProbe: { done: boolean } })
        .departureProbe.done,
  );
  const frames = await page.evaluate(
    () =>
      (
        window as unknown as {
          departureProbe: {
            frames: {
              width: number;
              height: number;
              x: number;
              y: number;
              opacity: number;
            }[];
          };
        }
      ).departureProbe.frames,
  );
  expect(frames.length).toBeGreaterThanOrEqual(4);
  expect(Math.max(...frames.map((frame) => frame.width))).toBeGreaterThan(250);
  expect(Math.min(...frames.map((frame) => frame.width))).toBeLessThan(22);
  const last = frames.at(-1)!;
  const logo = await page.locator(".brand-mark").boundingBox();
  expect(Math.abs(last.x - logo!.x - logo!.width / 2)).toBeLessThan(5);
  expect(Math.abs(last.y - logo!.y - logo!.height - 12)).toBeLessThan(5);
  expect(last.opacity).toBeLessThan(0.5);
  await expect(stack).toHaveCount(0);
});

test("连续启动和关闭时, 旧通知只下移一层且不跳到下一行", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/#settings");
  await settingsCategory(page, "runtime");
  await page.getByRole("button", { name: "创建运行环境", exact: true }).click();
  await page.locator("#runtime-environment-name").fill("通知动画验证");
  await page.locator("#runtime-environment-proton").fill("/opt/Proton");
  await page.getByRole("button", { name: "创建环境", exact: true }).click();
  const stack = page.locator(".notification-stack");
  const expand = page.getByRole("button", { name: /展开 \d+ 条通知/ });
  if (await expand.count()) {
    await expand.click();
    await page.getByRole("button", { name: "全部清除", exact: true }).click();
  } else {
    await stack.getByRole("button", { name: "关闭通知", exact: true }).click();
  }
  await expect(stack).toHaveCount(0);
  await page.locator('[data-page="home"]').click();
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.locator("#launch-btn").click();
  // 启动页提示会在阶段推进中到期, 悬停暂停计时以保留旧通知。
  await page.locator(".notification-stack").hover();
  await expect(page.locator("#launch-btn")).toHaveText(/关闭游戏/);
  await expect(stack).toHaveAttribute("data-phase", "0");
  const oldId = await page
    .locator('.notification-slot[data-front="true"]')
    .getAttribute("data-notification-id");
  await page.evaluate((oldId) => {
    const old = document.querySelector<HTMLElement>(
      `[data-notification-id="${oldId}"] .notification-depth`,
    )!;
    const baseline = old.getBoundingClientRect();
    const probe = {
      baseline: { top: baseline.top, height: baseline.height },
      frames: [] as {
        phase: number;
        shift: number;
        height: number;
        centerX: number;
        centerY: number;
      }[],
      done: false,
    };
    (window as unknown as { multiNoticeProbe: typeof probe }).multiNoticeProbe =
      probe;
    const start = performance.now();
    function sample() {
      const root = document.querySelector<HTMLElement>(".notification-stack");
      const incoming = root?.querySelector<HTMLElement>(
        ".is-arriving .notification-surface",
      );
      if (root && incoming && old.isConnected) {
        const rect = old.getBoundingClientRect();
        const fresh = incoming.getBoundingClientRect();
        probe.frames.push({
          phase: Number(root.dataset.phase),
          shift: rect.top - baseline.top,
          height: rect.height,
          centerX: fresh.left + fresh.width / 2,
          centerY: fresh.top + fresh.height / 2,
        });
      }
      if (performance.now() - start < 1800) requestAnimationFrame(sample);
      else probe.done = true;
    }
    requestAnimationFrame(sample);
  }, oldId);
  // 用键盘关闭, 保持指针停留在通知区域以继续暂停计时。
  await page.locator("#launch-btn").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(
    () =>
      (window as unknown as { multiNoticeProbe: { done: boolean } })
        .multiNoticeProbe.done,
  );
  const probe = await page.evaluate(
    () =>
      (
        window as unknown as {
          multiNoticeProbe: {
            baseline: { top: number; height: number };
            frames: {
              phase: number;
              shift: number;
              height: number;
              centerX: number;
              centerY: number;
            }[];
          };
        }
      ).multiNoticeProbe,
  );
  expect(probe.frames.length).toBeGreaterThan(5);
  expect(
    probe.frames
      .filter((frame) => frame.phase === 1 || frame.phase === 2)
      .every((frame) => Math.abs(frame.shift) < 0.6),
  ).toBe(true);
  expect(Math.max(...probe.frames.map((frame) => frame.shift))).toBeLessThan(
    20,
  );
  expect(Math.max(...probe.frames.map((frame) => frame.height))).toBeLessThan(
    probe.baseline.height + 1,
  );
  await expect(stack).toHaveAttribute("data-phase", "0");
  const settled = await page
    .locator('.notification-slot[data-front="true"] .notification-surface')
    .boundingBox();
  const last = probe.frames.at(-1)!;
  expect(
    Math.hypot(
      last.centerX - settled!.x - settled!.width / 2,
      last.centerY - settled!.y - settled!.height / 2,
    ),
  ).toBeLessThan(12);
  const old = await page
    .locator(`[data-notification-id="${oldId}"] .notification-depth`)
    .boundingBox();
  expect(old!.y - probe.baseline.top).toBeCloseTo(10, 0);
  await stack.getByRole("button", { name: /展开 \d+ 条通知/ }).click();
  await expect(stack).toHaveClass(/is-expanded/);
  const surfaces = page.locator(".notification-depth");
  await expect
    .poll(async () => {
      const rects = await surfaces.evaluateAll((elements) =>
        elements.map((el) => el.getBoundingClientRect().toJSON()),
      );
      return rects[1].top >= rects[0].bottom + 8;
    })
    .toBe(true);
  await page.getByRole("button", { name: "收起", exact: true }).click();
  await expect
    .poll(async () => {
      const rects = await surfaces.evaluateAll((elements) =>
        elements.map((el) => el.getBoundingClientRect().toJSON()),
      );
      return Math.abs(rects[1].top - rects[0].top - 10) < 0.6;
    })
    .toBe(true);
});

test("展开和收起不产生滚动条占位, 超出可用高度时仍能滚动", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await page.locator("#launch-btn").click();
  await page.locator("#settings-redetect").click();
  const stack = page.locator(".notification-stack");
  await expect(page.locator(".notification-card")).toHaveCount(3);
  await page.setViewportSize({ width: 1180, height: 320 });
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.evaluate(() => {
    const probe = {
      frames: [] as { gutter: number; width: number; scrollbar: string }[],
      done: false,
    };
    (
      window as unknown as { notificationScrollProbe: typeof probe }
    ).notificationScrollProbe = probe;
    const start = performance.now();
    function sample() {
      const el = document.querySelector<HTMLElement>(".notification-stack")!;
      probe.frames.push({
        gutter: el.offsetWidth - el.clientWidth,
        width: el.clientWidth,
        scrollbar: getComputedStyle(el).scrollbarWidth,
      });
      if (performance.now() - start < 900) requestAnimationFrame(sample);
      else probe.done = true;
    }
    requestAnimationFrame(sample);
  });
  await stack.getByRole("button", { name: "展开 3 条通知" }).click();
  await page.waitForFunction(
    () =>
      (window as unknown as { notificationScrollProbe: { done: boolean } })
        .notificationScrollProbe.done,
  );
  const frames = await page.evaluate(
    () =>
      (
        window as unknown as {
          notificationScrollProbe: {
            frames: { gutter: number; width: number; scrollbar: string }[];
          };
        }
      ).notificationScrollProbe.frames,
  );
  expect(frames.every((frame) => frame.gutter === 0)).toBe(true);
  expect(new Set(frames.map((frame) => frame.width)).size).toBe(1);
  const scroll = await stack.evaluate((el) => ({
    height: el.clientHeight,
    content: el.scrollHeight,
    scrollbar: getComputedStyle(el).scrollbarWidth,
    webkitScrollbar: getComputedStyle(el, "::-webkit-scrollbar").display,
  }));
  expect(scroll.content).toBeGreaterThan(scroll.height);
  expect(scroll.scrollbar).toBe("none");
  expect(scroll.webkitScrollbar).toBe("none");
  await stack.hover();
  await page.mouse.wheel(0, 180);
  await expect
    .poll(() => stack.evaluate((el) => el.scrollTop))
    .toBeGreaterThan(0);
  await stack.getByRole("button", { name: "收起", exact: true }).click();
  await expect.poll(() => stack.evaluate((el) => el.scrollTop)).toBe(0);
  expect(
    await stack.evaluate(
      (el) => (el as HTMLElement).offsetWidth - el.clientWidth,
    ),
  ).toBe(0);
});
