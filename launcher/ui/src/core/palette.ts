// 主视觉自适应主题。
// 色相来自当前可见裁切的主色族, 明度与对比度按磨砂玻璃的实际合成色计算。

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface SceneSample {
  a: number;
  b: number;
  chroma: number;
  weight: number;
  average: Rgb;
  dark: Rgb;
  bright: Rgb;
}

export interface ThemeRoles {
  ink: Rgb;
  soft: Rgb;
  muted: Rgb;
  faint: Rgb;
  accent: Rgb;
  accentFill: Rgb;
  accentFg: Rgb;
  accentSoft: Rgb;
  primaryFg: Rgb;
  primaryHover: Rgb;
}

export interface AdaptiveTheme {
  tintLight: Rgb;
  tintDark: Rgb;
  glowLight: Rgb;
  glowDark: Rgb;
  heroAccent: Rgb;
  background: {
    light: Rgb;
    dark: Rgb;
  };
  light: ThemeRoles;
  dark: ThemeRoles;
}

interface Oklab {
  l: number;
  a: number;
  b: number;
}

interface LinearRgb {
  r: number;
  g: number;
  b: number;
}

interface Block {
  color: Rgb;
  luminance: number;
}

interface Crop {
  x: number;
  y: number;
  width: number;
  height: number;
}

// 玻璃与内容薄纱的传输率。薄纱让文字区比边缘略实一档。
const GLASS_FILL = 0.6;
const GLASS_VEIL = 0.12;
const DARK_VEIL = 0.15;
const EFFECTIVE_FILL = 1 - (1 - GLASS_FILL) * (1 - GLASS_VEIL);
const EFFECTIVE_DARK = 1 - (1 - GLASS_FILL) * (1 - DARK_VEIL);
// 二级界面打开时场景遮罩保留 66% 的画面。
const SCENE_SHADE = 0.66;
const LIGHT_TINT_L = 0.97;
const DARK_TINT_L = 0.24;
const LIGHT_GLOW_L = 0.64;
const DARK_GLOW_L = 0.8;
const HERO_ACCENT_L = 0.84;
const MAX_ACCENT_C = 0.1;
const MAX_TINT_C = 0.035;
const MAX_DARK_TINT_C = 0.045;
const MAX_SOFT_C = 0.03;
// 正文目标 7.5:1, 辅助文字至少 4.6:1; 极亮或极暗画面按可达上限回落。
const INK_TARGET = 7.5;
const SOFT_TARGET = 6.2;
const MUTED_TARGET = 5.2;
const FAINT_TARGET = 4.6;
const ACCENT_TARGET = 5;
const FILL_FG_TARGET = 4.6;
const FILL_BG_TARGET = 2.4;
const LIGHT_FG: Rgb = { r: 247, g: 249, b: 251 };
const DARK_FG: Rgb = { r: 17, g: 23, b: 31 };
const HUE_BINS = 12;
const BLOCK_COLUMNS = 4;
const BLOCK_ROWS = 3;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function srgbToLinear(value: number): number {
  return value <= 0.04045
    ? value / 12.92
    : Math.pow((value + 0.055) / 1.055, 2.4);
}

function linearToSrgb(value: number): number {
  const clamped = clamp(value, 0, 1);
  return clamped <= 0.0031308
    ? clamped * 12.92
    : 1.055 * Math.pow(clamped, 1 / 2.4) - 0.055;
}

// Oklab 参考实现: https://bottosson.github.io/posts/oklab/
export function srgbToOklab(r: number, g: number, b: number): Oklab {
  const lr = srgbToLinear(r);
  const lg = srgbToLinear(g);
  const lb = srgbToLinear(b);
  const l = Math.cbrt(
    0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb,
  );
  const m = Math.cbrt(
    0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb,
  );
  const s = Math.cbrt(
    0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb,
  );
  return {
    l: 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    a: 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    b: 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  };
}

function oklabToLinear(l: number, a: number, b: number): LinearRgb {
  const lRoot = l + 0.3963377774 * a + 0.2158037573 * b;
  const mRoot = l - 0.1055613458 * a - 0.0638541728 * b;
  const sRoot = l - 0.0894841775 * a - 1.291485548 * b;
  const lCube = lRoot * lRoot * lRoot;
  const mCube = mRoot * mRoot * mRoot;
  const sCube = sRoot * sRoot * sRoot;
  return {
    r: 4.0767416621 * lCube - 3.3077115913 * mCube + 0.2309699292 * sCube,
    g: -1.2684380046 * lCube + 2.6097574011 * mCube - 0.3413193965 * sCube,
    b: -0.0041960863 * lCube - 0.7034186147 * mCube + 1.707614701 * sCube,
  };
}

function inGamut({ r, g, b }: LinearRgb): boolean {
  const epsilon = 1e-4;
  return (
    r >= -epsilon &&
    r <= 1 + epsilon &&
    g >= -epsilon &&
    g <= 1 + epsilon &&
    b >= -epsilon &&
    b <= 1 + epsilon
  );
}

function encode({ r, g, b }: LinearRgb): Rgb {
  return {
    r: Math.round(clamp(linearToSrgb(r), 0, 1) * 255),
    g: Math.round(clamp(linearToSrgb(g), 0, 1) * 255),
    b: Math.round(clamp(linearToSrgb(b), 0, 1) * 255),
  };
}

// 目标明度与彩度可能落在 sRGB 之外, 逐步降低彩度而不是直接裁剪通道。
function oklchToRgb(l: number, chroma: number, a: number, b: number): Rgb {
  const magnitude = Math.hypot(a, b);
  const dirA = magnitude > 1e-6 ? a / magnitude : 0;
  const dirB = magnitude > 1e-6 ? b / magnitude : 0;
  let scale = 1;
  for (let attempt = 0; attempt < 12; attempt++) {
    const linear = oklabToLinear(
      l,
      dirA * chroma * scale,
      dirB * chroma * scale,
    );
    if (inGamut(linear)) return encode(linear);
    scale *= 0.85;
  }
  const gray = l * l * l;
  return encode({ r: gray, g: gray, b: gray });
}

function scaleSrgb(color: Rgb, factor: number): Rgb {
  return {
    r: Math.round(clamp(color.r * factor, 0, 255)),
    g: Math.round(clamp(color.g * factor, 0, 255)),
    b: Math.round(clamp(color.b * factor, 0, 255)),
  };
}

function brightenSrgb(color: Rgb, amount: number): Rgb {
  const shift = amount * 255;
  return {
    r: Math.round(clamp(color.r + shift, 0, 255)),
    g: Math.round(clamp(color.g + shift, 0, 255)),
    b: Math.round(clamp(color.b + shift, 0, 255)),
  };
}

function overlay(front: Rgb, fill: number, backdrop: Rgb): Rgb {
  const mix = (frontValue: number, backValue: number) =>
    Math.round(frontValue * fill + backValue * (1 - fill));
  return {
    r: mix(front.r, backdrop.r),
    g: mix(front.g, backdrop.g),
    b: mix(front.b, backdrop.b),
  };
}

export function relativeLuminance({ r, g, b }: Rgb): number {
  return (
    0.2126 * srgbToLinear(r / 255) +
    0.7152 * srgbToLinear(g / 255) +
    0.0722 * srgbToLinear(b / 255)
  );
}

export function contrastRatio(first: Rgb, second: Rgb): number {
  const a = relativeLuminance(first);
  const b = relativeLuminance(second);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

function pickText(
  background: Rgb,
  target: number,
  chroma: number,
  a: number,
  b: number,
  mode: "light" | "dark",
  lift = 0,
): Rgb {
  if (mode === "light") {
    for (let l = 0.98; l >= 0.02; l -= 0.005) {
      const color = oklchToRgb(l, chroma, a, b);
      if (contrastRatio(color, background) >= target) return color;
    }
    return oklchToRgb(0.02, chroma, a, b);
  }
  for (let l = 0.02; l <= 0.98; l += 0.005) {
    const color = oklchToRgb(l, chroma, a, b);
    if (contrastRatio(color, background) >= target)
      return oklchToRgb(Math.min(0.98, l + lift), chroma, a, b);
  }
  return oklchToRgb(0.98, chroma, a, b);
}

// 选择层与开关需要一块能承托前景文字的实色, 同时与玻璃保持可见差异。
function pickFill(
  background: Rgb,
  foreground: Rgb,
  chroma: number,
  a: number,
  b: number,
  mode: "light" | "dark",
): Rgb {
  if (mode === "light") {
    for (let l = 0.72; l >= 0.22; l -= 0.005) {
      const color = oklchToRgb(l, chroma, a, b);
      if (
        contrastRatio(color, foreground) >= FILL_FG_TARGET &&
        contrastRatio(color, background) >= FILL_BG_TARGET
      )
        return color;
    }
    return oklchToRgb(0.4, chroma, a, b);
  }
  for (let l = 0.32; l <= 0.92; l += 0.005) {
    const color = oklchToRgb(l, chroma, a, b);
    if (
      contrastRatio(color, foreground) >= FILL_FG_TARGET &&
      contrastRatio(color, background) >= FILL_BG_TARGET
    )
      return color;
  }
  return oklchToRgb(0.55, chroma, a, b);
}

function blockAverages(
  data: Uint8ClampedArray,
  width: number,
  height: number,
): Block[] {
  const blocks: Block[] = [];
  const blockWidth = Math.max(1, Math.floor(width / BLOCK_COLUMNS));
  const blockHeight = Math.max(1, Math.floor(height / BLOCK_ROWS));
  for (let row = 0; row < BLOCK_ROWS; row++) {
    for (let column = 0; column < BLOCK_COLUMNS; column++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let count = 0;
      for (
        let y = row * blockHeight;
        y < Math.min(height, (row + 1) * blockHeight);
        y++
      ) {
        for (
          let x = column * blockWidth;
          x < Math.min(width, (column + 1) * blockWidth);
          x++
        ) {
          const index = (y * width + x) * 4;
          if (data[index + 3] < 16) continue;
          r += data[index];
          g += data[index + 1];
          b += data[index + 2];
          count++;
        }
      }
      if (!count) continue;
      const color = {
        r: Math.round(r / count),
        g: Math.round(g / count),
        b: Math.round(b / count),
      };
      blocks.push({ color, luminance: relativeLuminance(color) });
    }
  }
  return blocks;
}

// 按色相分桶取占比最高的一族, 直接平均互补色会互相抵消成灰。
export function sampleScene(
  data: Uint8ClampedArray,
  width: number,
  height: number,
): SceneSample | null {
  const total = width * height;
  if (
    !Number.isFinite(total) ||
    total <= 0 ||
    !Number.isFinite(width) ||
    !Number.isFinite(height) ||
    data.length < total * 4
  )
    return null;
  const sumA = new Float64Array(HUE_BINS);
  const sumB = new Float64Array(HUE_BINS);
  const sumChroma = new Float64Array(HUE_BINS);
  const weights = new Float64Array(HUE_BINS);
  let red = 0;
  let green = 0;
  let blue = 0;
  let count = 0;
  for (let index = 0; index < total * 4; index += 4) {
    if (data[index + 3] < 16) continue;
    const r = data[index] / 255;
    const g = data[index + 1] / 255;
    const b = data[index + 2] / 255;
    red += data[index];
    green += data[index + 1];
    blue += data[index + 2];
    count++;
    const lab = srgbToOklab(r, g, b);
    const chroma = Math.hypot(lab.a, lab.b);
    if (chroma < 0.015) continue;
    const bin = Math.min(
      HUE_BINS - 1,
      Math.floor(
        ((Math.atan2(lab.b, lab.a) + Math.PI) / (2 * Math.PI)) * HUE_BINS,
      ),
    );
    // 暗部的高光与阴影容易带出压缩噪点, 权重随明度降低。
    const weight = chroma * (0.4 + lab.l);
    sumA[bin] += lab.a * weight;
    sumB[bin] += lab.b * weight;
    sumChroma[bin] += chroma * weight;
    weights[bin] += weight;
  }
  if (!count) return null;
  const blocks = blockAverages(data, width, height);
  const average = {
    r: Math.round(red / count),
    g: Math.round(green / count),
    b: Math.round(blue / count),
  };
  let best = -1;
  let bestScore = 0;
  for (let bin = 0; bin < HUE_BINS; bin++) {
    const score =
      weights[bin] +
      (weights[(bin + HUE_BINS - 1) % HUE_BINS] +
        weights[(bin + 1) % HUE_BINS]) /
        2;
    if (score > bestScore) {
      bestScore = score;
      best = bin;
    }
  }
  const darkest = blocks.reduce<Block | null>(
    (found, block) =>
      !found || block.luminance < found.luminance ? block : found,
    null,
  );
  const brightest = blocks.reduce<Block | null>(
    (found, block) =>
      !found || block.luminance > found.luminance ? block : found,
    null,
  );
  if (best < 0 || !(weights[best] > 0)) {
    return {
      a: 0,
      b: 0,
      chroma: 0,
      weight: 0,
      average,
      dark: darkest?.color ?? average,
      bright: brightest?.color ?? average,
    };
  }
  return {
    a: sumA[best] / weights[best],
    b: sumB[best] / weights[best],
    chroma: sumChroma[best] / weights[best],
    weight: weights[best],
    average,
    dark: darkest?.color ?? average,
    bright: brightest?.color ?? average,
  };
}

export function createAdaptiveTheme(
  sample: SceneSample | null,
): AdaptiveTheme | null {
  if (!sample) return null;
  const magnitude = Math.hypot(sample.a, sample.b);
  if (!Number.isFinite(magnitude)) return null;
  const hue =
    magnitude > 1e-6
      ? { a: sample.a / magnitude, b: sample.b / magnitude }
      : { a: 0, b: 0 };
  const accentChroma = clamp(
    Number.isFinite(sample.chroma) ? sample.chroma * 0.75 : 0,
    0,
    MAX_ACCENT_C,
  );
  const tintLight = oklchToRgb(
    LIGHT_TINT_L,
    Math.min(MAX_TINT_C, accentChroma * 0.4),
    hue.a,
    hue.b,
  );
  const tintDark = oklchToRgb(
    DARK_TINT_L,
    Math.min(MAX_DARK_TINT_C, accentChroma * 0.5),
    hue.a,
    hue.b,
  );
  const background = {
    light: overlay(
      tintLight,
      EFFECTIVE_FILL,
      scaleSrgb(sample.dark, SCENE_SHADE * 0.95),
    ),
    // 暗色正文按场景遮罩后的亮部估算, 与二级界面的实际背景一致。
    dark: overlay(
      tintDark,
      EFFECTIVE_DARK,
      brightenSrgb(scaleSrgb(sample.bright, SCENE_SHADE), 0.04),
    ),
  };
  const textChroma = Math.min(0.018, accentChroma * 0.2);
  const lightText = (target: number, chroma: number) =>
    pickText(background.light, target, chroma, hue.a, hue.b, "light");
  const darkText = (target: number, chroma: number, lift: number) =>
    pickText(background.dark, target, chroma, hue.a, hue.b, "dark", lift);
  const soft = oklchToRgb(
    0.93,
    Math.min(MAX_SOFT_C, accentChroma * 0.3),
    hue.a,
    hue.b,
  );
  const primaryFg = pickText(soft, 8, 0.008, hue.a, hue.b, "light");
  const primaryHover = oklchToRgb(
    0.955,
    Math.min(MAX_SOFT_C, accentChroma * 0.3),
    hue.a,
    hue.b,
  );
  return {
    tintLight,
    tintDark,
    glowLight: oklchToRgb(
      LIGHT_GLOW_L,
      Math.min(0.12, accentChroma * 1.4),
      hue.a,
      hue.b,
    ),
    glowDark: oklchToRgb(
      DARK_GLOW_L,
      Math.min(0.12, accentChroma * 1.3),
      hue.a,
      hue.b,
    ),
    heroAccent: oklchToRgb(
      HERO_ACCENT_L,
      Math.min(0.09, accentChroma),
      hue.a,
      hue.b,
    ),
    background,
    light: {
      ink: lightText(INK_TARGET, textChroma * 0.35),
      soft: lightText(SOFT_TARGET, textChroma * 0.55),
      muted: lightText(MUTED_TARGET, textChroma * 0.8),
      faint: lightText(FAINT_TARGET, textChroma),
      accent: pickText(
        background.light,
        ACCENT_TARGET,
        accentChroma,
        hue.a,
        hue.b,
        "light",
      ),
      accentFill: pickFill(
        background.light,
        LIGHT_FG,
        accentChroma,
        hue.a,
        hue.b,
        "light",
      ),
      accentFg: LIGHT_FG,
      accentSoft: soft,
      primaryFg,
      primaryHover,
    },
    dark: {
      ink: darkText(INK_TARGET, textChroma * 0.35, 0.16),
      soft: darkText(SOFT_TARGET, textChroma * 0.55, 0.09),
      muted: darkText(MUTED_TARGET, textChroma * 0.8, 0.04),
      faint: darkText(FAINT_TARGET, textChroma, 0),
      accent: pickText(
        background.dark,
        ACCENT_TARGET,
        accentChroma,
        hue.a,
        hue.b,
        "dark",
        0.08,
      ),
      accentFill: pickFill(
        background.dark,
        DARK_FG,
        accentChroma,
        hue.a,
        hue.b,
        "dark",
      ),
      accentFg: DARK_FG,
      accentSoft: soft,
      primaryFg,
      primaryHover,
    },
  };
}

export function formatRgb({ r, g, b }: Rgb): string {
  return `rgb(${r} ${g} ${b})`;
}

export function formatRgba({ r, g, b }: Rgb, alpha: number): string {
  return `rgba(${r}, ${g}, ${b}, ${Number(alpha.toFixed(3))})`;
}

// 计算 background-position 这类 cover 裁切的源矩形。
export function coverCrop(
  sourceWidth: number,
  sourceHeight: number,
  targetWidth: number,
  targetHeight: number,
  focusX = 0.5,
  focusY = 0.5,
): Crop | null {
  if (
    !(sourceWidth > 0) ||
    !(sourceHeight > 0) ||
    !(targetWidth > 0) ||
    !(targetHeight > 0)
  )
    return null;
  const scale = Math.max(
    targetWidth / sourceWidth,
    targetHeight / sourceHeight,
  );
  const width = Math.min(sourceWidth, targetWidth / scale);
  const height = Math.min(sourceHeight, targetHeight / scale);
  return {
    x: (sourceWidth - width) * clamp(focusX, 0, 1),
    y: (sourceHeight - height) * clamp(focusY, 0, 1),
    width,
    height,
  };
}
