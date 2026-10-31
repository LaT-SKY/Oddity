import { describe, expect, it } from "vitest";
import {
  contrastRatio,
  coverCrop,
  createAdaptiveTheme,
  formatRgb,
  formatRgba,
  relativeLuminance,
  sampleScene,
  srgbToOklab,
  type SceneSample,
} from "../../src/core/palette";

const BLACK = { r: 0, g: 0, b: 0 };
const WHITE = { r: 255, g: 255, b: 255 };

function pixels(colors: [number, number, number][]) {
  const data = new Uint8ClampedArray(colors.length * 4);
  colors.forEach(([r, g, b], index) => {
    data[index * 4] = r;
    data[index * 4 + 1] = g;
    data[index * 4 + 2] = b;
    data[index * 4 + 3] = 255;
  });
  return data;
}

function sample(overrides: Partial<SceneSample> = {}): SceneSample {
  return {
    a: -0.85,
    b: -0.35,
    chroma: 0.12,
    weight: 1,
    average: { r: 70, g: 110, b: 170 },
    dark: { r: 20, g: 40, b: 70 },
    bright: { r: 180, g: 205, b: 230 },
    ...overrides,
  };
}

describe("Oklab 转换", () => {
  it("白色与黑色落在亮度两端", () => {
    const white = srgbToOklab(1, 1, 1);
    expect(white.l).toBeCloseTo(1, 5);
    expect(white.a).toBeCloseTo(0, 5);
    expect(white.b).toBeCloseTo(0, 5);
    const black = srgbToOklab(0, 0, 0);
    expect(black.l).toBeCloseTo(0, 5);
  });
});

describe("主视觉取色", () => {
  it("按主色族取色并统计最暗与最亮区块", () => {
    const data = pixels([
      [30, 60, 180],
      [30, 60, 180],
      [30, 60, 180],
      [200, 120, 40],
      [20, 20, 20],
      [240, 240, 235],
    ]);
    const result = sampleScene(data, 3, 2)!;
    expect(result).not.toBeNull();
    expect(result.b).toBeLessThan(0);
    expect(result.weight).toBeGreaterThan(0);
    expect(relativeLuminance(result.dark)).toBeLessThan(
      relativeLuminance(result.bright),
    );
    expect(result.average.r).toBeGreaterThan(0);
  });

  it("没有彩度时保留中性结果", () => {
    const result = sampleScene(
      pixels([
        [128, 128, 128],
        [64, 64, 64],
        [200, 200, 200],
      ]),
      3,
      1,
    )!;
    expect(result.chroma).toBe(0);
    expect(result.a).toBe(0);
    expect(result.b).toBe(0);
    expect(result.dark).not.toBeNull();
  });

  it("无效输入返回空结果", () => {
    expect(sampleScene(new Uint8ClampedArray(0), 0, 1)).toBeNull();
    expect(sampleScene(new Uint8ClampedArray(4), 4, 1)).toBeNull();
    expect(sampleScene(new Uint8ClampedArray(4), Number.NaN, 1)).toBeNull();
    const transparent = new Uint8ClampedArray(16);
    expect(sampleScene(transparent, 2, 2)).toBeNull();
  });
});

describe("自适应主题", () => {
  it("蓝色主视觉生成冷色主题并达到文字对比度目标", () => {
    const theme = createAdaptiveTheme(sample())!;
    expect(relativeLuminance(theme.tintLight)).toBeGreaterThan(0.7);
    expect(relativeLuminance(theme.tintDark)).toBeLessThan(0.1);
    expect(relativeLuminance(theme.light.ink)).toBeLessThan(
      relativeLuminance(theme.light.faint),
    );
    expect(relativeLuminance(theme.dark.ink)).toBeGreaterThan(
      relativeLuminance(theme.dark.faint),
    );
    expect(
      contrastRatio(theme.light.ink, theme.background.light),
    ).toBeGreaterThanOrEqual(7);
    // 极亮的主视觉上 7:1 不可达, 暗色正文保底 AA。
    expect(
      contrastRatio(theme.dark.ink, theme.background.dark),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.light.muted, theme.background.light),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.dark.muted, theme.background.dark),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.light.faint, theme.background.light),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.dark.faint, theme.background.dark),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.light.accent, theme.background.light),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.dark.accent, theme.background.dark),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.light.accentFill, theme.light.accentFg),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.dark.accentFill, theme.dark.accentFg),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(theme.light.primaryFg, theme.light.accentSoft),
    ).toBeGreaterThanOrEqual(7);
    expect(
      contrastRatio(theme.dark.primaryFg, theme.dark.accentSoft),
    ).toBeGreaterThanOrEqual(7);
  });

  it("暖色主视觉生成暖色主题", () => {
    const theme = createAdaptiveTheme(
      sample({
        a: 0.7,
        b: 0.5,
        chroma: 0.14,
        average: { r: 200, g: 120, b: 60 },
        dark: { r: 90, g: 40, b: 10 },
        bright: { r: 240, g: 190, b: 120 },
      }),
    )!;
    expect(theme.tintLight.r).toBeGreaterThan(theme.tintLight.b);
    expect(theme.tintDark.r).toBeGreaterThan(theme.tintDark.b);
    expect(theme.light.accent.r).toBeGreaterThan(theme.light.accent.b);
  });

  it("低彩度主视觉退化为中性主题", () => {
    const theme = createAdaptiveTheme(
      sample({
        a: 0,
        b: 0,
        chroma: 0,
        average: { r: 120, g: 120, b: 120 },
        dark: { r: 40, g: 40, b: 40 },
        bright: { r: 200, g: 200, b: 200 },
      }),
    )!;
    expect(Math.abs(theme.tintLight.r - theme.tintLight.b)).toBeLessThan(3);
    expect(Math.abs(theme.tintDark.r - theme.tintDark.b)).toBeLessThan(3);
    expect(
      contrastRatio(theme.light.ink, theme.background.light),
    ).toBeGreaterThanOrEqual(7);
  });

  it("缺失或非有限样本返回空主题", () => {
    expect(createAdaptiveTheme(null)).toBeNull();
    expect(createAdaptiveTheme(sample({ a: Number.NaN, b: 0 }))).toBeNull();
  });
});

describe("裁切与颜色字符串", () => {
  it("cover 裁切按焦点保留画面中心", () => {
    expect(coverCrop(1920, 1080, 1280, 720, 0.54, 0.38)).toEqual({
      x: 0,
      y: 0,
      width: 1920,
      height: 1080,
    });
    const square = coverCrop(1920, 1080, 800, 800, 0.54, 0.38)!;
    expect(square.width).toBeCloseTo(1080);
    expect(square.height).toBeCloseTo(1080);
    expect(square.x).toBeCloseTo(453.6);
    expect(square.y).toBeCloseTo(0);
    const tall = coverCrop(800, 800, 1920, 1080, 2, -1)!;
    expect(tall.width).toBeCloseTo(800);
    expect(tall.height).toBeCloseTo(450);
    expect(tall.x).toBeCloseTo(0);
    expect(tall.y).toBeCloseTo(0);
    expect(coverCrop(800, 800, 1920, 1080, 0.5, 0.5)!.y).toBeCloseTo(175);
    expect(coverCrop(0, 1080, 800, 800)).toBeNull();
    expect(coverCrop(1920, 1080, 0, 800)).toBeNull();
  });

  it("对比度与颜色字符串", () => {
    expect(relativeLuminance(WHITE)).toBeCloseTo(1, 5);
    expect(relativeLuminance(BLACK)).toBeCloseTo(0, 5);
    expect(contrastRatio(WHITE, BLACK)).toBeCloseTo(21, 5);
    expect(formatRgb({ r: 1, g: 2, b: 3 })).toBe("rgb(1 2 3)");
    expect(formatRgba({ r: 1, g: 2, b: 3 }, 0.5)).toBe("rgba(1, 2, 3, 0.5)");
  });
});
