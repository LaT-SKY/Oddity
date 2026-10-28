import { defaultAttendanceTasks } from "./attendanceTasks";
import { computed, nextTick, reactive, ref } from "vue";
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
  LauncherBackgroundMedia,
  Settings,
} from "../types/launcher";
import { invoke, isTauri, listenGameState, listenDownloadState } from "./api";
import { notify, resetNotifications } from "./notifications";
export { notify, notifications } from "./notifications";
import { settingsNavigation, type SettingsCategory } from "./settingsCatalog";
import { APP_VERSION } from "./version";

let generation = 0;
let events = new window.AbortController();
const BULLETIN_CACHE_KEY = "nte.bulletins.v2";
let bulletinRequest = 0;
const BULLETIN_CACHE_TTL_MS = 15 * 60 * 1000;
const disposers: (() => void)[] = [];
let downloadRevision = 0;
let bootstrapRevision = 0;
const downloadStarting = ref<"check" | "apply" | null>(null);
export const runtimeEnvironmentBusy = ref(false);
export const runtimeEnvironmentError = ref("");
// 文本输入防抖, 失焦和开关类控件立即保存。
const AUTO_SAVE_DELAY = 500;
const SAVED_STATE_DURATION = 2000;
let autoSaveTimer: number | null = null;
let autoSaveQueued = false;
let autoSaveRunning = false;
let installationRefreshing = false;
let installationRefreshPending = false;
let pendingCheck: { full: boolean } | null = null;
let saveStatusTimer = 0;
let saveChain: Promise<boolean> = Promise.resolve(true);
let lastSavedSettings = "";
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
  tasks: { ...defaultAttendanceTasks },
});
export const gameRecords = reactive({
  loading: false,
  waiting: false,
  error: "",
  nextRefresh: 0,
});
const GAME_RECORD_REFRESH_MS = 30 * 60 * 1000;
let attendanceRevision = 0;
let attendanceStatusRevision = 0;
let gameRecordsRetryTimer: number | null = null;
type AttendanceUpdate = "all" | "sign" | "records";
function mergeAttendance(next: AttendanceStatus, source: AttendanceUpdate) {
  if (source === "all" || !attendance.status) {
    syncAttendance(next, false);
    return;
  }
  attendanceStatusRevision++;
  // 两类任务只提交各自负责的字段, 不覆盖并发完成的结果或账号变更。
  for (const current of attendance.status.accounts) {
    const incoming = next.accounts.find((a) => a.id === current.id);
    if (!incoming) continue;
    if (source === "records") {
      current.gameRecord = incoming.gameRecord;
      current.gameRecordError = incoming.gameRecordError;
    } else {
      current.lastReport = incoming.lastReport;
      current.signedToday = incoming.signedToday;
      current.failedToday = incoming.failedToday;
      current.attention = incoming.attention;
    }
  }
}
export function syncAttendance(next: AttendanceStatus, syncForm = true) {
  attendanceRevision++;
  attendanceStatusRevision++;
  const oldIds = attendance.status?.accounts.map((a) => a.id).join(",");
  if (oldIds !== next.accounts.map((a) => a.id).join(","))
    gameRecords.nextRefresh = 0;
  attendance.status = next;
  if (syncForm) {
    attendanceForm.enabled = next.preferences.trigger !== "off";
    attendanceForm.trigger =
      next.preferences.trigger === "off" ? "launch" : next.preferences.trigger;
    attendanceForm.time = next.preferences.time;
    attendanceForm.tasks = { ...next.preferences.tasks };
  }
}
export async function refreshGameRecords(force = false) {
  if (
    state.bootstrap?.settings.distribution === "global" ||
    !attendance.status?.accounts.length ||
    gameRecords.loading ||
    (attendance.busy && !attendance.running) ||
    (!force && Date.now() < gameRecords.nextRefresh)
  )
    return;
  const epoch = generation;
  const revision = attendanceRevision;
  if (gameRecordsRetryTimer !== null) {
    window.clearTimeout(gameRecordsRetryTimer);
    timers.delete(gameRecordsRetryTimer);
    gameRecordsRetryTimer = null;
  }
  gameRecords.loading = true;
  gameRecords.waiting = false;
  gameRecords.error = "";
  gameRecords.nextRefresh = Date.now() + GAME_RECORD_REFRESH_MS;
  try {
    const next = await invoke("attendance_refresh_game_records");
    // 账号变更后丢弃旧查询, 签到结果由各自字段独立合并。
    if (epoch !== generation || revision !== attendanceRevision) return;
    mergeAttendance(next, "records");
  } catch (cause) {
    if (epoch === generation && revision === attendanceRevision) {
      const busy =
        cause &&
        typeof cause === "object" &&
        "kind" in cause &&
        cause.kind === "busy";
      gameRecords.waiting = !!busy;
      gameRecords.error = busy ? "" : attendanceMessage(cause);
      gameRecords.nextRefresh =
        Date.now() + (busy ? 2000 : GAME_RECORD_REFRESH_MS);
      if (busy)
        gameRecordsRetryTimer = schedule(() => {
          gameRecordsRetryTimer = null;
          if (state.page === "home" && !document.hidden)
            void refreshGameRecords();
        }, 2000);
    }
  } finally {
    if (epoch === generation) gameRecords.loading = false;
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
  source: AttendanceUpdate = "all",
) {
  if (attendance.busy || attendance.running)
    throw new Error("签到操作正在进行, 请稍后重试.");
  const epoch = generation;
  attendance.busy = true;
  attendance.error = "";
  try {
    const next = await action();
    if (epoch === generation) {
      if (source === "all") syncAttendance(next, syncForm);
      else mergeAttendance(next, source);
      if (source === "all") gameRecords.nextRefresh = 0;
    }
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
  const revision = attendanceStatusRevision;
  try {
    const next = await invoke("attendance_status");
    // 轮询不能覆盖正在进行的登录或签到结果。
    if (
      epoch !== generation ||
      revision !== attendanceStatusRevision ||
      attendance.busy ||
      attendance.running ||
      gameRecords.loading
    )
      return;
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
    await saveAttendancePreferences();
    await attendanceAction(
      async () => {
        attendance.running = true;
        try {
          return await invoke("attendance_run");
        } finally {
          attendance.running = false;
        }
      },
      false,
      "sign",
    );
  } catch {
    /* 错误由签到区显示。 */
  }
}
function attendancePreferences() {
  return {
    trigger: attendanceForm.enabled ? attendanceForm.trigger : ("off" as const),
    time: attendanceForm.time,
    tasks: { ...attendanceForm.tasks },
  };
}
export async function saveAttendancePreferences() {
  if (!attendance.status) return;
  const preferences = attendancePreferences();
  const current = attendance.status.preferences;
  if (
    current.trigger === preferences.trigger &&
    current.time === preferences.time &&
    JSON.stringify(current.tasks) === JSON.stringify(preferences.tasks)
  )
    return;
  await attendanceAction(() => invoke("attendance_configure", { preferences }));
}

export const form = reactive({
  installRoot: "",
  globalInstallRoot: "",
  distribution: "cn" as "cn" | "global",
  launchMode: "background",
  gameMode: {
    performance: false,
    inhibit: false,
    notifications: false,
    overlay: false,
  },
  theme: "light",
  heroMedia: "video",
  reduceMotion: false,
  stackBulletins: false,
  minimizeToTrayOnClose: true,
  showLauncherAfterGameExit: true,
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

type SaveState = "idle" | "saving" | "saved" | "error";
export const ui = reactive({
  setupNeeded: false,
  updateError: false,
  saveState: "idle" as SaveState,
  redetecting: false,
  maximized: false,
  introStarted: false,
  introDone: false,
  scrollEdge: { top: false, bottom: false },
});
const BUNDLED_CROP_X = 0.54;
const BUNDLED_CROP_Y = 0.38;
const EMPTY_BACKGROUND: LauncherBackgroundMedia = { video: null, image: null };

export const media = reactive({
  url: "",
  image: "",
  looping: false,
  officialVideo: false,
  officialImage: false,
  cropX: BUNDLED_CROP_X,
  cropY: BUNDLED_CROP_Y,
  videoKey: "",
  imageKey: "",
  loopCut: 1,
  initialized: !isTauri(),
});
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
        icon: "i-download-simple",
      }
    : LAUNCH_COPY[state.game]),
  busy:
    runtimeEnvironmentBusy.value ||
    state.game === "launching" ||
    state.game === "closing" ||
    downloadBlocksLaunch(),
}));
function downloadNeedsWork() {
  return (
    state.download?.distribution === state.bootstrap?.settings.distribution &&
    ["needs-install", "needs-update", "needs-verify"].includes(
      state.download?.phase || "idle",
    )
  );
}
export function downloadBusy() {
  return (
    downloadStarting.value !== null ||
    ["checking", "launcher", "game"].includes(state.download?.phase || "")
  );
}
function downloadBlocksLaunch() {
  return (
    downloadBusy() &&
    (downloadStarting.value || state.download?.mode) === "apply"
  );
}
// 只有实际进入下载写入后才锁定路径; 启动下载前的保存仍读取表单。
const installPathsBusy = computed(
  () =>
    state.game !== "idle" ||
    (state.download?.mode === "apply" &&
      ["checking", "launcher", "game"].includes(state.download?.phase || "")),
);
export const settingsPathsLocked = computed(
  () => runtimeEnvironmentBusy.value || installPathsBusy.value,
);
export const runtimePathsLocked = computed(
  () => runtimeEnvironmentBusy.value || state.game !== "idle",
);
export const runtimeEnvironmentLocked = computed(
  () =>
    runtimeEnvironmentBusy.value ||
    state.game !== "idle" ||
    state.checking ||
    downloadBusy(),
);
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

export const diskBudget = computed(() => {
  const status = state.download;
  const available = status?.diskAvailableBytes ?? null;
  const required = status?.diskRequiredBytes ?? 0;
  if (available === null || required <= 0) return null;
  const shortfall = Math.max(0, required - available);
  const percent =
    available > 0 ? Math.min(100, (required / available) * 100) : 100;
  return {
    availableText: formatSize(available),
    requiredText: formatSize(required),
    shortfallText: formatSize(shortfall),
    insufficient: shortfall > 0,
    percent,
  };
});

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
    cancelled: status?.mode === "check" ? "资源检查已取消" : "下载已暂停",
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
  // 进度按下载体积推进, 避免不同大小的文件在阶段切换时让百分比来回跳动.
  const byteProgress = totalBytes
    ? Math.round((downloadedBytes / totalBytes) * 100)
    : fileProgress;
  const stageText = DOWNLOAD_STAGE_LABELS[status?.stage || ""] || "";
  const launcherScope = phase === "needs-install" || phase === "launcher";
  return {
    title: titles[phase],
    progress: phase === "ready" ? 100 : Math.min(99, byteProgress),
    downloadedText:
      totalBytes > 0 && downloadedBytes <= totalBytes
        ? `${formatSize(downloadedBytes)} / ${formatSize(totalBytes)}`
        : formatSize(downloadedBytes),
    sizeText: totalBytes
      ? `${launcherScope ? "启动器文件" : "本次需下载"} ${formatSize(totalBytes)}`
      : "",
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
            ? "资源检查完成, 可以启动游戏"
            : phase === "cancelled"
              ? status?.mode === "check"
                ? "可重新检查游戏资源"
                : "未完成的文件将在下次安装或更新时继续"
              : phase === "needs-install"
                ? "启动器文件尚未安装, 完成后将继续检查游戏资源"
                : status?.pendingFiles
                  ? `${status.pendingFiles} 个文件待处理`
                  : "选择地区和安装目录后检查资源"),
  };
});
export const homeDownloadNotice = computed(() => {
  const phase =
    state.download?.distribution === state.bootstrap?.settings.distribution
      ? state.download?.phase || "idle"
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
        ? state.download?.mode === "check"
          ? "资源检查已取消, 打开更新页重新检查"
          : "下载已暂停, 打开更新页继续"
        : downloadView.value.title,
  };
});
function normalizedRoot(value: string) {
  return value.replace(/\/+$/, "");
}

function currentInstallRoot() {
  const data = state.bootstrap;
  const root =
    form.distribution === "cn" ? form.installRoot : form.globalInstallRoot;
  const saved =
    data?.settings.distribution === form.distribution
      ? form.distribution === "cn"
        ? data.settings.installRoot
        : data.settings.globalInstallRoot
      : null;
  if (
    data?.settings.distribution === form.distribution &&
    normalizedRoot(root.trim()) === normalizedRoot(saved || data.installRoot)
  )
    return data.installRoot;
  return root.trim();
}

// 检查结果只对当前的地区和目录有效.
const checkStale = computed(() => {
  const result = state.check;
  if (!result) return false;
  return (
    result.distribution !== form.distribution ||
    normalizedRoot(result.installRoot) !== normalizedRoot(currentInstallRoot())
  );
});

export const updateView = computed(() => {
  const bootstrap = state.bootstrap;
  const launcherInstalled = Boolean(bootstrap?.launcherRoot);
  const result = checkStale.value || !launcherInstalled ? null : state.check;
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
    kind: !bootstrap || launcherInstalled ? "idle" : "warn",
    title: !bootstrap || launcherInstalled ? "尚未检查" : "尚未安装",
    copy: !bootstrap
      ? "等待检查"
      : launcherInstalled
        ? bootstrap.launcherVersion
          ? "本地版本 " + bootstrap.launcherVersion
          : "等待检查"
        : "此目录尚未安装启动器文件, 请先安装游戏",
    icon:
      bootstrap && !launcherInstalled
        ? "i-download-simple"
        : "i-arrows-clockwise",
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
  else if (result && !result.installed)
    status = {
      kind: "warn",
      title: "尚未安装",
      copy: "此目录尚未安装启动器文件, 请先安装游戏",
      icon: "i-download-simple",
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
const BUNDLED_VIDEO_CANDIDATES = [
  { url: "assets/hero-loop.mp4", type: 'video/mp4; codecs="avc1.42E01E"' },
  { url: "assets/hero-loop.webm", type: 'video/webm; codecs="vp9"' },
];
let heroVideoRequest: Promise<boolean> | null = null;
let heroImageRequest: Promise<void> | null = null;
let launcherBackground: LauncherBackgroundMedia = { ...EMPTY_BACKGROUND };
let launcherBackgroundKey = "";
let launcherBackgroundRequest: {
  key: string;
  promise: Promise<LauncherBackgroundMedia>;
} | null = null;
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

function heroBackgroundKey(data: Bootstrap | null = state.bootstrap) {
  return data
    ? `${data.settings?.distribution || ""}|${data.launcherRoot || ""}`
    : "";
}

async function resolveLauncherBackground(): Promise<LauncherBackgroundMedia> {
  if (!isTauri()) return EMPTY_BACKGROUND;
  const key = heroBackgroundKey();
  if (!key) return EMPTY_BACKGROUND;
  if (key === launcherBackgroundKey) return launcherBackground;
  if (launcherBackgroundRequest?.key === key)
    return launcherBackgroundRequest.promise;

  const epoch = generation;
  const request: {
    key: string;
    promise: Promise<LauncherBackgroundMedia>;
  } = {
    key,
    promise: Promise.resolve(EMPTY_BACKGROUND),
  };
  request.promise = invoke("launcher_background")
    .then((next) => {
      if (epoch === generation && key === heroBackgroundKey()) {
        launcherBackground = next;
        launcherBackgroundKey = key;
      }
      return launcherBackground;
    })
    .catch(() => launcherBackground)
    .finally(() => {
      if (launcherBackgroundRequest === request)
        launcherBackgroundRequest = null;
    });
  launcherBackgroundRequest = request;
  return request.promise;
}

function releaseHeroMedia() {
  stopHeroVideo(videoElement);
  if (media.url.startsWith("blob:")) URL.revokeObjectURL(media.url);
  if (media.image.startsWith("blob:")) URL.revokeObjectURL(media.image);
  media.url = "";
  media.image = "";
  media.initialized = !isTauri();
  media.officialVideo = false;
  media.officialImage = false;
  media.cropX = BUNDLED_CROP_X;
  media.cropY = BUNDLED_CROP_Y;
  media.videoKey = "";
  media.imageKey = "";
  media.loopCut = 1;
  launcherBackground = { ...EMPTY_BACKGROUND };
  launcherBackgroundKey = "";
  heroVideoRequest = null;
  heroImageRequest = null;
}

function refreshLauncherBackground(data: Bootstrap) {
  if (!isTauri()) return;
  const key = heroBackgroundKey(data);
  if (key === launcherBackgroundKey) return;
  releaseHeroMedia();
  const epoch = generation;
  void resolveLauncherBackground().then(() => {
    if (epoch !== generation || key !== heroBackgroundKey()) return;
    if (!launcherBackground.image) media.initialized = true;
    if (ui.introDone && state.page === "home") applyHeroMedia(state.heroMedia);
  });
}

async function loadHeroImage() {
  if (!isTauri() || state.page !== "home") return;
  const epoch = generation;
  const key = heroBackgroundKey();
  if (!key || media.imageKey === key) return;
  if (heroImageRequest) return heroImageRequest;

  const promise = (async () => {
    const source = await resolveLauncherBackground();
    if (epoch !== generation || key !== heroBackgroundKey()) return;
    media.imageKey = key;
    media.initialized = true;
    if (!source.image) {
      media.officialImage = false;
      return;
    }
    if (media.image.startsWith("blob:")) URL.revokeObjectURL(media.image);
    media.image = source.image;
    media.officialImage = true;
  })();
  heroImageRequest = promise;
  promise.finally(() => {
    if (heroImageRequest === promise) heroImageRequest = null;
  });
  return promise;
}

async function loadHeroVideo(video: HTMLVideoElement) {
  const epoch = generation;
  const key = heroBackgroundKey();
  if (media.url && media.videoKey === key) return true;
  if (heroVideoRequest) return heroVideoRequest;

  const promise = (async () => {
    const source = await resolveLauncherBackground();
    if (epoch !== generation || key !== heroBackgroundKey()) return false;
    if (media.url && media.videoKey === key) return true;

    let url = "";
    let official = false;
    if (source.video && video.canPlayType("video/mp4")) {
      url = source.video;
      official = true;
    }
    if (!url) {
      const chosen = BUNDLED_VIDEO_CANDIDATES.find((candidate) =>
        video.canPlayType(candidate.type),
      );
      if (!chosen) return false;
      const response = await fetch(chosen.url, { cache: "no-store" });
      if (!response.ok) return false;
      const blob = await response.blob();
      if (epoch !== generation || key !== heroBackgroundKey()) return false;
      url = URL.createObjectURL(blob);
    }
    if (media.url.startsWith("blob:")) URL.revokeObjectURL(media.url);
    media.url = url;
    media.videoKey = key;
    media.officialVideo = official;
    media.cropX = official ? 0.5 : BUNDLED_CROP_X;
    media.cropY = official ? 0.5 : BUNDLED_CROP_Y;
    media.loopCut = official ? 0.02 : 1;
    await nextTick();
    if (epoch !== generation || key !== heroBackgroundKey()) return false;
    video.load();
    return true;
  })().catch(() => false);
  heroVideoRequest = promise;
  promise.finally(() => {
    if (heroVideoRequest === promise) heroVideoRequest = null;
  });
  return promise;
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

function startHeroVideo(video: HTMLVideoElement) {
  media.looping = true;
  requestHeroPlayback(video);
}

function stopHeroVideo(video: HTMLVideoElement | null, reset = true) {
  media.looping = false;
  if (!video) return;
  video.pause();
  if (!reset) return;
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

  if (state.page === "home") void loadHeroImage();

  const video = videoElement;
  if (!video) return value;

  if (!canPlayHeroVideo()) {
    stopHeroVideo(video, state.page === "home");
    return value;
  }

  const epoch = generation;
  loadHeroVideo(video).then((loaded) => {
    if (epoch !== generation) return;
    if (!loaded) {
      stopHeroVideo(video, state.page === "home");
      return;
    }
    if (!canPlayHeroVideo()) {
      stopHeroVideo(video, state.page === "home");
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

function applyBootstrap(
  data: Bootstrap,
  announceError = true,
  syncForm = true,
) {
  bootstrapRevision++;
  const previous = state.bootstrap?.settings;
  const previousResolvedRoot = state.bootstrap?.installRoot;
  state.bootstrap = data;
  if (previous && previous.distribution !== data.settings.distribution) {
    bulletinRequest++;
    state.bulletins = null;
    state.bulletinsLoading = false;
    void fetchBulletins();
    gameRecords.nextRefresh = 0;
  }

  refreshLauncherBackground(data);
  const settings = data.settings || {};
  if (
    previous &&
    installPathsChanged(previous, data.settings, previousResolvedRoot)
  ) {
    state.check = null;
    state.download = null;
    ui.updateError = false;
  }

  const ready = isReady(data);
  ui.setupNeeded = !ready;
  if (data.error && announceError) {
    notify(data.error, "error", 6000, {
      key: "bootstrap",
      primary: {
        label: "打开设置",
        icon: "i-gear-six",
        run: () => openSettings(data.prefixExists ? "maintenance" : "runtime"),
      },
    });
  }
  if (!syncForm) return;
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
  form.launchMode = settings.launchMode || "background";
  Object.assign(
    form.gameMode,
    {
      performance: false,
      inhibit: false,
      notifications: false,
      overlay: false,
    },
    settings.gameMode,
  );

  form.stackBulletins = settings.stackBulletins;
  applyReduceMotion(Boolean(settings.reduceMotion));
  applyHeroMedia(settings.heroMedia || "video");
  form.minimizeToTrayOnClose = settings.minimizeToTrayOnClose ?? true;
  form.showLauncherAfterGameExit = settings.showLauncherAfterGameExit ?? true;
  // 记录表单当前对应的持久化状态, 自动保存时只提交真正的修改。
  lastSavedSettings = JSON.stringify(currentSettings());
  void nextTick(syncScrollEdges);
}

export async function checkUpdate(full = false, silent = false) {
  const epoch = generation;
  if (runtimeEnvironmentBusy.value) return;
  if (state.checking) {
    // 本轮结束后再执行最后一次用户主动检查.
    if (!silent) pendingCheck = { full };
    return;
  }
  state.checking = true;
  ui.updateError = false;
  try {
    // 手动检查前先保存目录, 保证检查的就是界面上的目录.
    if (!silent && !(await saveSettings(silent))) return;
    if (epoch !== generation) return;
    const result = await invoke("check_update", { full });
    if (epoch !== generation) return;
    state.check = result;
  } catch (error) {
    if (epoch !== generation) return;
    const message = error instanceof Error ? error.message : String(error);
    ui.updateError = true;
    if (!silent)
      notify(message, "error", 6500, {
        key: "update-check",
        primary: {
          label: "重试",
          icon: "i-arrows-clockwise",
          run: () => checkUpdate(full, false),
        },
      });
  } finally {
    if (epoch === generation) {
      state.checking = false;
      const pending = pendingCheck;
      pendingCheck = null;
      if (pending && !ui.updateError) void checkUpdate(pending.full, false);
    }
  }
}

function applyDownloadStatus(status: DownloadStatus) {
  downloadRevision++;
  const previous = state.download?.phase;
  state.download = status;
  if (status.phase === "ready" && previous !== "ready") {
    const epoch = generation;
    const revision = downloadRevision;
    const bootstrapVersion = bootstrapRevision;
    void invoke("bootstrap")
      .then((data) => {
        if (
          epoch === generation &&
          revision === downloadRevision &&
          bootstrapVersion === bootstrapRevision
        )
          applyBootstrap(data, false, false);
      })
      .catch(() => {});
  }
}

export async function loadDownloadStatus() {
  const revision = downloadRevision;
  const epoch = generation;
  try {
    const status = await invoke("game_download_status");
    if (epoch === generation && revision === downloadRevision && status)
      applyDownloadStatus(status);
  } catch {}
}

function pollDownloadStatus() {
  if (isTauri()) return;
  const poll = () => {
    if (!downloadBusy()) return;
    void loadDownloadStatus().then(() => schedule(poll, 250));
  };
  schedule(poll, 250);
}

export async function startGameDownload(
  mode: "check" | "apply",
  full = false,
  silent = false,
) {
  if (runtimeEnvironmentBusy.value || downloadBusy()) return;
  const epoch = generation;
  downloadStarting.value = mode;
  try {
    if (!silent && !(await saveSettings(silent))) return;
    if (epoch !== generation) return;
    const revision = downloadRevision;
    const status = await invoke("game_download_start", { mode, full });
    if (epoch !== generation) return;
    if (revision === downloadRevision) applyDownloadStatus(status);
    pollDownloadStatus();
  } catch (error) {
    if (epoch === generation && !silent)
      notify(
        error instanceof Error ? error.message : String(error),
        "error",
        6500,
        {
          key: "game-download",
          primary: {
            label: "重试",
            icon: "i-arrows-clockwise",
            run: () => startGameDownload(mode, full, false),
          },
        },
      );
  } finally {
    if (epoch === generation) downloadStarting.value = null;
  }
}

// 启动器文件与游戏资源使用同一轮静默刷新。
async function refreshInstallationStatus(includeGame = true) {
  if (runtimeEnvironmentBusy.value || installationRefreshing) {
    installationRefreshPending = true;
    return;
  }
  if (state.checking || downloadBusy()) return;
  if (!state.bootstrap?.launcherRoot) return;
  const epoch = generation;
  installationRefreshing = true;
  try {
    await Promise.all([
      checkUpdate(false, true),
      includeGame ? startGameDownload("check", false, true) : true,
    ]);
  } finally {
    if (epoch === generation) {
      installationRefreshing = false;
      if (installationRefreshPending) {
        installationRefreshPending = false;
        void refreshInstallationStatus();
      }
    }
  }
}

export async function cancelGameDownload() {
  const epoch = generation;
  const revision = downloadRevision;
  try {
    const status = await invoke("game_download_cancel");
    if (epoch === generation && revision === downloadRevision)
      applyDownloadStatus(status);
  } catch (error) {
    notify(String(error), "error", 6500);
  }
}

function readBulletinCache(distribution: string) {
  try {
    const parsed = JSON.parse(
      window.localStorage.getItem(`${BULLETIN_CACHE_KEY}.${distribution}`) ||
        "",
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

function writeBulletinCache(feed: BulletinFeed, distribution: string) {
  try {
    window.localStorage.setItem(
      `${BULLETIN_CACHE_KEY}.${distribution}`,
      JSON.stringify({ savedAt: Date.now(), feed }),
    );
  } catch {}
}

export async function fetchBulletins(silent = true, force = false) {
  const epoch = generation;
  const distribution = state.bootstrap?.settings.distribution || "cn";
  if (state.bulletinsLoading) return;
  const cached = readBulletinCache(distribution);
  if (
    !force &&
    cached &&
    cached.feed.errors.length === 0 &&
    !state.bulletins?.errors.length &&
    Date.now() - cached.savedAt < BULLETIN_CACHE_TTL_MS
  ) {
    state.bulletins = cached.feed;
    return;
  }
  const request = ++bulletinRequest;
  if (!state.bulletins && cached) state.bulletins = cached.feed;
  state.bulletinsLoading = true;
  try {
    const feed = await invoke("fetch_bulletins", { distribution });
    if (epoch !== generation || request !== bulletinRequest) return;
    state.bulletins = feed;
    if (feed.errors.length === 0) writeBulletinCache(feed, distribution);
    if (!silent && feed.errors.length > 0 && feed.items.length === 0) {
      notify("公告暂时无法更新，请稍后重试。", "error", 6000, {
        key: "bulletins",
        primary: {
          label: "重试",
          icon: "i-arrows-clockwise",
          run: () => fetchBulletins(false, true),
        },
        secondary: { label: "查看公告", run: () => showPage("bulletin") },
      });
    }
  } catch (error) {
    if (epoch !== generation || request !== bulletinRequest) return;
    if (!state.bulletins) {
      state.bulletins = {
        items: [],
        errors: [error instanceof Error ? error.message : String(error)],
      };
    }
    if (!silent) {
      notify(
        error instanceof Error ? error.message : String(error),
        "error",
        6000,
        {
          key: "bulletins",
          primary: {
            label: "重试",
            icon: "i-arrows-clockwise",
            run: () => fetchBulletins(false, true),
          },
          secondary: { label: "查看公告", run: () => showPage("bulletin") },
        },
      );
    }
  } finally {
    if (epoch === generation && request === bulletinRequest)
      state.bulletinsLoading = false;
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
  if (announce && status.error) notify(status.error, "error", 7500);
  if (status.running) {
    state.session = status;
    setLaunchState(status.closing ? "closing" : "running");
    return;
  }
  state.session = null;
  setLaunchState("idle");
  if (announce && !status.error && previous === "running")
    notify("游戏已退出。");
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
  if (state.game !== "idle" || runtimeEnvironmentBusy.value) return;
  if (!state.bootstrap?.prefixExists) {
    openSettings("runtime");
    notify("请先在设置中创建并选择 Proton 运行环境.", "error", 6500, {
      key: "runtime-required",
      primary: {
        label: "打开设置",
        icon: "i-gear-six",
        run: () => openSettings("runtime"),
      },
    });
    return;
  }
  if (!isTauri()) notify("浏览器预览：模拟启动流程。", "ok", 2600);
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
      notify(status.error || "游戏进程已退出，启动未完成。", "error", 7000);
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
    notify(
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
    notify("游戏已关闭。");
  } catch (error) {
    if (epoch !== generation) return;
    let status = null;
    try {
      status = await invoke("game_status");
    } catch {}
    if (epoch !== generation) return;
    if (status) applyGameStatus(status, { force: true });
    else setLaunchState("running");
    notify(
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
  else if (!downloadBlocksLaunch()) launchGame();
}

function installPathsChanged(
  previous: Settings | undefined,
  next: Settings,
  previousResolvedRoot?: string,
) {
  if (!previous) return false;
  if (previous.distribution !== next.distribution) return true;
  const previousRoot =
    previous.distribution === "cn"
      ? previous.installRoot
      : previous.globalInstallRoot;
  const nextRoot =
    next.distribution === "cn" ? next.installRoot : next.globalInstallRoot;
  if (normalizedRoot(previousRoot || "") === normalizedRoot(nextRoot || ""))
    return false;
  // 首次自动保存会把界面上的默认探测目录写入设置, 这不算切换目录。
  if (
    !previousRoot &&
    nextRoot &&
    normalizedRoot(nextRoot) === normalizedRoot(previousResolvedRoot || "")
  )
    return false;
  return true;
}

function currentSettings(): Settings {
  // 下载写入期间沿用已持久化的路径, 避免把自动探测到的目录当成修改提交。
  const persisted = installPathsBusy.value
    ? state.bootstrap?.settings
    : undefined;
  const launchMode = ["official", "background", "direct"].includes(
    form.launchMode,
  )
    ? form.launchMode
    : "background";
  return {
    installRoot: persisted
      ? persisted.installRoot
      : form.installRoot.trim() || null,
    globalInstallRoot: persisted
      ? persisted.globalInstallRoot
      : form.globalInstallRoot.trim() || null,
    distribution: persisted ? persisted.distribution : form.distribution,
    runtimeEnvironments: JSON.parse(
      JSON.stringify(state.bootstrap?.settings.runtimeEnvironments ?? []),
    ),
    activeEnvironment: state.bootstrap?.settings.activeEnvironment ?? null,
    stableEnvironment: state.bootstrap?.settings.stableEnvironment ?? null,
    launchMode,
    gameMode: { ...form.gameMode },
    language: "zh-CN",
    reduceMotion: form.reduceMotion,
    stackBulletins: form.stackBulletins,
    theme: THEMES.has(form.theme) ? form.theme : "light",
    heroMedia: HERO_MEDIA.has(form.heroMedia) ? form.heroMedia : "video",
    minimizeToTrayOnClose: form.minimizeToTrayOnClose,
    showLauncherAfterGameExit: form.showLauncherAfterGameExit,
  };
}

function clearSaveStatusTimer() {
  if (!saveStatusTimer) return;
  window.clearTimeout(saveStatusTimer);
  timers.delete(saveStatusTimer);
  saveStatusTimer = 0;
}

function markSaved() {
  clearSaveStatusTimer();
  ui.saveState = "saved";
  saveStatusTimer = schedule(() => {
    saveStatusTimer = 0;
    if (ui.saveState === "saved") ui.saveState = "idle";
  }, SAVED_STATE_DURATION);
}

function cancelAutoSaveTimer() {
  if (autoSaveTimer === null) return;
  window.clearTimeout(autoSaveTimer);
  timers.delete(autoSaveTimer);
  autoSaveTimer = null;
}

// 表单事件只登记修改, 由统一队列合并并串行写入。
export function queueSettingsSave(delay = AUTO_SAVE_DELAY) {
  autoSaveQueued = true;
  if (ui.saveState === "saved" || ui.saveState === "error") {
    clearSaveStatusTimer();
    ui.saveState = "idle";
  }
  cancelAutoSaveTimer();
  if (delay <= 0) {
    void flushAutoSave();
    return;
  }
  autoSaveTimer = schedule(() => {
    autoSaveTimer = null;
    void flushAutoSave();
  }, delay);
}

async function flushAutoSave() {
  if (autoSaveRunning) return;
  autoSaveRunning = true;
  const epoch = generation;
  try {
    let ok = true;
    let performed = false;
    while (autoSaveQueued) {
      performed = true;
      autoSaveQueued = false;
      ok = await enqueuePersist(false, epoch, true);
      if (!ok) break;
    }
    // 连续修改合并为多次写入时, 只在最后一次成功后显示已保存。
    if (performed && ok && !autoSaveQueued) markSaved();
  } finally {
    autoSaveRunning = false;
  }
}

function enqueuePersist(
  silent: boolean,
  epoch = generation,
  refreshOnPathChange = false,
): Promise<boolean> {
  const run = () => persistCurrentSettings(silent, epoch, refreshOnPathChange);
  // 失败也不能中断后续保存, 因此串行链始终以已处理状态继续。
  const next = saveChain.then(run, run);
  saveChain = next.catch(() => false);
  return next;
}

async function persistCurrentSettings(
  silent: boolean,
  epoch: number,
  refreshOnPathChange = false,
): Promise<boolean> {
  if (epoch !== generation) return false;
  const previous = state.bootstrap?.settings;
  const previousResolvedRoot = state.bootstrap?.installRoot;
  const settings = currentSettings();
  const serialized = JSON.stringify(settings);
  const changed = serialized !== lastSavedSettings;
  const preferences = attendancePreferences();
  const savedPreferences = attendance.status?.preferences;
  const preferencesChanged = Boolean(
    attendance.loaded &&
    savedPreferences &&
    (savedPreferences.trigger !== preferences.trigger ||
      savedPreferences.time !== preferences.time ||
      JSON.stringify(savedPreferences.tasks) !==
        JSON.stringify(preferences.tasks)),
  );
  if (!changed && !preferencesChanged) return true;

  clearSaveStatusTimer();
  ui.saveState = "saving";
  try {
    if (changed) {
      const data = await invoke("save_settings", { settings });
      if (epoch !== generation) return false;
      lastSavedSettings = serialized;
      // 保存响应只刷新环境状态, 不覆盖请求期间继续修改的表单。
      applyBootstrap(data, false, false);
      if (
        refreshOnPathChange &&
        installPathsChanged(previous, settings, previousResolvedRoot)
      ) {
        void refreshInstallationStatus();
      }
    }
    if (preferencesChanged) {
      try {
        await saveAttendancePreferences();
      } catch (cause) {
        if (epoch !== generation) return false;
        if (!silent)
          notify(
            "签到设置保存失败: " + attendanceMessage(cause),
            "error",
            6000,
          );
        ui.saveState = "error";
        return false;
      }
    }
    if (epoch !== generation) return false;
    return true;
  } catch (error) {
    if (epoch !== generation) return false;
    if (!silent)
      notify(
        error instanceof Error ? error.message : String(error),
        "error",
        6000,
      );
    ui.saveState = "error";
    return false;
  }
}

export async function saveSettings(silent = false): Promise<boolean> {
  const epoch = generation;
  autoSaveQueued = false;
  cancelAutoSaveTimer();
  await saveChain;
  if (epoch !== generation) return false;
  const ok = await enqueuePersist(silent, epoch);
  if (ok && epoch === generation && !autoSaveQueued) markSaved();
  return ok;
}

async function changeEnvironment(
  operation: () => Promise<Bootstrap>,
): Promise<boolean> {
  if (runtimeEnvironmentLocked.value) return false;
  const epoch = generation;
  runtimeEnvironmentError.value = "";
  // 先占用操作入口, 防止保存表单期间的自动检查抢占安装锁.
  runtimeEnvironmentBusy.value = true;
  try {
    if (!(await saveSettings()) || epoch !== generation) return false;
    const data = await operation();
    if (epoch !== generation) return false;
    applyBootstrap(data);
    notify("运行环境已更新.");
    return true;
  } catch (cause) {
    if (epoch === generation) {
      runtimeEnvironmentError.value = String(cause);
      notify(runtimeEnvironmentError.value, "error", 6500);
    }
    return false;
  } finally {
    if (epoch === generation) {
      runtimeEnvironmentBusy.value = false;
      if (installationRefreshPending) {
        installationRefreshPending = false;
        void refreshInstallationStatus();
      }
    }
  }
}

export function createRuntimeEnvironment(
  name: string,
  protonPath: string,
  sourceId: string | null,
  distribution: "cn" | "global",
) {
  return changeEnvironment(() =>
    invoke("environment_create", { name, protonPath, sourceId, distribution }),
  );
}

export function runtimeEnvironmentAction(
  action: "switch" | "keep" | "discard",
  id: string | null,
) {
  return changeEnvironment(() => invoke("environment_action", { action, id }));
}

export async function redetect() {
  const epoch = generation;
  if (ui.redetecting) return;
  ui.redetecting = true;
  try {
    // 先写入未保存的修改, 失败时保留表单并让错误提示留在界面上。
    if (!(await saveSettings())) return;
    if (epoch !== generation) return;
    const data = await invoke("bootstrap");
    if (epoch !== generation) return;
    applyBootstrap(data);
    notify("已重新检测环境。");
  } catch (error) {
    if (epoch !== generation) return;
    notify(
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
    notify("预览模式：已在新标签页打开。");
    return;
  }
  try {
    await invoke("open_url", { url });
    if (epoch !== generation) return;
    notify("已在浏览器打开。");
  } catch (error) {
    if (epoch !== generation) return;
    notify(
      error instanceof Error ? error.message : String(error),
      "error",
      6000,
    );
  }
}

function hashPage() {
  const value = window.location.hash.replace(/^#/, "");
  return ["home", "update", "game-mode", "bulletin", "settings"].includes(value)
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

export function openSettings(category: SettingsCategory) {
  settingsNavigation.category = category;
  settingsNavigation.query = "";
  showPage("settings");
}

export function showPage(name: string | undefined, syncHash = true) {
  if (
    !name ||
    !["home", "update", "game-mode", "bulletin", "settings"].includes(name)
  )
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
    notify(error instanceof Error ? error.message : String(error), "error");
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
    const index = ["1", "2", "3", "4", "5"].indexOf(event.key);
    if (index >= 0) {
      event.preventDefault();
      showPage(["home", "update", "game-mode", "bulletin", "settings"][index]);
    }
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
  // 子组件先挂载时发起的读取属于上一代, 在此重新加载当前状态。
  attendance.loading = false;
  void loadAttendance();
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
    notify(
      error instanceof Error ? error.message : String(error),
      "error",
      6500,
    );
  }

  void loadDownloadStatus();
  void fetchBulletins();

  startIntro();

  if (state.bootstrap && (state.bootstrap.ok || state.bootstrap.gameExists)) {
    whenIntroDone(() => {
      schedule(
        () =>
          void refreshInstallationStatus(Boolean(state.bootstrap?.gameExists)),
        400,
      );
    });
  }
}

export function dispose() {
  generation++;
  runtimeEnvironmentBusy.value = false;
  runtimeEnvironmentError.value = "";
  Object.assign(gameRecords, {
    loading: false,
    waiting: false,
    error: "",
    nextRefresh: 0,
  });
  gameRecordsRetryTimer = null;
  downloadStarting.value = null;
  autoSaveQueued = false;
  autoSaveRunning = false;
  autoSaveTimer = null;
  installationRefreshing = false;
  installationRefreshPending = false;
  pendingCheck = null;
  saveStatusTimer = 0;
  lastSavedSettings = "";
  saveChain = Promise.resolve(true);
  events.abort();
  disposers.splice(0).forEach((stop) => stop());
  timers.forEach((id) => window.clearTimeout(id));
  timers.clear();
  pendingWaits.forEach((resolve) => resolve());
  pendingWaits.clear();
  releaseHeroMedia();
  videoElement = null;
  launcherBackgroundRequest = null;
  pendingHeroMedia = null;
  introWaiters.length = 0;
  ui.introStarted = false;
  ui.introDone = false;
  resetNotifications();
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
    saveState: "idle",
    redetecting: false,
    updateError: false,
    setupNeeded: false,
    maximized: false,
    scrollEdge: { top: false, bottom: false },
  });
}
