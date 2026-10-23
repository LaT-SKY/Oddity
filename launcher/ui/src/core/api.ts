import { APP_VERSION } from "./version";
import { invoke as tauriInvoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Bootstrap,
  Settings,
  GameStatus,
  DownloadStatus,
  Commands,
  CommandArgs,
  PreviewPaths,
  BulletinFeed,
  AttendanceStatus,
  AttendancePreferences,
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
    launcherRoot: joinPath(installRoot, "NTELauncher"),
    bridgeRoot: joinPath(dataHome, "oddity/winroot"),
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
    sdkPath: joinPath(installRoot, "NTELauncher/WPGameClientSDK_64.dll"),
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
  ok: true,
  error: null,
  installRoot: preview.installRoot,
  launcherRoot: preview.launcherRoot,
  bridgeRoot: preview.bridgeRoot,
  bridgeReady: true,
  prefix: preview.prefix,
  prefixExists: true,
  protonName: preview.protonName,
  protonPath: preview.protonPath,
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
    { label: "Proton", ok: true, detail: preview.protonName },
    { label: "Wine prefix", ok: true, detail: preview.prefix },
    { label: "官方启动器桥接", ok: true, detail: preview.bridgeRoot },
  ],
  settings: {
    installRoot: null,
    globalInstallRoot: null,
    distribution: "cn",
    bridgeRoot: null,
    prefix: null,
    protonPath: null,
    launchMode: "background",
    language: "zh-CN",
    reduceMotion: false,
    theme: "light",
    heroMedia: "video",
    minimizeToTrayOnClose: true,
  },
  appVersion: APP_VERSION,
  os: previewOs(),
};

const mockCheck = {
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
      url: "https://yh.wanmei.com/launcher/launcher_ob2.html?expand=0",
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
};

function previewGameStatus() {
  return clone(previewGame);
}

const previewAttendance: AttendanceStatus = {
  accounts: [],
  preferences: { trigger: "off", time: "08:00" },
};
const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function listenGameState(handler: (status: GameStatus) => void) {
  if (!isTauri()) return () => {};
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
  phase: "idle",
  distribution: "cn",
  installRoot: preview.installRoot,
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
      for (const account of previewAttendance.accounts) {
        if (
          account.signedToday ||
          ["auth", "verification"].includes(account.attention?.kind ?? "")
        )
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
          ],
        };
      }
      return clone(previewAttendance);
    case "attendance_logout":
      previewAttendance.accounts = previewAttendance.accounts.filter(
        (account) =>
          account.id !== (args as CommandArgs["attendance_logout"]).accountId,
      );
      if (!previewAttendance.accounts.length)
        previewAttendance.preferences = { trigger: "off", time: "08:00" };
      return clone(previewAttendance);
    case "bootstrap":
      return clone(mock);
    case "check_update":
      await new Promise((resolve) => setTimeout(resolve, 620));
      return clone(mockCheck);
    case "fetch_bulletins":
      await new Promise((resolve) => setTimeout(resolve, 260));
      return clone(mockBulletins);
    case "save_settings": {
      const settings = (args as { settings: Settings }).settings;
      mock.settings = { ...mock.settings, ...settings };
      if (settings.distribution === "global")
        mock.installRoot =
          settings.globalInstallRoot || "~/Games/Neverness To Everness Global";
      else if (settings.installRoot) mock.installRoot = settings.installRoot;
      mock.launcherRoot = joinPath(
        mock.installRoot,
        settings.distribution === "global" ? "NTEGlobal" : "NTELauncher",
      );
      return clone(mock);
    }
    case "game_download_status":
      return clone(mockDownload);
    case "game_download_start": {
      const { mode } = args as CommandArgs["game_download_start"];
      Object.assign(mockDownload, {
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
    case "open_url":
      return null;
    case "launch_official":
      previewGame.running = true;
      previewGame.closing = false;
      previewGame.pid = 23140;
      previewGame.startedAt = Date.now();
      return {
        mode: "official",
        pid: previewGame.pid,
        exe: previewGame.exe,
        prefix: mock.prefix,
        log: "",
        startedAt: previewGame.startedAt,
      };
    case "game_status":
      return previewGameStatus();
    case "close_game":
      previewGame.closing = true;
      await delay(850);
      previewGame.running = false;
      previewGame.closing = false;
      previewGame.pid = 0;
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
