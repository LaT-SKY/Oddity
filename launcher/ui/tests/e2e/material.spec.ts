import { expect, test, type Page } from "@playwright/test";

interface Material {
  tintLight: string;
  tintDark: string;
  accentText: string;
  bone: string;
  muted: string;
  background: string;
  alpha: number;
  backdrop: string;
  veil: string;
  contrast: number;
}

// 解析 color(srgb ... / a) 与 rgba(...), 按最坏底色估算正文对比度。
async function readMaterial(page: Page, backdrop: number): Promise<Material> {
  return page.evaluate((backdropValue) => {
    const parse = (value: string) => {
      const modern = value.match(
        /^color\(srgb\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*\/\s*([\d.]+))?\)$/,
      );
      if (modern)
        return {
          r: Number(modern[1]) * 255,
          g: Number(modern[2]) * 255,
          b: Number(modern[3]) * 255,
          a: modern[4] ? Number(modern[4]) : 1,
        };
      const legacy = value.match(/^rgba?\(([^)]+)\)$/);
      if (!legacy) return null;
      const parts = legacy[1]
        .split(/[\s,/]+/)
        .filter(Boolean)
        .map(Number);
      return { r: parts[0], g: parts[1], b: parts[2], a: parts[3] ?? 1 };
    };
    const luminance = (color: { r: number; g: number; b: number }) => {
      const linear = (value: number) => {
        const channel = value / 255;
        return channel <= 0.04045
          ? channel / 12.92
          : Math.pow((channel + 0.055) / 1.055, 2.4);
      };
      return (
        0.2126 * linear(color.r) +
        0.7152 * linear(color.g) +
        0.0722 * linear(color.b)
      );
    };
    const drawer = document.querySelector<HTMLElement>(".drawer")!;
    const background = parse(getComputedStyle(drawer).backgroundColor)!;
    const bone = parse(getComputedStyle(drawer).getPropertyValue("--bone"))!;
    const solid = {
      r: background.r * background.a + backdropValue * (1 - background.a),
      g: background.g * background.a + backdropValue * (1 - background.a),
      b: background.b * background.a + backdropValue * (1 - background.a),
    };
    const first = luminance(solid);
    const second = luminance(bone);
    const root = getComputedStyle(document.documentElement);
    return {
      tintLight: root.getPropertyValue("--scene-tint-light").trim(),
      tintDark: root.getPropertyValue("--scene-tint-dark").trim(),
      accentText: root.getPropertyValue("--accent-text").trim(),
      bone: root.getPropertyValue("--bone").trim(),
      muted: root.getPropertyValue("--muted").trim(),
      background: getComputedStyle(drawer).backgroundColor,
      alpha: background.a,
      backdrop:
        getComputedStyle(drawer).backdropFilter ||
        getComputedStyle(drawer).getPropertyValue("-webkit-backdrop-filter"),
      veil: getComputedStyle(drawer, "::before").backgroundImage,
      contrast:
        (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05),
    };
  }, backdrop);
}

async function waitForTint(page: Page) {
  // 内联变量由取色写入, 回退值不会触发通过。
  await expect
    .poll(
      () =>
        page.evaluate(() =>
          document.documentElement.style.getPropertyValue("--scene-tint-light"),
        ),
      { timeout: 12_000 },
    )
    .toMatch(/^rgb\(\d+ \d+ \d+\)$/);
}

test("二级界面使用主视觉取色的磨砂材质并保持正文对比度", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#app")).toHaveClass(/is-intro-done/);
  await waitForTint(page);
  await page.locator('[data-page="settings"]').click();
  await expect(page.locator("#page-settings")).toBeVisible();
  await expect(page.locator(".drawer")).toHaveCSS("backdrop-filter", /blur\(/);

  const light = await readMaterial(page, 0);
  expect(light.tintLight).toMatch(/^rgb\(\d+ \d+ \d+\)$/);
  expect(light.tintDark).toMatch(/^rgb\(\d+ \d+ \d+\)$/);
  expect(light.accentText).toMatch(/^rgb\(\d+ \d+ \d+\)$/);
  expect(light.bone).toMatch(/^rgb\(\d+ \d+ \d+\)$/);
  expect(light.alpha).toBeGreaterThan(0.55);
  expect(light.alpha).toBeLessThan(0.68);
  expect(light.backdrop).toContain("56px");
  expect(light.veil).toContain("linear-gradient");
  expect(light.contrast).toBeGreaterThanOrEqual(4.5);
  expect(light.tintLight).not.toBe(light.tintDark);

  await page.locator("#set-theme-trigger").click();
  await page.getByRole("option", { name: "暗色", exact: true }).click();
  await expect(page.locator("#app")).toHaveAttribute("data-theme", "dark");
  await page.waitForTimeout(200);

  // 暗色正文按主视觉最亮区块估算, 这里用偏亮的中间色代表亮部。
  // 暗色正文按场景遮罩后的亮部估算, 这里用中间偏亮的底色代表亮部。
  const dark = await readMaterial(page, 128);
  expect(dark.background).not.toBe(light.background);
  expect(dark.alpha).toBeGreaterThan(0.55);
  expect(dark.alpha).toBeLessThan(0.68);
  expect(dark.contrast).toBeGreaterThanOrEqual(4.5);
  expect(dark.bone).not.toBe(light.bone);

  // 关闭跟随主视觉后回到静态主题, 再打开时重新取色。
  await page.locator("#set-adaptive-theme").uncheck();
  await expect
    .poll(() =>
      page.evaluate(() =>
        document.documentElement.style.getPropertyValue("--scene-tint-light"),
      ),
    )
    .toBe("");
  await expect(page.locator("#set-adaptive-theme")).not.toBeChecked();
  await page.locator("#set-adaptive-theme").check();
  await waitForTint(page);
});
