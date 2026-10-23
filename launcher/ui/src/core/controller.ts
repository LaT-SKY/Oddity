import { computed, nextTick, reactive } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  AttendanceStatus,
  AttendanceTrigger,
  Bootstrap,
  CheckResult,
  LaunchResult,
  GameStatus,
  DownloadStatus,
  BulletinFeed,
} from "../types/launcher";
import { invoke, isTauri, listenGameState, listenDownloadState } from "./api";
import { APP_VERSION } from "./version";

let generation = 0;
let events = new window.AbortController();
const BULLETIN_CACHE_KEY = "nte.bulletins.v1";
const BULLETIN_CACHE_TTL_MS = 15 * 60 * 1000;
const disposers: (() => void)[] = [];
let downloadRevision = 0;
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

export const attendance = reactive({
  status: null as AttendanceStatus | null,
  loaded: false,
  loading: false,
  busy: false,
  running: false,
  error: "",
});
export const attendanceForm = reactive({
  enabled: false,
  trigger: "launch" as Exclude<AttendanceTrigger, "off">,
  time: "08:00",
});
export function syncAttendance(next: AttendanceStatus, syncForm = true) {
  attendance.status = next;
  if (syncForm) {
    attendanceForm.enabled = next.preferences.trigger !== "off";
    attendanceForm.trigger =
      next.preferences.trigger === "off" ? "launch" : next.preferences.trigger;
    attendanceForm.time = next.preferences.time;
  }
}
function attendanceMessage(cause: unknown) {
  return cause && typeof cause === "object" && "message" in cause
    ? String(cause.message)
    : String(cause);
}
async function attendanceAction(
  action: () => Promise<AttendanceStatus>,
  syncForm = true,
) {
  if (attendance.busy || attendance.running)
    throw new Error("签到操作正在进行, 请稍后重试.");
  const epoch = generation;
  attendance.busy = true;
  attendance.error = "";
  try {
    const next = await action();
    if (epoch === generation) syncAttendance(next, syncForm);
  } catch (cause) {
    if (epoch === generation) attendance.error = attendanceMessage(cause);
    throw cause;
  } finally {
    if (epoch === generation) attendance.busy = false;
  }
}
export async function loadAttendance() {
  if (attendance.busy || attendance.loading || attendance.running) return;
  const epoch = generation;
  attendance.loading = true;
  try {
    const next = await invoke("attendance_status");
    // 轮询不能覆盖正在进行的登录或签到结果。
    if (epoch !== generation || attendance.busy || attendance.running) return;
    syncAttendance(next, !attendance.loaded);
    attendance.loaded = true;
    attendance.error = "";
  } catch (cause) {
    if (epoch === generation) attendance.error = attendanceMessage(cause);
  } finally {
    if (epoch === generation) attendance.loading = false;
  }
}
export function loginAttendance(
  phone: string,
  password: string,
  rememberPassword: boolean,
) {
  return attendanceAction(
    () =>
      invoke("attendance_login", {
        phone: phone.trim(),
        password,
        rememberPassword,
      }),
    false,
  );
}
export async function sendAttendanceCode(phone: string): Promise<string> {
  if (attendance.busy || attendance.running)
    throw new Error("签到操作正在进行, 请稍后重试.");
  const epoch = generation;
  attendance.busy = true;
  attendance.error = "";
  try {
    return await invoke("attendance_send_code", { phone: phone.trim() });
  } catch (cause) {
    if (epoch === generation) attendance.error = attendanceMessage(cause);
    throw cause;
  } finally {
    if (epoch === generation) attendance.busy = false;
  }
}
export function loginAttendanceSms(
  phone: string,
  code: string,
  deviceId: string,
): Promise<void> {
  return attendanceAction(
    () =>
      invoke("attendance_login_sms", { phone: phone.trim(), code, deviceId }),
    false,
  );
}
export function logoutAttendance(accountId: string) {
  return attendanceAction(async () => {
    const next = await invoke("attendance_logout", { accountId });
    // 最后一个账号移除后偏好会被后端重置, 同步回开关。
    if (!next.accounts.length) syncAttendance(next, true);
    return next;
  }, false);
}
export async function runAttendance() {
  try {
    await attendanceAction(async () => {
      attendance.running = true;
      try {
        return await invoke("attendance_run");
      } finally {
        attendance.running = false;
      }
    }, false);
  } catch {
    /* 错误由签到区显示。 */
  }
}
export async function saveAttendancePreferences() {
  if (!attendance.status) return;
  const preferences = {
    trigger: attendanceForm.enabled ? attendanceForm.trigger : ("off" as const),
    time: attendanceForm.time,
  };
  const current = attendance.status.preferences;
  if (
    current.trigger === preferences.trigger &&
    current.time === preferences.time
  )
    return;
  await attendanceAction(() => invoke("attendance_configure", { preferences }));
}

export const form = reactive({
  installRoot: "",
  globalInstallRoot: "",
  distribution: "cn" as "cn" | "global",
  bridgeRoot: "",
  prefix: "",
  protonPath: "",
  launchMode: "background",
  theme: "light",
  heroMedia: "video",
  reduceMotion: false,
  minimizeToTrayOnClose: true,
});

type GamePhase = "idle" | "launching" | "running" | "closing";
export const state = reactive<{
  bootstrap: Bootstrap | null;
  check: CheckResult | null;
  download: DownloadStatus | null;
  bulletins: BulletinFeed | null;
  bulletinsLoading: boolean;
  page: string;
  checking: boolean;
  game: GamePhase;
  session: GameStatus | LaunchResult | null;
  heroMedia: string;
}>({
  bootstrap: null,
  check: null,
  download: null,
  bulletins: null,
  bulletinsLoading: false,
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
  ...(state.game === "idle" &&
  (!state.bootstrap?.gameExists || downloadNeedsWork())
    ? {
        label: state.bootstrap?.gameExists ? "更新游戏" : "安装游戏",
        icon: "i-arrow-down",
      }
    : LAUNCH_COPY[state.game]),
  busy:
    state.game === "launching" || state.game === "closing" || downloadBusy(),
}));
function downloadNeedsWork() {
  return (
    state.download?.distribution === form.distribution &&
    ["needs-install", "needs-update", "needs-verify"].includes(
      state.download.phase,
    )
  );
}
export function downloadBusy() {
  return ["checking", "launcher", "game"].includes(state.download?.phase || "");
}
export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

const DOWNLOAD_STAGE_LABELS: Record<string, string> = {
  download: "下载",
  patch: "应用补丁",
  verify: "校验文件",
  install: "写入文件",
};

export const downloadView = computed(() => {
  const status = state.download;
  const titles: Record<string, string> = {
    idle: "尚未检查游戏资源",
    checking: "正在检查游戏资源",
    "needs-install": "需要安装启动器文件",
    "needs-update": "发现待下载的游戏资源",
    "needs-verify": "存在尚未验证的游戏资源",
    launcher: "正在下载启动器文件",
    game: "正在下载游戏资源",
    ready: "游戏资源已就绪",
    cancelled: "下载已暂停",
    failed: "操作失败",
  };
  const phase = status?.phase || "idle";
  const totalBytes = status?.totalBytes || 0;
  const downloadedBytes = status?.downloadedBytes || 0;
  const fileProgress = status?.totalFiles
    ? Math.min(
        100,
        Math.round((status.completedFiles / status.totalFiles) * 100),
      )
    : 0;
  const stageText = DOWNLOAD_STAGE_LABELS[status?.stage || ""] || "";
  return {
    title: titles[phase],
    progress: phase === "ready" ? 100 : Math.min(99, fileProgress),
    downloadedText:
      totalBytes > 0 && downloadedBytes <= totalBytes
        ? `${formatSize(downloadedBytes)} / ${formatSize(totalBytes)}`
        : formatSize(downloadedBytes),
    sizeText: totalBytes ? `本次需下载 ${formatSize(totalBytes)}` : "",
    splitText:
      status && status.patchBytes > 0
        ? status.fullBytes > 0
          ? `补丁 ${formatSize(status.patchBytes)} / 完整文件 ${formatSize(status.fullBytes)}`
          : `补丁 ${formatSize(status.patchBytes)}`
        : "",
    stageText,
    detail:
      status?.error ||
      (status?.unverifiedFiles
        ? `${status.unverifiedFiles} 个文件尚未验证, 请完整校验或安装修复`
        : phase === "checking"
          ? "正在读取官方清单并核对本地文件"
          : phase === "ready"
            ? "资源检查完成，可以启动游戏"
            : phase === "cancelled"
              ? "未完成的文件将在下次安装或更新时继续"
              : status?.pendingFiles
                ? `${status.pendingFiles} 个文件待处理`
                : "选择地区和安装目录后检查资源"),
  };
});
export const homeDownloadNotice = computed(() => {
  const phase =
    state.download?.distribution === form.distribution
      ? state.download.phase
      : "idle";
  return {
    visible: [
      "needs-install",
      "needs-update",
      "needs-verify",
      "failed",
      "cancelled",
    ].includes(phase),
    kind: phase === "failed" ? "error" : "warn",
    text:
      phase === "cancelled"
        ? "下载已暂停，打开更新页继续"
        : downloadView.value.title,
  };
});
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

function applyBootstrap(data: Bootstrap, announceError = true) {
  state.bootstrap = data;
  const settings = data.settings || {};

  const ready = isReady(data);
  ui.setupNeeded = !ready;
  applyTheme(settings.theme || "light");

  form.installRoot =
    settings.installRoot ||
    (settings.distribution !== "global" ? data.installRoot : "") ||
    "";
  form.globalInstallRoot =
    settings.globalInstallRoot ||
    (settings.distribution === "global" ? data.installRoot : "") ||
    "";
  form.distribution = settings.distribution || "cn";
  form.bridgeRoot =
    settings.bridgeRoot ||
    (settings.distribution !== "global" ? data.bridgeRoot : "") ||
    "";
  form.prefix = settings.prefix || data.prefix || "";
  form.protonPath = settings.protonPath || data.protonPath || "";
  form.launchMode = settings.launchMode || "background";

  applyReduceMotion(Boolean(settings.reduceMotion));
  applyHeroMedia(settings.heroMedia || "video");
  form.minimizeToTrayOnClose = settings.minimizeToTrayOnClose ?? true;

  if (data.error && announceError) {
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

function applyDownloadStatus(status: DownloadStatus) {
  downloadRevision++;
  const previous = state.download?.phase;
  state.download = status;
  if (status.phase === "ready" && previous !== "ready") {
    void invoke("bootstrap")
      .then((data) => applyBootstrap(data, false))
      .catch(() => {});
  }
}

export async function loadDownloadStatus() {
  const revision = downloadRevision;
  try {
    const status = await invoke("game_download_status");
    if (revision === downloadRevision && status) applyDownloadStatus(status);
  } catch {}
}

export async function startGameDownload(mode: "check" | "apply", full = false) {
  if (downloadBusy()) return;
  if (!(await saveSettings(false))) return;
  try {
    applyDownloadStatus(await invoke("game_download_start", { mode, full }));
    if (!isTauri()) {
      const poll = () => {
        if (!downloadBusy()) return;
        void loadDownloadStatus().then(() => schedule(poll, 250));
      };
      schedule(poll, 250);
    }
  } catch (error) {
    toast(
      error instanceof Error ? error.message : String(error),
      "error",
      6500,
    );
  }
}

export async function cancelGameDownload() {
  try {
    applyDownloadStatus(await invoke("game_download_cancel"));
  } catch (error) {
    toast(String(error), "error", 6500);
  }
}

function readBulletinCache() {
  try {
    const parsed = JSON.parse(
      window.localStorage.getItem(BULLETIN_CACHE_KEY) || "",
    ) as { savedAt?: unknown; feed?: BulletinFeed };
    if (
      typeof parsed.savedAt !== "number" ||
      !parsed.feed ||
      !Array.isArray(parsed.feed.items) ||
      !Array.isArray(parsed.feed.errors)
    )
      return null;
    return { savedAt: parsed.savedAt, feed: parsed.feed };
  } catch {
    return null;
  }
}

function writeBulletinCache(feed: BulletinFeed) {
  try {
    window.localStorage.setItem(
      BULLETIN_CACHE_KEY,
      JSON.stringify({ savedAt: Date.now(), feed }),
    );
  } catch {}
}

export async function fetchBulletins(silent = true, force = false) {
  const epoch = generation;
  if (state.bulletinsLoading) return;
  const cached = readBulletinCache();
  if (!force && cached && Date.now() - cached.savedAt < BULLETIN_CACHE_TTL_MS) {
    state.bulletins = cached.feed;
    return;
  }
  state.bulletinsLoading = true;
  try {
    const feed = await invoke("fetch_bulletins");
    if (epoch !== generation) return;
    state.bulletins = feed;
    writeBulletinCache(feed);
    if (!silent && feed.errors.length > 0 && feed.items.length === 0) {
      toast("公告暂时无法更新，请稍后重试。", "error", 6000);
    }
  } catch (error) {
    if (epoch !== generation) return;
    if (!state.bulletins) {
      state.bulletins = {
        items: [],
        errors: [error instanceof Error ? error.message : String(error)],
      };
    }
    if (!silent) {
      toast(
        error instanceof Error ? error.message : String(error),
        "error",
        6000,
      );
    }
  } finally {
    if (epoch === generation) state.bulletinsLoading = false;
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
  else if (!state.bootstrap?.gameExists || downloadNeedsWork())
    showPage("update");
  else if (!downloadBusy()) launchGame();
}

export async function saveSettings(announce = true): Promise<boolean> {
  const epoch = generation;
  if (ui.saving) return false;
  const settings = {
    installRoot: form.installRoot.trim() || null,
    globalInstallRoot: form.globalInstallRoot.trim() || null,
    distribution: form.distribution,
    bridgeRoot: form.bridgeRoot.trim() || null,
    prefix: form.prefix.trim() || null,
    protonPath: form.protonPath.trim() || null,
    launchMode: form.launchMode || "background",
    language: "zh-CN",
    reduceMotion: form.reduceMotion,
    theme: applyTheme(form.theme),
    heroMedia: applyHeroMedia(form.heroMedia),
    minimizeToTrayOnClose: form.minimizeToTrayOnClose,
  };
  ui.saving = true;
  try {
    const data = await invoke("save_settings", { settings });
    if (epoch !== generation) return false;
    applyBootstrap(data, announce);
    applyReduceMotion(settings.reduceMotion);
    if (announce && attendance.loaded && attendance.status) {
      try {
        await saveAttendancePreferences();
      } catch (cause) {
        if (epoch === generation)
          toast(
            "设置已保存, 签到设置保存失败: " + attendanceMessage(cause),
            "error",
            6000,
          );
        return false;
      }
    }
    if (epoch === generation && announce) toast("设置已保存.");
    return true;
  } catch (error) {
    if (epoch !== generation) return false;
    toast(
      error instanceof Error ? error.message : String(error),
      "error",
      6000,
    );
    return false;
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
    if (
      event.key === "Tab" &&
      !(
        event.target instanceof Element &&
        event.target.closest('[role="dialog"]')
      )
    )
      event.preventDefault();
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
  disposers.push(listenDownloadState(applyDownloadStatus));
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

  void loadDownloadStatus();
  void fetchBulletins();

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
    download: null,
    bulletins: null,
    bulletinsLoading: false,
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
