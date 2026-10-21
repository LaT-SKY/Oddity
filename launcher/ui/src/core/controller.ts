import { computed, nextTick, reactive } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  Bootstrap,
  CheckResult,
  LaunchResult,
  GameStatus,
} from "../types/launcher";
import { invoke, isTauri, listenGameState } from "./api";
import { APP_VERSION } from "./version";

let generation = 0;
let events = new window.AbortController();
const disposers: (() => void)[] = [];
const timers = new Set<number>();
function schedule(callback: () => void, ms: number) {
  const id = window.setTimeout(() => {
    timers.delete(id);
    callback();
  }, ms);
  timers.add(id);
  return id;
}
function onDocument<K extends keyof DocumentEventMap>(
  type: K,
  handler: (event: DocumentEventMap[K]) => void,
  options: AddEventListenerOptions = {},
) {
  document.addEventListener(type, handler, {
    ...options,
    signal: events.signal,
  });
}
function onWindow<K extends keyof WindowEventMap>(
  type: K,
  handler: (event: WindowEventMap[K]) => void,
) {
  window.addEventListener(type, handler, { signal: events.signal });
}

export const form = reactive({
  installRoot: "",
  bridgeRoot: "",
  prefix: "",
  protonPath: "",
  launchMode: "background",
  theme: "light",
  heroMedia: "video",
  reduceMotion: false,
});

type GamePhase = "idle" | "launching" | "running" | "closing";
export const state = reactive<{
  bootstrap: Bootstrap | null;
  check: CheckResult | null;
  page: string;
  checking: boolean;
  game: GamePhase;
  session: GameStatus | LaunchResult | null;
  heroMedia: string;
}>({
  bootstrap: null,
  check: null,
  page: "home",
  checking: false,
  game: "idle",
  session: null,
  heroMedia: "video",
});

export const ui = reactive({
  setupNeeded: false,
  updateError: false,
  saving: false,
  redetecting: false,
  maximized: false,
  introStarted: false,
  introDone: false,
  toast: { message: "", kind: "ok", visible: false },
  scrollEdge: { top: false, bottom: false },
});
export const media = reactive({ url: "", looping: false });
let toastTimer = 0;
let videoElement: HTMLVideoElement | null = null;
export function setHeroVideo(video: HTMLVideoElement | null) {
  videoElement = video;
}
export const displayVersion = computed(
  () => state.bootstrap?.appVersion || APP_VERSION,
);
export const launchView = computed(() => ({
  ...LAUNCH_COPY[state.game],
  busy: state.game === "launching" || state.game === "closing",
}));
export const updateView = computed(() => {
  const result = state.check;
  const rows = [
    ...(result?.missingPreview ?? []).map((path) => ({ tag: "缺失", path })),
    ...(result?.changedPreview ?? []).map((path) => ({ tag: "变更", path })),
  ];
  const pending = (result?.missing || 0) + (result?.changed || 0);
  const knownOnly =
    pending === 1 &&
    rows.length === 1 &&
    rows[0].tag === "变更" &&
    rows[0].path.includes("QtQuick/Controls/Styles/Flat/plugins.qmltypes");
  const effectivePending = knownOnly ? 0 : pending;
  const version = result?.localVersion || "未知";
  const files = result?.files || 0;
  let status = {
    kind: "idle",
    title: "尚未检查",
    copy: state.bootstrap?.launcherVersion
      ? "本地版本 " + state.bootstrap.launcherVersion
      : "等待检查",
    icon: "i-arrows-clockwise",
  };
  if (state.checking)
    status = {
      kind: "checking",
      title: "正在检查",
      copy: "读取官方文件列表…",
      icon: "i-arrows-clockwise",
    };
  else if (ui.updateError)
    status = {
      kind: "error",
      title: "检查失败",
      copy: "无法连接官方 CDN 或安装目录无效",
      icon: "i-warning-circle",
    };
  else if (result)
    status =
      effectivePending === 0
        ? {
            kind: "ok",
            title: "已是最新",
            copy:
              version +
              " · " +
              files +
              (knownOnly ? " 个文件（1 个已知例外）" : " 个文件与官方一致"),
            icon: "i-check-circle",
          }
        : {
            kind: "warn",
            title: pending + " 个文件待更新",
            copy: version + " · " + files + " 个文件",
            icon: "i-warning-circle",
          };
  return {
    ...status,
    rows,
    pending: effectivePending,
    remaining: pending - rows.length,
    showFiles: !state.checking && !ui.updateError && effectivePending > 0,
    showHomeStatus: !state.checking && (ui.updateError || effectivePending > 0),
    homeStatusKind: ui.updateError
      ? "error"
      : effectivePending === 0
        ? "ok"
        : "warn",
    homeStatusText: ui.updateError
      ? "检查失败，打开更新页"
      : effectivePending + " 个文件待更新",
  };
});

function isReady(data: Bootstrap | null) {
  return Boolean(
    data && data.ok && data.protonName && data.umuAvailable && data.gameExists,
  );
}

const THEMES = new Set(["light", "dark"]);

// 保留短暂启动反馈, 避免窗口立即收起而看不到状态变化。
const LAUNCH_FEEDBACK_MS = 900;
const LAUNCH_COPY = {
  idle: { label: "启动游戏", icon: "i-play" },
  launching: { label: "正在启动", icon: "i-play" },
  running: { label: "关闭游戏", icon: "i-power" },
  closing: { label: "正在关闭", icon: "i-power" },
};

const pendingWaits = new Set<() => void>();
const wait = (ms: number) =>
  new Promise<void>((resolve) => {
    pendingWaits.add(resolve);
    schedule(() => {
      pendingWaits.delete(resolve);
      resolve();
    }, ms);
  });

export function applyTheme(theme: string) {
  const value = THEMES.has(theme) ? theme : "light";
  try {
    localStorage.setItem("nte.theme", value);
  } catch {}
  form.theme = value;
  return value;
}

const HERO_MEDIA = new Set(["video", "photo"]);
const REDUCE_MOTION_KEY = "nte.reduceMotion";
const INTRO_DURATION = 380;
let heroVideoRequest: Promise<boolean> | null = null;
let pendingHeroMedia: string | null = null;
const introWaiters: (() => void)[] = [];

function motionReduced() {
  return (
    form.reduceMotion ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

export function applyReduceMotion(value: boolean) {
  const enabled = Boolean(value);
  try {
    localStorage.setItem(REDUCE_MOTION_KEY, enabled ? "1" : "0");
  } catch {}
  form.reduceMotion = enabled;
  return enabled;
}

function whenIntroDone(callback: () => void) {
  if (ui.introDone) callback();
  else introWaiters.push(callback);
}

function finishIntro() {
  if (ui.introDone) return;
  ui.introDone = true;
  const pending = pendingHeroMedia;
  pendingHeroMedia = null;
  if (pending) applyHeroMedia(pending);
  introWaiters.splice(0).forEach((callback) => callback());
}

function startIntro() {
  if (ui.introStarted) return;
  ui.introStarted = true;
  schedule(finishIntro, motionReduced() ? 0 : INTRO_DURATION);
}

async function loadHeroVideo(video: HTMLVideoElement) {
  const epoch = generation;
  if (media.url) return true;
  if (heroVideoRequest) return heroVideoRequest;

  const candidates = [
    { url: "assets/hero-loop.mp4", type: 'video/mp4; codecs="avc1.42E01E"' },
    { url: "assets/hero-loop.webm", type: 'video/webm; codecs="vp9"' },
  ];
  const chosen = candidates.find((candidate) =>
    video.canPlayType(candidate.type),
  );
  if (!chosen) return false;

  heroVideoRequest = (async () => {
    const response = await fetch(chosen.url, { cache: "no-store" });
    if (!response.ok) return false;
    const blob = await response.blob();
    if (epoch !== generation) return false;
    media.url = URL.createObjectURL(blob);
    await nextTick();
    if (epoch !== generation) return false;
    video.load();
    return true;
  })().catch(() => false);

  const result = await heroVideoRequest;
  if (epoch === generation) heroVideoRequest = null;
  return result;
}

function canPlayHeroVideo() {
  if (state.heroMedia !== "video" || state.page !== "home" || document.hidden)
    return false;
  return !motionReduced();
}

function requestHeroPlayback(video: HTMLVideoElement) {
  if (isTauri()) {
    void invoke("play_hero_video").catch(() => {});
    return;
  }
  const attempt = video.play();
  if (attempt && typeof attempt.catch === "function") attempt.catch(() => {});
}

export function heroEnded() {
  if (!media.looping || !canPlayHeroVideo() || !videoElement) return;
  videoElement.currentTime = 0;
  requestHeroPlayback(videoElement);
}

function startHeroVideo(video: HTMLVideoElement) {
  media.looping = true;
  requestHeroPlayback(video);
}

function stopHeroVideo(video: HTMLVideoElement | null) {
  media.looping = false;
  if (!video) return;
  video.pause();
  try {
    video.currentTime = 0;
  } catch {}
}

export function applyHeroMedia(input: string) {
  const value = HERO_MEDIA.has(input) ? input : "video";
  state.heroMedia = value;
  form.heroMedia = value;

  // 入场完成后再解码视频, 避免影响 WebKitGTK 的合成帧。
  if (!ui.introDone) {
    pendingHeroMedia = value;
    return value;
  }

  const video = videoElement;
  if (!video) return value;

  if (!canPlayHeroVideo()) {
    stopHeroVideo(video);
    return value;
  }

  const epoch = generation;
  loadHeroVideo(video).then((loaded) => {
    if (epoch !== generation) return;
    if (!loaded) {
      stopHeroVideo(video);
      return;
    }
    if (!canPlayHeroVideo()) {
      stopHeroVideo(video);
      return;
    }
    if (!media.looping) {
      startHeroVideo(video);
    } else {
      requestHeroPlayback(video);
    }
  });
  return value;
}

function toast(message: string, kind = "ok", duration = 3400) {
  window.clearTimeout(toastTimer);
  Object.assign(ui.toast, { message, kind, visible: true });
  toastTimer = schedule(() => {
    ui.toast.visible = false;
  }, duration);
}

function applyBootstrap(data: Bootstrap) {
  state.bootstrap = data;
  const settings = data.settings || {};

  const ready = isReady(data);
  ui.setupNeeded = !ready;
  applyTheme(settings.theme || "light");

  form.installRoot = settings.installRoot || data.installRoot || "";
  form.bridgeRoot = settings.bridgeRoot || data.bridgeRoot || "";
  form.prefix = settings.prefix || data.prefix || "";
  form.protonPath = settings.protonPath || data.protonPath || "";
  form.launchMode = settings.launchMode || "background";

  applyReduceMotion(Boolean(settings.reduceMotion));
  applyHeroMedia(settings.heroMedia || "video");

  if (data.error) {
    toast(data.error, "error", 6000);
  }
  void nextTick(syncScrollEdges);
}

export async function checkUpdate(full = false, silent = false) {
  const epoch = generation;
  if (state.checking) return;
  state.checking = true;
  ui.updateError = false;
  try {
    const result = await invoke("check_update", { full });
    if (epoch !== generation) return;
    state.check = result;
  } catch (error) {
    if (epoch !== generation) return;
    const message = error instanceof Error ? error.message : String(error);
    ui.updateError = true;
    if (!silent) toast(message, "error", 6500);
  } finally {
    if (epoch === generation) state.checking = false;
  }
}

function setLaunchState(next: GamePhase) {
  state.game = next;
}

function applyGameStatus(
  status: GameStatus | null,
  { force = false, announce = false } = {},
) {
  if (!status || typeof status.running !== "boolean") return;
  // 启动和关闭期间由命令维护按钮状态, 避免事件覆盖中间反馈。
  const previous = state.game;
  if (!force && (previous === "launching" || previous === "closing")) return;
  if (status.running) {
    state.session = status;
    setLaunchState(status.closing ? "closing" : "running");
    return;
  }
  state.session = null;
  setLaunchState("idle");
  if (announce && previous === "running") toast("游戏已退出。");
}

async function syncGameStatus() {
  const epoch = generation;
  if (!isTauri()) return;
  try {
    const status = await invoke("game_status");
    if (epoch === generation) applyGameStatus(status);
  } catch {}
}

async function launchGame() {
  const epoch = generation;
  if (state.game !== "idle") return;
  if (!isTauri()) toast("浏览器预览：模拟启动流程。", "ok", 2600);
  setLaunchState("launching");
  const startedAt = performance.now();
  try {
    const result = await invoke("launch_official");
    if (epoch !== generation) return;
    state.session = result;
    const remaining = LAUNCH_FEEDBACK_MS - (performance.now() - startedAt);
    if (remaining > 0) await wait(remaining);
    if (epoch !== generation) return;

    // 收起窗口前确认游戏仍在运行。
    const status = await invoke("game_status").catch(() => null);
    if (epoch !== generation) return;
    if (status && !status.running) {
      setLaunchState("idle");
      toast("游戏进程已退出，启动未完成。", "error", 7000);
      return;
    }

    setLaunchState("running");
    if (isTauri()) {
      try {
        await invoke("win_hide");
      } catch {}
    }
  } catch (error) {
    if (epoch !== generation) return;
    let status = null;
    try {
      status = await invoke("game_status");
    } catch {}
    if (epoch !== generation) return;
    if (status && status.running) applyGameStatus(status, { force: true });
    else setLaunchState("idle");
    toast(
      error instanceof Error ? error.message : String(error),
      "error",
      7500,
    );
  }
}

async function closeGame() {
  const epoch = generation;
  if (state.game !== "running") return;
  setLaunchState("closing");
  try {
    const status = await invoke("close_game");
    if (epoch !== generation) return;
    applyGameStatus(status, { force: true });
    toast("游戏已关闭。");
  } catch (error) {
    if (epoch !== generation) return;
    let status = null;
    try {
      status = await invoke("game_status");
    } catch {}
    if (epoch !== generation) return;
    if (status) applyGameStatus(status, { force: true });
    else setLaunchState("running");
    toast(
      error instanceof Error ? error.message : String(error),
      "error",
      7500,
    );
  }
}

export function handleLaunchAction() {
  if (state.game === "running") closeGame();
  else launchGame();
}

export async function saveSettings() {
  const epoch = generation;
  if (ui.saving) return;
  const settings = {
    installRoot: form.installRoot.trim() || null,
    bridgeRoot: form.bridgeRoot.trim() || null,
    prefix: form.prefix.trim() || null,
    protonPath: form.protonPath.trim() || null,
    launchMode: form.launchMode || "background",
    language: "zh-CN",
    reduceMotion: form.reduceMotion,
    theme: applyTheme(form.theme),
    heroMedia: applyHeroMedia(form.heroMedia),
  };
  ui.saving = true;
  try {
    const data = await invoke("save_settings", { settings });
    if (epoch !== generation) return;
    applyBootstrap(data);
    applyReduceMotion(settings.reduceMotion);
    toast("设置已保存。");
  } catch (error) {
    if (epoch !== generation) return;
    toast(
      error instanceof Error ? error.message : String(error),
      "error",
      6000,
    );
  } finally {
    if (epoch === generation) ui.saving = false;
  }
}

export async function redetect() {
  const epoch = generation;
  if (ui.redetecting) return;
  ui.redetecting = true;
  try {
    const data = await invoke("bootstrap");
    if (epoch !== generation) return;
    applyBootstrap(data);
    toast("已重新检测环境。");
  } catch (error) {
    if (epoch !== generation) return;
    toast(
      error instanceof Error ? error.message : String(error),
      "error",
      6000,
    );
  } finally {
    if (epoch === generation) ui.redetecting = false;
  }
}

export async function openUrl(url: string) {
  const epoch = generation;
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    toast("预览模式：已在新标签页打开。");
    return;
  }
  try {
    await invoke("open_url", { url });
    if (epoch !== generation) return;
    toast("已在浏览器打开。");
  } catch (error) {
    if (epoch !== generation) return;
    toast(
      error instanceof Error ? error.message : String(error),
      "error",
      6000,
    );
  }
}

function hashPage() {
  const value = window.location.hash.replace(/^#/, "");
  return ["home", "update", "bulletin", "settings"].includes(value)
    ? value
    : "home";
}

export function syncScrollEdges() {
  const activePage = document.querySelector<HTMLElement>(".page.is-active");
  if (!activePage || activePage.classList.contains("page--home")) {
    ui.scrollEdge.top = false;
    ui.scrollEdge.bottom = false;
    return;
  }
  const { scrollTop, scrollHeight, clientHeight } = activePage;
  const maxScroll = scrollHeight - clientHeight;
  if (maxScroll <= 2) {
    ui.scrollEdge.top = false;
    ui.scrollEdge.bottom = false;
    return;
  }
  ui.scrollEdge.top = scrollTop > 4;
  ui.scrollEdge.bottom = scrollTop < maxScroll - 4;
}

export function showPage(name: string | undefined, syncHash = true) {
  if (!name || !["home", "update", "bulletin", "settings"].includes(name))
    return;
  const changed = state.page !== name;
  state.page = name;
  // 封面与播放起点一致, 返回主页时不恢复到缩放动画的中途。
  if (changed) applyHeroMedia(state.heroMedia);
  void nextTick(syncScrollEdges);
  if (syncHash && window.location.hash !== "#" + name) {
    try {
      history.replaceState(null, "", "#" + name);
    } catch {}
  }
}

export async function windowAction(action: "minimize" | "maximize" | "close") {
  const epoch = generation;
  if (!isTauri()) return;
  const command =
    action === "minimize"
      ? "win_minimize"
      : action === "maximize"
        ? "win_toggle_maximize"
        : "win_close";
  try {
    await invoke(command);
  } catch (error) {
    if (epoch !== generation) return;
    toast(error instanceof Error ? error.message : String(error), "error");
  }
}

export function dragWindow(event: MouseEvent) {
  if (event.button !== 0) return;
  if (
    event.target instanceof Element &&
    event.target.closest("button, input, select, a")
  )
    return;
  if (isTauri()) void invoke("win_drag").catch(() => {});
}

function bindInteractions() {
  onDocument("visibilitychange", () => applyHeroMedia(state.heroMedia));
  onDocument(
    "scroll",
    (event) => {
      if (
        event.target instanceof Element &&
        event.target.classList.contains("page")
      ) {
        syncScrollEdges();
      }
    },
    { capture: true, passive: true },
  );
  onWindow("resize", syncScrollEdges);
  const blockDrag = (event: Event) => event.preventDefault();
  onDocument("dragstart", blockDrag);
  onDocument("dragover", blockDrag);
  onDocument("drop", blockDrag);
  onDocument("selectstart", (event) => {
    if (
      event.target instanceof Element &&
      event.target.closest("input, textarea, [contenteditable='true']")
    )
      return;
    event.preventDefault();
  });
  onDocument("contextmenu", (event) => {
    if (
      event.target instanceof Element &&
      event.target.closest("input, textarea, select, [contenteditable='true']")
    )
      return;
    event.preventDefault();
  });

  onDocument("keydown", (event) => {
    if (event.key === "Tab") event.preventDefault();
  });
  onDocument(
    "pointerdown",
    () => {
      if (!canPlayHeroVideo()) return;
      const video = videoElement;
      if (video && video.src) {
        requestHeroPlayback(video);
      }
    },
    { once: true },
  );

  onDocument("keydown", (event) => {
    if (!event.ctrlKey || event.altKey || event.metaKey) return;
    const index = ["1", "2", "3", "4"].indexOf(event.key);
    if (index >= 0) {
      event.preventDefault();
      showPage(["home", "update", "bulletin", "settings"][index]);
    }
  });

  onDocument("keydown", (event) => {
    if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "s")
      return;
    if (state.page !== "settings") return;
    event.preventDefault();
    void saveSettings();
  });

  onWindow("hashchange", () => showPage(hashPage(), false));

  onWindow("focus", () => {
    if (state.game === "launching" || state.game === "closing") return;
    syncGameStatus();
  });
}

async function watchWindowShape() {
  const epoch = generation;
  if (!isTauri()) return;
  try {
    const win = getCurrentWindow();
    const sync = async () => {
      const maximized = await win.isMaximized();
      if (epoch === generation) ui.maximized = maximized;
    };
    await sync();
    const stop = await win.onResized(sync);
    if (epoch !== generation) stop();
    else disposers.push(stop);
  } catch {}
}

export async function init() {
  const epoch = ++generation;
  events = new window.AbortController();
  let storedReduceMotion = false;
  try {
    applyTheme(localStorage.getItem("nte.theme") || "light");
    storedReduceMotion =
      storedReduceMotion || localStorage.getItem(REDUCE_MOTION_KEY) === "1";
  } catch {
    applyTheme("light");
  }
  applyReduceMotion(storedReduceMotion);
  bindInteractions();
  showPage(hashPage(), false);
  setLaunchState("idle");
  disposers.push(
    listenGameState((status) => applyGameStatus(status, { announce: true })),
  );
  watchWindowShape();
  await syncGameStatus();
  if (epoch !== generation) return;

  try {
    const data = await invoke("bootstrap");
    if (epoch !== generation) return;
    applyBootstrap(data);
  } catch (error) {
    if (epoch !== generation) return;
    ui.setupNeeded = true;
    toast(
      error instanceof Error ? error.message : String(error),
      "error",
      6500,
    );
  }

  startIntro();

  if (state.bootstrap && state.bootstrap.ok) {
    whenIntroDone(() => {
      schedule(() => checkUpdate(false, true), 400);
    });
  }
}

export function dispose() {
  generation++;
  events.abort();
  disposers.splice(0).forEach((stop) => stop());
  timers.forEach((id) => window.clearTimeout(id));
  timers.clear();
  pendingWaits.forEach((resolve) => resolve());
  pendingWaits.clear();
  stopHeroVideo(videoElement);
  videoElement = null;
  if (media.url) URL.revokeObjectURL(media.url);
  media.url = "";
  heroVideoRequest = null;
  pendingHeroMedia = null;
  introWaiters.length = 0;
  ui.introStarted = false;
  ui.introDone = false;
  ui.toast.visible = false;
  Object.assign(state, {
    bootstrap: null,
    check: null,
    page: "home",
    checking: false,
    game: "idle",
    session: null,
  });
  Object.assign(ui, {
    saving: false,
    redetecting: false,
    updateError: false,
    setupNeeded: false,
    maximized: false,
    scrollEdge: { top: false, bottom: false },
  });
}
