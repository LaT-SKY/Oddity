import { defaultGamepadOptions } from "../types/gamepad";
import { defaultGamescope } from "./gameMode";
import { defaultAttendanceTasks } from "./attendanceTasks";
import { distributions } from "./distribution";
import { APP_VERSION } from "./version";
import { invoke as tauriInvoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Bootstrap,
  CheckResult,
  Settings,
  GameStatus,
  LaunchProgress,
  LaunchStage,
  DownloadStatus,
  Commands,
  CommandArgs,
  PreviewPaths,
  BulletinFeed,
  PlaytimeHistory,
  PlaytimeSession,
  AttendanceStatus,
  AttendancePreferences,
  SnapshotComparison,
  SnapshotDiffRow,
  SnapshotOverview,
  SnapshotReference,
  SnapshotSummary,
} from "../types/launcher";
export { isTauri };
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value));

const PREVIEW_KEYS = [
  "home",
  "dataHome",
  "steamRoot",
  "installRoot",
  "protonName",
] as const;

function readStoredPreview(): PreviewPaths {
  try {
    return (
      JSON.parse(window.localStorage.getItem("nte.preview.paths") || "{}") || {}
    );
  } catch {
    return {};
  }
}

function readQueryPreview() {
  const params = new URLSearchParams(window.location.search);
  const query: PreviewPaths = {};
  for (const key of PREVIEW_KEYS) {
    const value = params.get(key);
    if (value && value.trim()) query[key] = value.trim();
  }
  return query;
}

// 预览路径按查询参数、本地存储、窗口配置的顺序覆盖默认值。
function previewOverrides() {
  const sources = [
    window.__NTE_PREVIEW__,
    readStoredPreview(),
    readQueryPreview(),
  ];
  const overrides: PreviewPaths = {};
  for (const source of sources) {
    if (!source || typeof source !== "object") continue;
    for (const key of PREVIEW_KEYS) {
      if (typeof source[key] === "string" && source[key].trim()) {
        overrides[key] = source[key].trim();
      }
    }
  }
  return overrides;
}

const joinPath = (...parts: string[]) =>
  parts
    .filter((part) => part != null && part !== "")
    .map((part, index) =>
      index === 0
        ? String(part).replace(/\/+$/, "")
        : String(part).replace(/^\/+|\/+$/g, ""),
    )
    .join("/");

function previewPaths() {
  const overrides = previewOverrides();
  const home = overrides.home || "~";
  const dataHome = overrides.dataHome || joinPath(home, ".local/share");
  const steamRoot = overrides.steamRoot || joinPath(dataHome, "Steam");
  const installRoot =
    overrides.installRoot || joinPath(home, "Games/Neverness To Everness");
  const protonName = overrides.protonName || "GE-Proton11-5-x86_64";
  return {
    installRoot,
    launcherRoot: joinPath(installRoot, distributions.cn.launcherDir),
    prefix: joinPath(dataHome, "oddity/prefix"),
    protonName,
    protonPath: joinPath(
      steamRoot,
      "compatibilitytools.d",
      protonName,
      "proton",
    ),
    gameExe: joinPath(
      installRoot,
      "Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
    ),
    sdkPath: joinPath(
      installRoot,
      distributions.cn.launcherDir,
      "WPGameClientSDK_64.dll",
    ),
  };
}

function previewOs() {
  const platform =
    navigator.userAgentData?.platform || navigator.platform || "";
  const family = /win/i.test(platform)
    ? "windows"
    : /mac/i.test(platform)
      ? "macos"
      : "linux";
  const arch = /arm64|aarch64/i.test(navigator.userAgent)
    ? "aarch64"
    : "x86_64";
  return `${family} / ${arch} / browser preview`;
}

const preview = previewPaths();

const mock: Bootstrap = {
  ok: false,
  error: "请先在设置中创建并选择 Proton 运行环境",
  installRoot: preview.installRoot,
  launcherRoot: preview.launcherRoot,
  prefix: "",
  prefixExists: false,
  protonName: "",
  protonPath: "",
  umuAvailable: true,
  launcherVersion: "1.0.7.0924",
  launcherBuild: "1fba17f8",
  gameVersion: "1.4.16",
  gameExe: preview.gameExe,
  gameExists: true,
  sdkPath: preview.sdkPath,
  checks: [
    { label: "启动器配置", ok: true, detail: preview.launcherRoot },
    { label: "游戏可执行文件", ok: true, detail: preview.gameExe },
    { label: "umu-launcher", ok: true, detail: "umu-run（PATH）" },
    { label: "Proton", ok: false, detail: "未选择运行环境" },
    { label: "Wine prefix", ok: false, detail: "" },
    {
      label: "官方启动器",
      ok: true,
      detail: joinPath(preview.launcherRoot, distributions.cn.clientExe),
    },
  ],
  runtimeInstallations: [
    { distribution: "cn", installRoot: preview.installRoot, installed: true },
    {
      distribution: "global",
      installRoot: "~/Games/Neverness To Everness Global",
      installed: false,
    },
  ],
  settings: {
    gamepad: defaultGamepadOptions(),
    installRoot: null,
    globalInstallRoot: null,
    distribution: "cn",
    runtimeEnvironments: [],
    activeEnvironment: null,
    stableEnvironment: null,
    launchMode: "background",
    gameMode: {
      performance: false,
      inhibit: false,
      notifications: false,
      overlay: false,
      gamescope: { ...defaultGamescope },
    },
    updates: {
      autoDownload: false,
      metered: "ask",
      limitBytesPerSecond: 0,
      window: null,
    },
    language: "zh-CN",
    reduceMotion: false,
    stackBulletins: false,
    theme: "light",
    heroMedia: "video",
    minimizeToTrayOnClose: true,
    showLauncherAfterGameExit: true,
    snapshotSuccessMinutes: 1,
  },
  appVersion: APP_VERSION,
  os: previewOs(),
};

const previewSnapshotNow = Date.now();
let previewSnapshotAttempts = 2;
let previewSnapshotBaseline: SnapshotSummary | null = {
  id: "preview-baseline",
  capturedAt: previewSnapshotNow - 86_400_000,
  status: "success",
  durationMs: 42 * 60_000,
  failure: null,
  environment: null,
  proton: "GE-Proton9-20",
  distribution: "cn",
  mode: "background",
};
let previewSnapshotLatest: SnapshotSummary | null = {
  id: "preview-latest",
  capturedAt: previewSnapshotNow - 3_600_000,
  status: "failed",
  durationMs: 4_000,
  failure: { reason: "earlyExit", exitCode: 3, detail: null },
  environment: null,
  proton: "GE-Proton9-21",
  distribution: "cn",
  mode: "background",
};
let previewSnapshotReference: SnapshotReference | null = {
  label: "更新驱动前",
  savedAt: previewSnapshotNow - 172_800_000,
  capturedAt: previewSnapshotNow - 172_800_000,
  environment: null,
  proton: "GE-Proton9-20",
  distribution: "cn",
  mode: "background",
};

function previewSnapshotEnvironment() {
  const active = mock.settings.runtimeEnvironments.find(
    (environment) => environment.id === mock.settings.activeEnvironment,
  );
  return active?.name ?? null;
}

function previewSnapshotProton() {
  return mock.protonName || previewSnapshotBaseline?.proton || null;
}

function previewSnapshotOverview(): SnapshotOverview {
  return {
    baseline: previewSnapshotBaseline ? { ...previewSnapshotBaseline } : null,
    reference: previewSnapshotReference,
    latest: previewSnapshotLatest ? { ...previewSnapshotLatest } : null,
    attempts: previewSnapshotAttempts,
  };
}

function previewSnapshotComparison(
  target: "baseline" | "reference",
): SnapshotComparison {
  if (target === "reference" && !previewSnapshotReference)
    throw new Error("尚未保存参考快照");
  if (target === "baseline" && !previewSnapshotBaseline)
    throw new Error("还没有成功启动记录");
  const rows: SnapshotDiffRow[] = [
    {
      key: "launcher.version",
      section: "启动器",
      label: "Oddity 版本",
      baseline: "0.1.0-alpha.2",
      current: "0.1.0-alpha.2",
      status: "same",
    },
    {
      key: "system.session",
      section: "系统",
      label: "会话类型",
      baseline: "Wayland",
      current: "Wayland",
      status: "same",
    },
    {
      key: "graphics.gpu.10DE:2C58",
      section: "图形",
      label: "显卡 10DE:2C58",
      baseline: "nvidia 615.78.08",
      current: "nvidia 615.78.10",
      status: "changed",
    },
    {
      key: "graphics.opengl",
      section: "图形",
      label: "OpenGL 驱动",
      baseline: "NVIDIA GeForce RTX 5060 · 4.6.0 NVIDIA 615.78.08",
      current: "NVIDIA GeForce RTX 5060 · 4.6.0 NVIDIA 615.78.10",
      status: "changed",
    },
    {
      key: "graphics.vulkan",
      section: "图形",
      label: "Vulkan 驱动",
      baseline: null,
      current: "Mesa 26.2.5",
      status: "unknown",
    },
    {
      key: "game.version",
      section: "游戏",
      label: "游戏版本",
      baseline: "1.0.7",
      current: "1.0.7",
      status: "same",
    },
    {
      key: "runtime.proton",
      section: "运行环境",
      label: "Proton",
      baseline: "GE-Proton9-20",
      current: "GE-Proton9-21",
      status: "changed",
    },
    {
      key: "runtime.gamescope",
      section: "运行环境",
      label: "Gamescope",
      baseline: "关闭",
      current: "关闭",
      status: "same",
    },
  ];
  const targetTime =
    target === "reference"
      ? previewSnapshotReference!.capturedAt
      : previewSnapshotBaseline!.capturedAt;
  return {
    target,
    targetLabel: target === "reference" ? "参考快照" : "上次成功启动",
    targetTime,
    targetEnvironment: previewSnapshotEnvironment(),
    currentEnvironment: previewSnapshotEnvironment(),
    rows,
    changed: rows.filter(
      (row) =>
        row.status === "changed" ||
        row.status === "added" ||
        row.status === "removed",
    ).length,
    unknown: rows.filter((row) => row.status === "unknown").length,
    unchanged: rows.filter((row) => row.status === "same").length,
  };
}

function previewSnapshotSaveReference(label: string | null): SnapshotReference {
  const now = Date.now();
  previewSnapshotReference = {
    label: label?.trim() || "参考快照",
    savedAt: now,
    capturedAt: now,
    environment: previewSnapshotEnvironment(),
    proton: previewSnapshotProton(),
    distribution: mock.settings.distribution,
    mode: mock.settings.launchMode,
  };
  return previewSnapshotReference;
}

function applyPreviewDistribution() {
  if (mock.settings.distribution === "global")
    mock.installRoot =
      mock.settings.globalInstallRoot || "~/Games/Neverness To Everness Global";
  else mock.installRoot = mock.settings.installRoot || preview.installRoot;
  mock.launcherRoot = joinPath(
    mock.installRoot,
    distributions[mock.settings.distribution].launcherDir,
  );
  mock.sdkPath = joinPath(mock.launcherRoot, "WPGameClientSDK_64.dll");
  mock.gameExe = joinPath(
    mock.installRoot,
    "Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe",
  );
  for (const item of mock.checks) {
    if (item.label === "启动器配置") item.detail = mock.launcherRoot;
    if (item.label === "游戏可执行文件") item.detail = mock.gameExe;
    if (
      item.detail.endsWith("NTEGame.exe") ||
      item.detail.endsWith("NTEGlobalGame.exe")
    ) {
      item.detail = joinPath(
        mock.launcherRoot,
        distributions[mock.settings.distribution].clientExe,
      );
    }
  }
  mockCheck.distribution = mock.settings.distribution;
  mockCheck.installRoot = mock.installRoot;
}

function selectPreviewEnvironment(
  profile: Bootstrap["settings"]["runtimeEnvironments"][number],
) {
  mock.settings.distribution = profile.distribution;
  applyPreviewDistribution();
  mock.settings.activeEnvironment = profile.id;
  mock.prefix = joinPath(
    preview.prefix.replace(/\/prefix$/, ""),
    "environments",
    profile.id,
    "prefix",
  );
  mock.prefixExists = true;
  mock.protonPath = profile.proton.path;
  mock.protonName = mock.protonPath.split("/").pop() || preview.protonName;
  mock.ok = true;
  mock.error = null;
  mock.checks[3] = { label: "Proton", ok: true, detail: mock.protonName };
  mock.checks[4] = { label: "Wine prefix", ok: true, detail: mock.prefix };
}

const mockCheck: CheckResult = {
  installed: true,
  distribution: "cn",
  installRoot: preview.installRoot,
  remoteVersion: "1.0.7.0924",
  localVersion: "1.0.7.0924",
  productVersion: "1.0.7.0924_1",
  files: 667,
  ok: 667,
  missing: 0,
  changed: 0,
  bytes: 0,
  durationMs: 342,
  missingPreview: [],
  changedPreview: [],
};

const mockBulletins: BulletinFeed = {
  items: [
    {
      kind: "bilibili",
      title: "《异环》官方动态示例",
      summary: "浏览器预览使用本地示例数据，原生启动器会自动读取官方动态。",
      url: "https://space.bilibili.com/3546636978489848/dynamic",
      imageUrl: null,
      publishedAt: "刚刚",
      source: "B站官方动态",
    },
    {
      kind: "preview",
      title: "《异环》版本前瞻特别节目 | 情报回顾",
      summary: "版本前瞻回顾",
      url: distributions.cn.official,
      imageUrl: "/assets/hero-keyart.jpg",
      publishedAt: "",
      source: "版本前瞻",
    },
    {
      kind: "maintenance",
      title: "《异环》不停服更新公告",
      summary: "服务器维护时间以官方公告为准。",
      url: "https://yh.wanmei.com/news/gamebroad/index.html",
      imageUrl: null,
      publishedAt: "最新",
      source: "官方公告",
    },
  ],
  errors: [],
};

// 浏览器预览模拟会话, 原生窗口的状态由 Rust 维护。
const previewGame = {
  running: false,
  closing: false,
  pid: 0,
  startedAt: 0,
  exe: preview.gameExe,
  log: "",
  launch: null as LaunchProgress | null,
};
const previewGameListeners = new Set<(status: GameStatus) => void>();
let previewLaunchTimers: number[] = [];

function previewGameStatus() {
  return clone(previewGame);
}

function emitPreviewGameStatus() {
  const status = previewGameStatus();
  for (const handler of previewGameListeners) handler(status);
}

function clearPreviewLaunchTimers() {
  for (const timer of previewLaunchTimers) window.clearTimeout(timer);
  previewLaunchTimers = [];
}

function startPreviewLaunch() {
  clearPreviewLaunchTimers();
  const startedAt = Date.now();
  previewGame.running = true;
  previewGame.closing = false;
  previewGame.pid = 23140;
  previewGame.startedAt = startedAt;
  previewGame.launch = { stage: "preparing", startedAt, cancelling: false };
  emitPreviewGameStatus();
  const stages: { stage: LaunchStage; delay: number }[] = [
    { stage: "helper", delay: 600 },
    { stage: "resources", delay: 1300 },
    { stage: "game", delay: 3000 },
  ];
  for (const item of stages) {
    previewLaunchTimers.push(
      window.setTimeout(() => {
        if (!previewGame.launch || previewGame.launch.cancelling) return;
        previewGame.launch = { ...previewGame.launch, stage: item.stage };
        emitPreviewGameStatus();
      }, item.delay),
    );
  }
  previewLaunchTimers.push(
    window.setTimeout(() => {
      if (!previewGame.launch || previewGame.launch.cancelling) return;
      previewGame.launch = null;
      emitPreviewGameStatus();
    }, 3700),
  );
}

function previewSessions(): PlaytimeSession[] {
  const sessions: PlaytimeSession[] = [];
  const now = new Date();
  for (let offset = 11; offset >= 0; offset -= 1) {
    const month = new Date(now.getFullYear(), now.getMonth() - offset, 1);
    const count = 2 + ((offset * 3) % 4);
    for (let index = 0; index < count; index += 1) {
      const day = 3 + ((offset * 5 + index * 6) % 22);
      const startedAt = new Date(
        month.getFullYear(),
        month.getMonth(),
        day,
        19 + (index % 3),
        (index * 17) % 60,
      ).getTime();
      const endedAt =
        startedAt + (35 + ((offset * 11 + index * 23) % 150)) * 60_000;
      if (endedAt > Date.now()) continue;
      sessions.push({
        startedAt,
        endedAt,
        mode: index % 3 ? "background" : "official",
        distribution: "cn",
      });
    }
  }
  return sessions.sort((left, right) => left.startedAt - right.startedAt);
}

const previewPlaytime = previewSessions();

function previewHistory(): PlaytimeHistory {
  return {
    sessions: clone(previewPlaytime),
    active: previewGame.running
      ? {
          startedAt: previewGame.startedAt,
          lastSeenAt: Date.now(),
          mode: "background",
          distribution: "cn",
        }
      : null,
  };
}

const previewAttendance: AttendanceStatus = {
  accounts: [],
  preferences: {
    trigger: "off",
    time: "08:00",
    tasks: { ...defaultAttendanceTasks },
  },
};
const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function listenGameState(handler: (status: GameStatus) => void) {
  if (!isTauri()) {
    previewGameListeners.add(handler);
    return () => {
      previewGameListeners.delete(handler);
    };
  }
  let disposed = false;
  let unlisten: (() => void) | null = null;
  listen<GameStatus>("game-state", (event) => handler(event.payload))
    .then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    })
    .catch(() => {});
  return () => {
    disposed = true;
    if (unlisten) unlisten();
  };
}

/// 外部启动切换地区或运行环境后, 后端推送新的 bootstrap 让界面同步.
export function listenSettingsChanged(handler: (data: Bootstrap) => void) {
  if (!isTauri()) return () => {};
  let disposed = false;
  let unlisten: (() => void) | null = null;
  listen<Bootstrap>("settings-changed", (event) => handler(event.payload))
    .then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    })
    .catch(() => {});
  return () => {
    disposed = true;
    if (unlisten) unlisten();
  };
}

/// 后端创建游玩会话后通知前端补读记录, 避免 running 先于会话创建。
export function listenPlaytimeStarted(handler: () => void) {
  if (!isTauri()) return () => {};
  let disposed = false;
  let unlisten: (() => void) | null = null;
  listen("playtime-started", () => handler())
    .then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    })
    .catch(() => {});
  return () => {
    disposed = true;
    if (unlisten) unlisten();
  };
}

export function listenDownloadState(handler: (status: DownloadStatus) => void) {
  if (!isTauri()) return () => {};
  let disposed = false;
  let unlisten: (() => void) | null = null;
  listen<DownloadStatus>("game-download-state", (event) =>
    handler(event.payload),
  )
    .then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    })
    .catch(() => {});
  return () => {
    disposed = true;
    if (unlisten) unlisten();
  };
}

const mockDownload: DownloadStatus = {
  mode: "check",
  phase: "idle",
  origin: "manual",
  autoWaiting: "",
  checkedAt: 0,
  distribution: "cn",
  installRoot: preview.installRoot,
  installed: true,
  full: false,
  unverifiedFiles: 0,
  currentFile: "",
  currentFileBytes: 0,
  completedFiles: 0,
  totalFiles: 0,
  downloadedBytes: 0,
  totalBytes: 0,
  patchFiles: 0,
  patchBytes: 0,
  fullFiles: 0,
  fullBytes: 0,
  stage: "",
  remoteVersion: "",
  pendingFiles: 0,
  diskAvailableBytes: 256 * 1024 ** 3,
  diskRequiredBytes: 0,
  error: "",
};

export async function invoke<C extends keyof Commands>(
  command: C,
  ...params: CommandArgs[C] extends undefined
    ? [args?: undefined]
    : [args: CommandArgs[C]]
): Promise<Commands[C]> {
  if (isTauri()) return tauriInvoke<Commands[C]>(command, params[0]);
  return previewInvoke(command, params[0]) as Promise<Commands[C]>;
}

async function previewInvoke(
  command: keyof Commands,
  args?: CommandArgs[keyof CommandArgs],
): Promise<Commands[keyof Commands]> {
  switch (command) {
    case "attendance_status":
      return clone(previewAttendance);
    case "attendance_refresh_game_records":
      previewAttendance.accounts.forEach((account, index) => {
        account.gameRecord = {
          roleId: String(100000001 + index),
          roleName: index ? `预览角色 ${index + 1}` : "海特洛调查员",
          serverName: "预览服务器",
          stamina: index ? 240 : 168,
          staminaMax: 240,
          cityStamina: index ? 180 : 420,
          cityStaminaMax: 700,
          activity: index ? 100 : 60,
          weeklyRemaining: index ? 0 : 2,
          updatedAt: Math.floor(Date.now() / 1000),
        };
        account.gameRecordError = null;
      });
      return clone(previewAttendance);
    case "attendance_keyring_check":
      return undefined;
    case "attendance_send_code": {
      const { phone } = args as CommandArgs["attendance_send_code"];
      if (!/^[0-9]{11}$/.test(phone)) throw new Error("请输入有效的手机号码");
      return "preview-device-" + phone;
    }
    case "attendance_login_sms": {
      const { phone, code, deviceId } =
        args as CommandArgs["attendance_login_sms"];
      if (!/^[0-9]{11}$/.test(phone)) throw new Error("请输入有效的手机号码");
      if (!/^[0-9]{4,8}$/.test(code))
        throw new Error("请输入 4-8 位短信验证码");
      if (deviceId !== "preview-device-" + phone)
        throw new Error("请先为当前手机号获取验证码.");
      return previewInvoke("attendance_login", {
        phone,
        password: "preview",
        rememberPassword: false,
      });
    }
    case "attendance_login": {
      const login = args as CommandArgs["attendance_login"];
      if (!/^[0-9]{11}$/.test(login.phone) || !login.password)
        throw new Error("请输入手机号码和密码");
      let hash = 2166136261;
      for (const char of login.phone)
        hash = Math.imul(hash ^ char.charCodeAt(0), 16777619);
      const account = {
        id: (hash >>> 0).toString(16),
        maskedPhone: login.phone.slice(0, 3) + "****" + login.phone.slice(7),
        rememberPassword: login.rememberPassword,
        signedToday: false,
        failedToday: false,
        lastReport: null,
        attention: null,
      };
      const index = previewAttendance.accounts.findIndex(
        (item) => item.id === account.id,
      );
      if (index < 0) previewAttendance.accounts.push(account);
      else previewAttendance.accounts[index] = account;
      return clone(previewAttendance);
    }
    case "attendance_configure": {
      const preferences = (args as { preferences: AttendancePreferences })
        .preferences;
      if (preferences.trigger !== "off" && !previewAttendance.accounts.length)
        throw new Error("请先登录塔吉多账号");
      previewAttendance.preferences = clone(preferences);
      return clone(previewAttendance);
    }
    case "attendance_run":
      if (!previewAttendance.accounts.length)
        throw new Error("请先登录塔吉多账号");
      if (!Object.values(previewAttendance.preferences.tasks).some(Boolean))
        return clone(previewAttendance);
      for (const account of previewAttendance.accounts) {
        if (["auth", "verification"].includes(account.attention?.kind ?? ""))
          continue;
        account.signedToday = true;
        account.failedToday = false;
        account.lastReport = {
          at: Math.floor(Date.now() / 1000),
          ok: true,
          steps: [
            {
              label: "社区签到",
              ok: true,
              message: "模拟签到完成, 未发送真实请求",
              kind: null,
            },
            ...[
              "角色 预览角色",
              "BBS 金币签到",
              "浏览帖子",
              "点赞帖子",
              "分享帖子",
            ].map((label) => ({
              label,
              ok: true,
              message: "模拟任务完成, 未发送真实请求",
              kind: null,
              optional: label !== "角色 预览角色",
            })),
            {
              label: "今日金币",
              ok: true,
              message: "模拟今日金币 110/150, 未发送真实请求",
              kind: null,
              optional: true,
            },
            {
              label: "云异环时长",
              ok: true,
              message: "模拟本次赠送 15 分钟, 剩余 120 分钟, 未发送真实请求",
              kind: null,
              optional: true,
            },
          ].filter((step) => {
            const tasks = previewAttendance.preferences.tasks;
            if (step.label === "社区签到" || step.label === "BBS 金币签到")
              return tasks.communitySign;
            if (step.label.startsWith("角色 ")) return tasks.roleSign;
            if (step.label === "浏览帖子") return tasks.browsePosts;
            if (step.label === "点赞帖子") return tasks.likePosts;
            if (step.label === "分享帖子") return tasks.sharePosts;
            if (step.label === "云异环时长") return tasks.cloudDuration;
            return (
              tasks.communitySign ||
              tasks.browsePosts ||
              tasks.likePosts ||
              tasks.sharePosts
            );
          }),
        };
      }
      return clone(previewAttendance);
    case "attendance_logout":
      previewAttendance.accounts = previewAttendance.accounts.filter(
        (account) =>
          account.id !== (args as CommandArgs["attendance_logout"]).accountId,
      );
      if (!previewAttendance.accounts.length)
        previewAttendance.preferences = {
          trigger: "off",
          time: "08:00",
          tasks: { ...defaultAttendanceTasks },
        };
      return clone(previewAttendance);
    case "launcher_background":
      return { video: null, image: null };
    case "bootstrap":
      return clone(mock);
    case "environment_create": {
      const { name, protonPath, sourceId, distribution } =
        args as CommandArgs["environment_create"];
      const source = mock.settings.runtimeEnvironments.find(
        (profile) => profile.id === sourceId,
      );
      if (sourceId && !source) throw new Error("源运行环境不存在");
      const target = mock.runtimeInstallations.find(
        (entry) => entry.distribution === distribution,
      );
      if (!target?.installed) throw new Error("尚未安装所选区服, 请先安装游戏");
      if (!name.trim() || !protonPath.trim())
        throw new Error("请填写环境名称和 Proton 目录");
      if (
        mock.settings.runtimeEnvironments.some(
          (profile) => profile.name === name.trim(),
        )
      )
        throw new Error("运行环境名称已存在");
      const profile = {
        id: String(mock.settings.runtimeEnvironments.length + 1).padStart(
          32,
          "0",
        ),
        name: name.trim(),
        proton: { path: protonPath.trim(), identity: "browser-preview" },
        installRoot: target.installRoot,
        distribution,
        createdAt: Math.floor(Date.now() / 1000),
      };
      mock.settings.runtimeEnvironments.push(profile);
      if (!mock.settings.activeEnvironment) {
        mock.settings.stableEnvironment = profile.id;
        selectPreviewEnvironment(profile);
      }
      return clone(mock);
    }
    case "environment_action": {
      const { action, id } = args as CommandArgs["environment_action"];
      const profile = mock.settings.runtimeEnvironments.find(
        (entry) => entry.id === id,
      );
      if (!profile) throw new Error("运行环境不存在");
      if (action === "switch") {
        const target = mock.runtimeInstallations.find(
          (entry) => entry.distribution === profile.distribution,
        );
        if (target?.installRoot !== profile.installRoot)
          throw new Error("运行环境与绑定的游戏目录不匹配");
        selectPreviewEnvironment(profile);
      } else if (action === "keep") {
        mock.settings.stableEnvironment = id;
      } else {
        if (
          !id ||
          id === mock.settings.activeEnvironment ||
          id === mock.settings.stableEnvironment
        )
          throw new Error("不能删除当前环境或稳定环境");
        mock.settings.runtimeEnvironments =
          mock.settings.runtimeEnvironments.filter((entry) => entry.id !== id);
      }
      return clone(mock);
    }
    case "check_update":
      await new Promise((resolve) => setTimeout(resolve, 620));
      return clone(mockCheck);
    case "fetch_bulletins":
      await new Promise((resolve) => setTimeout(resolve, 260));
      if ((args as CommandArgs["fetch_bulletins"]).distribution === "global") {
        return {
          items: [
            {
              kind: "preview",
              title: "国际服官方新闻",
              summary: "浏览器预览数据",
              url: distributions.global.official,
              imageUrl: null,
              publishedAt: "",
              source: "国际服官网",
            },
          ],
          errors: [],
        };
      }
      return clone(mockBulletins);
    case "save_settings": {
      const settings = (args as { settings: Settings }).settings;
      mock.settings = { ...mock.settings, ...settings };
      mock.runtimeInstallations = [
        {
          distribution: "cn",
          installRoot: mock.settings.installRoot || preview.installRoot,
          installed: true,
        },
        {
          distribution: "global",
          installRoot:
            mock.settings.globalInstallRoot ||
            "~/Games/Neverness To Everness Global",
          installed: Boolean(mock.settings.globalInstallRoot),
        },
      ];
      applyPreviewDistribution();
      return clone(mock);
    }
    case "gamepad_poll":
      return {
        idle: { requested: false, error: null },
        devices: [],
        device: null,
        buttons: [],
        pressed: [],
        axes: [0, 0, 0, 0, 0, 0],
        focused: false,
        error: null,
      };
    case "gamepad_release_idle":
    case "gamepad_feedback":
      return null;
    case "browse_files":
      throw new Error("请在 Oddity 桌面应用中浏览文件.");
    case "gamepad_profile_validate":
      throw new Error("请在 Oddity 桌面应用中导入反馈配置.");
    case "gamepad_profile_load":
    case "gamepad_profile_save":
      throw new Error("请在 Oddity 桌面应用中导入或导出反馈配置.");
    case "game_download_status":
      return clone(mockDownload);
    case "game_download_start": {
      const { mode, full } = args as CommandArgs["game_download_start"];
      // 预览资源大小一致, 完整校验才能发现模拟的内容损坏。
      if (mode === "check" && !full) {
        Object.assign(mockDownload, {
          mode,
          phase: "ready",
          installed: true,
          full: false,
          unverifiedFiles: 0,
          currentFile: "",
          currentFileBytes: 0,
          completedFiles: 0,
          totalFiles: 0,
          downloadedBytes: 0,
          totalBytes: 0,
          patchFiles: 0,
          patchBytes: 0,
          fullFiles: 0,
          fullBytes: 0,
          stage: "",
          remoteVersion: "",
          pendingFiles: 0,
          diskRequiredBytes: 0,
          error: "",
          distribution: mock.settings.distribution,
          installRoot: mock.installRoot,
        });
        return clone(mockDownload);
      }
      Object.assign(mockDownload, {
        mode,
        full: full || (mockDownload.full && mockDownload.phase !== "ready"),
        unverifiedFiles: 0,
        phase: mode === "check" ? "needs-update" : "game",
        distribution: mock.settings.distribution,
        installRoot: mock.installRoot,
        currentFile:
          mode === "check" ? "" : "Client/WindowsNoEditor/example.pak",
        currentFileBytes: mode === "check" ? 0 : 6_000_000,
        completedFiles: 0,
        totalFiles: 2,
        downloadedBytes: 0,
        totalBytes: 12_000_000,
        patchFiles: 1,
        patchBytes: 10_000_000,
        fullFiles: 1,
        fullBytes: 2_000_000,
        stage: mode === "check" ? "" : "download",
        remoteVersion: "1.4.17",
        pendingFiles: 2,
        diskRequiredBytes: 18_000_000,
        error: "",
      });
      if (mode === "apply") {
        window.setTimeout(
          () =>
            Object.assign(mockDownload, {
              phase: "ready",
              completedFiles: 2,
              downloadedBytes: 12_000_000,
              pendingFiles: 0,
              currentFile: "",
              currentFileBytes: 0,
              stage: "",
            }),
          1400,
        );
      }
      return clone(mockDownload);
    }
    case "game_download_cancel":
      mockDownload.phase = "cancelled";
      return clone(mockDownload);
    case "environment_snapshot_overview":
      return clone(previewSnapshotOverview());
    case "environment_snapshot_compare": {
      const { target } = args as CommandArgs["environment_snapshot_compare"];
      return clone(previewSnapshotComparison(target));
    }
    case "environment_snapshot_save_reference": {
      const { label } =
        args as CommandArgs["environment_snapshot_save_reference"];
      return clone(previewSnapshotSaveReference(label));
    }
    case "environment_snapshot_delete_reference":
      previewSnapshotReference = null;
      return null;
    case "environment_snapshot_clear_history":
      previewSnapshotBaseline = null;
      previewSnapshotLatest = null;
      previewSnapshotAttempts = 0;
      return null;
    case "open_url":
      return null;
    case "prepare_log_export":
    case "save_log_export":
    case "discard_log_export":
    case "open_directory":
      throw new Error("请在 Oddity 桌面应用中导出日志");
    case "launch_official":
      if (!mock.settings.activeEnvironment)
        throw new Error("请先创建 Proton 运行环境");
      startPreviewLaunch();
      return {
        mode: "background",
        pid: previewGame.pid,
        exe: previewGame.exe,
        prefix: mock.prefix,
        log: "",
        startedAt: previewGame.startedAt,
      };
    case "cancel_launch":
      clearPreviewLaunchTimers();
      if (previewGame.launch) {
        previewGame.launch = {
          ...previewGame.launch,
          cancelling: true,
        };
        emitPreviewGameStatus();
      }
      await delay(450);
      previewGame.running = false;
      previewGame.closing = false;
      previewGame.pid = 0;
      previewGame.launch = null;
      emitPreviewGameStatus();
      return previewGameStatus();
    case "game_mode_tuning_info":
      return { pinCores: null, parkCores: null, renice: null, ioprio: null };
    case "game_mode_tuning_apply":
      throw new Error("请在原生启动器中应用 GameMode 配置");
    case "game_mode_gpu_info":
      return {
        config: {
          enabled: false,
          device: 0,
          vendor: "nvidia",
          coreOffset: -1,
          memoryOffset: -1,
          powerMode: -1,
          amdLevel: "auto",
        },
        devices: [],
      };
    case "game_mode_gpu_apply":
      throw new Error("请在原生启动器中应用 GPU 配置");
    case "game_mode_status":
      return {
        capabilities: (
          [
            "performance",
            "inhibit",
            "notifications",
            "overlay",
            "gamescope",
          ] as const
        ).map((id) => ({
          id,
          available: false,
          state: "unavailable" as const,
          detail: "浏览器预览无法检测系统能力, 请在原生启动器中检测",
        })),
        session: null,
      };
    case "game_status":
      return previewGameStatus();
    case "take_launch_error":
      return null;
    case "playtime_history":
      return previewHistory();
    case "playtime_export_poster": {
      const { destination } = args as CommandArgs["playtime_export_poster"];
      return destination;
    }
    case "close_game":
      clearPreviewLaunchTimers();
      previewGame.closing = true;
      previewGame.launch = null;
      emitPreviewGameStatus();
      await delay(850);
      previewGame.running = false;
      previewGame.closing = false;
      previewGame.pid = 0;
      previewPlaytime.push({
        startedAt: Date.now() - 90 * 60_000,
        endedAt: Date.now(),
        mode: "background",
        distribution: "cn",
      });
      emitPreviewGameStatus();
      return previewGameStatus();
    case "win_minimize":
    case "win_toggle_maximize":
    case "win_close":
    case "win_hide":
    case "win_drag":
      return null;
    default:
      throw new Error("unknown command: " + command);
  }
}
