import { APP_VERSION } from "./version";
import { invoke as tauriInvoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Bootstrap,
  Settings,
  GameStatus,
  Commands,
  CommandArgs,
  PreviewPaths,
  BulletinFeed,
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
      if (settings.installRoot) mock.installRoot = settings.installRoot;
      return clone(mock);
    }
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
