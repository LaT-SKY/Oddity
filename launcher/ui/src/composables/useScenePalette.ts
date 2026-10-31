import { nextTick, onBeforeUnmount, onMounted, watch } from "vue";
import { form, media, state, ui } from "../core/controller";
import {
  createAdaptiveTheme,
  coverCrop,
  formatRgb,
  formatRgba,
  sampleScene,
  type AdaptiveTheme,
  type SceneSample,
  type ThemeRoles,
} from "../core/palette";

const SAMPLE_WIDTH = 40;
const SAMPLE_HEIGHT = 24;
const RETRY_DELAYS = [0, 320, 800, 1600, 3200];
const BUNDLED = {
  photo: "assets/hero-keyart.jpg",
  video: "assets/hero-poster.jpg",
};
const BUNDLED_FOCUS = { x: 0.54, y: 0.38 };
const HEAT_ALPHAS = [0.1, 0.22, 0.4, 0.58, 0.78, 1];

interface Wallpaper {
  url: string;
  focus: { x: number; y: number };
}

function roleVariables(theme: AdaptiveTheme, mode: "light" | "dark") {
  const roles: ThemeRoles = mode === "dark" ? theme.dark : theme.light;
  const fill = roles.accentFill;
  const variables: Record<string, string> = {
    "--bone": formatRgb(roles.ink),
    "--text-soft": formatRgb(roles.soft),
    "--muted": formatRgb(roles.muted),
    "--faint": formatRgb(roles.faint),
    "--accent-text": formatRgb(roles.accent),
    "--signal": formatRgb(roles.accentSoft),
    "--signal-strong": formatRgb(fill),
    "--signal-ink": formatRgb(roles.primaryFg),
    "--signal-dim": formatRgba(fill, 0.12),
    "--nav-active-color": formatRgb(roles.accent),
    "--nav-active-bg": formatRgba(fill, mode === "dark" ? 0.1 : 0.16),
    "--nav-active-bar": formatRgb(fill),
    "--dot-signal": formatRgb(fill),
    "--primary-bg": formatRgb(roles.accentSoft),
    "--primary-fg": formatRgb(roles.primaryFg),
    "--primary-hover": formatRgb(roles.primaryHover),
    "--primary-icon-bg": formatRgba(roles.primaryFg, 0.14),
    "--primary-border": formatRgba(fill, mode === "dark" ? 0.22 : 0.3),
    "--focus": formatRgb(roles.accent),
    "--switch-on-bg": formatRgb(fill),
    "--switch-on-knob": formatRgb(roles.accentFg),
    "--selection-bg": formatRgb(fill),
    "--selection-fg": formatRgb(roles.accentFg),
    "--input-focus-border": formatRgba(fill, 0.52),
  };
  HEAT_ALPHAS.forEach((alpha, index) => {
    variables[`--heat-${index}`] =
      alpha === 1 ? formatRgb(fill) : formatRgba(fill, alpha);
  });
  return variables;
}

export function useScenePalette() {
  const root = document.documentElement;
  const styled = new Map<HTMLElement, Set<string>>();
  const tintNames = new Set<string>();
  let theme: AdaptiveTheme | null = null;
  let image: HTMLImageElement | null = null;
  let imageUrl = "";
  let imageReady = false;
  let retryIndex = 0;
  let retryTimer = 0;
  let disposed = false;
  let frame: HTMLCanvasElement | null = null;
  let context: CanvasRenderingContext2D | null = null;
  let observer: MutationObserver | undefined;

  function setTint(name: string, value: string) {
    root.style.setProperty(name, value);
    tintNames.add(name);
  }

  function setRoles(element: HTMLElement, mode: "light" | "dark") {
    if (!theme) return;
    const variables = roleVariables(theme, mode);
    const names = styled.get(element) ?? new Set<string>();
    for (const [name, value] of Object.entries(variables)) {
      element.style.setProperty(name, value);
      names.add(name);
    }
    styled.set(element, names);
  }

  function clear() {
    for (const name of tintNames) root.style.removeProperty(name);
    tintNames.clear();
    for (const [element, names] of styled) {
      for (const name of names) element.style.removeProperty(name);
    }
    styled.clear();
    theme = null;
    observer?.disconnect();
    observer = undefined;
  }

  function activeMode(): "light" | "dark" {
    return form.theme === "dark" ? "dark" : "light";
  }

  function elementMode(element: HTMLElement): "light" | "dark" {
    if (element.dataset.theme === "dark") return "dark";
    if (element.dataset.theme === "light") return "light";
    return activeMode();
  }

  function styleElement(element: HTMLElement) {
    setRoles(element, elementMode(element));
  }

  function writeRoles() {
    if (!theme) return;
    for (const element of styled.keys())
      if (!element.isConnected) styled.delete(element);
    setRoles(root, activeMode());
    setRoles(document.body, activeMode());
    document
      .querySelectorAll<HTMLElement>("[data-theme]")
      .forEach((element) => styleElement(element));
  }

  function observe() {
    if (observer || disposed) return;
    // 弹窗与浮层按需挂载在 body 下, 新节点出现时补上同一套主题色。
    observer = new MutationObserver((mutations) => {
      if (!theme) return;
      for (const element of styled.keys())
        if (!element.isConnected) styled.delete(element);
      for (const mutation of mutations) {
        if (
          mutation.type === "attributes" &&
          mutation.target instanceof HTMLElement
        ) {
          styleElement(mutation.target);
          continue;
        }
        mutation.addedNodes.forEach((node) => {
          if (!(node instanceof Element)) return;
          if (node instanceof HTMLElement && node.hasAttribute("data-theme"))
            styleElement(node);
          node
            .querySelectorAll<HTMLElement>("[data-theme]")
            .forEach(styleElement);
        });
      }
    });
    observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["data-theme"],
    });
  }

  function wallpaper(): Wallpaper {
    // 官方素材优先; 视频模式取内置海报, 避免跟随每一帧画面变色。
    if (media.image) return { url: media.image, focus: { x: 0.5, y: 0.5 } };
    return {
      url: new URL(
        state.heroMedia === "photo" ? BUNDLED.photo : BUNDLED.video,
        document.baseURI,
      ).href,
      focus: BUNDLED_FOCUS,
    };
  }

  function apply(next: AdaptiveTheme) {
    theme = next;
    setTint("--scene-tint-light", formatRgb(next.tintLight));
    setTint("--scene-tint-dark", formatRgb(next.tintDark));
    setTint("--scene-accent-light", formatRgb(next.glowLight));
    setTint("--scene-accent-dark", formatRgb(next.glowDark));
    setTint("--hero-accent", formatRgb(next.heroAccent));
    writeRoles();
    observe();
  }

  function ensureFrame() {
    if (frame) return context !== null;
    frame = document.createElement("canvas");
    frame.width = SAMPLE_WIDTH;
    frame.height = SAMPLE_HEIGHT;
    const target = frame.getContext("2d", { willReadFrequently: true });
    // 测试环境只提供部分画布接口, 取色在这些环境里直接跳过。
    context =
      target &&
      typeof target.clearRect === "function" &&
      typeof target.drawImage === "function" &&
      typeof target.getImageData === "function"
        ? target
        : null;
    return context !== null;
  }

  function readSample(draw: (ctx: CanvasRenderingContext2D) => void) {
    if (!ensureFrame()) return null;
    const ctx = context!;
    ctx.clearRect(0, 0, SAMPLE_WIDTH, SAMPLE_HEIGHT);
    try {
      draw(ctx);
      const { data } = ctx.getImageData(0, 0, SAMPLE_WIDTH, SAMPLE_HEIGHT);
      return sampleScene(data, SAMPLE_WIDTH, SAMPLE_HEIGHT);
    } catch {
      // 跨域视频会污染画布, 此时改用静态主视觉。
      return null;
    }
  }

  function sampleImage(target: Wallpaper): SceneSample | null {
    if (!imageReady || !image) return null;
    const crop = coverCrop(
      image.naturalWidth,
      image.naturalHeight,
      window.innerWidth,
      window.innerHeight,
      target.focus.x,
      target.focus.y,
    );
    if (!crop) return null;
    return readSample((ctx) => {
      ctx.drawImage(
        image!,
        crop.x,
        crop.y,
        crop.width,
        crop.height,
        0,
        0,
        SAMPLE_WIDTH,
        SAMPLE_HEIGHT,
      );
    });
  }

  // 静态图还没解码时用视频画布兜底。
  function sampleCanvas(): SceneSample | null {
    const source = document.getElementById("hero-canvas");
    if (!(source instanceof HTMLCanvasElement)) return null;
    if (source.width < 2 || source.height < 2) return null;
    return readSample((ctx) => {
      ctx.drawImage(source, 0, 0, SAMPLE_WIDTH, SAMPLE_HEIGHT);
    });
  }

  function loadImage(target: Wallpaper) {
    if (target.url === imageUrl) return;
    imageUrl = target.url;
    imageReady = false;
    image = null;
    const next = new Image();
    next.crossOrigin = "anonymous";
    next.decoding = "async";
    next.addEventListener("load", () => {
      if (disposed || image !== next) return;
      imageReady = true;
      sample();
    });
    next.addEventListener("error", () => {
      if (image !== next) return;
      imageReady = false;
      image = null;
      // 允许后续重试重新发起请求。
      imageUrl = "";
    });
    next.src = target.url;
    image = next;
  }

  function enabled() {
    return form.adaptiveTheme && ui.introDone && !disposed;
  }

  function sample() {
    if (!enabled()) return;
    const target = wallpaper();
    loadImage(target);
    const sampled = sampleImage(target) ?? sampleCanvas();
    if (sampled) {
      retryIndex = 0;
      window.clearTimeout(retryTimer);
      const next = createAdaptiveTheme(sampled);
      if (next) apply(next);
      return;
    }
    scheduleRetry();
  }

  function scheduleRetry() {
    if (retryIndex >= RETRY_DELAYS.length) return;
    const delay = RETRY_DELAYS[retryIndex++];
    window.clearTimeout(retryTimer);
    retryTimer = window.setTimeout(sample, delay);
  }

  function reset() {
    window.clearTimeout(retryTimer);
    retryTimer = 0;
    retryIndex = 0;
    imageUrl = "";
    imageReady = false;
    image = null;
    if (!enabled()) {
      clear();
      return;
    }
    scheduleRetry();
  }

  watch(
    () => ui.introDone,
    (done) => {
      if (done) reset();
    },
    { immediate: true },
  );
  watch(
    () => form.adaptiveTheme,
    (value) => {
      if (!value) {
        window.clearTimeout(retryTimer);
        clear();
        return;
      }
      reset();
    },
  );
  watch(
    [() => media.image, () => media.videoKey, () => media.officialImage],
    () => reset(),
  );
  watch(
    () => [state.heroMedia, form.heroMedia],
    () => reset(),
  );
  watch(
    () => form.theme,
    () => writeRoles(),
  );

  onMounted(() => {
    // 入场完成后主视觉才解码, 这里只补一次取色。
    void nextTick(() => {
      if (ui.introDone) reset();
    });
  });

  onBeforeUnmount(() => {
    disposed = true;
    window.clearTimeout(retryTimer);
    clear();
  });
}
