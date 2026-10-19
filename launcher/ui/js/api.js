// Thin bridge between the static UI and the Tauri host.
// In a browser preview it returns realistic mock data so the design can be
// iterated without rebuilding the Rust shell.

export const isTauri = () => Boolean(window.__TAURI__ && window.__TAURI__.core);

const clone = (value) => JSON.parse(JSON.stringify(value));

// Browser-preview paths. The Tauri shell always supplies the real values from
// Rust (`bootstrap`); the mock only needs a believable layout for design work.
// Resolve it at runtime instead of baking one machine's paths into the source.
// Precedence: URL query > localStorage > window.__NTE_PREVIEW__ > defaults.
const PREVIEW_KEYS = ["home", "dataHome", "steamRoot", "installRoot", "protonName"];

function readStoredPreview() {
  try {
    return JSON.parse(window.localStorage.getItem("nte.preview.paths") || "{}") || {};
  } catch {
    return {};
  }
}

function readQueryPreview() {
  const params = new URLSearchParams(window.location.search);
  const query = {};
  for (const key of PREVIEW_KEYS) {
    const value = params.get(key);
    if (value && value.trim()) query[key] = value.trim();
  }
  return query;
}

function previewOverrides() {
  const sources = [window.__NTE_PREVIEW__, readStoredPreview(), readQueryPreview()];
  const overrides = {};
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

const joinPath = (...parts) =>
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
  const installRoot = overrides.installRoot || joinPath(home, "Games/Neverness To Everness");
  const protonName = overrides.protonName || "GE-Proton11-5-x86_64";
  return {
    installRoot,
    launcherRoot: joinPath(installRoot, "NTELauncher"),
    bridgeRoot: joinPath(dataHome, "oddity/winroot"),
    prefix: joinPath(dataHome, "oddity/prefix"),
    protonName,
    protonPath: joinPath(steamRoot, "compatibilitytools.d", protonName, "proton"),
    gameExe: joinPath(installRoot, "Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe"),
    sdkPath: joinPath(installRoot, "NTELauncher/WPGameClientSDK_64.dll"),
  };
}

function previewOs() {
  const platform = navigator.userAgentData?.platform || navigator.platform || "";
  const family = /win/i.test(platform) ? "windows" : /mac/i.test(platform) ? "macos" : "linux";
  const arch = /arm64|aarch64/i.test(navigator.userAgent) ? "aarch64" : "x86_64";
  return `${family} / ${arch} / browser preview`;
}

const preview = previewPaths();

const mock = {
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
  },
  appVersion: "0.1.0",
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

// Browser-preview-only game session. The real state lives in the Rust shell;
// this lets the launch/close button state machine be designed and screenshotted
// without Proton.
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

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Subscribe to backend game-session changes. Returns an unsubscribe function so
 * callers can release the listener if the UI is torn down.
 */
export function listenGameState(handler) {
  if (!isTauri()) return () => {};
  const listen = window.__TAURI__?.event?.listen;
  if (typeof listen !== "function") return () => {};
  let disposed = false;
  let unlisten = null;
  listen("game-state", (event) => handler(event.payload)).then((stop) => {
    if (disposed) stop();
    else unlisten = stop;
  }).catch(() => {});
  return () => {
    disposed = true;
    if (unlisten) unlisten();
  };
}

export async function invoke(command, args = {}) {
  if (isTauri()) {
    return window.__TAURI__.core.invoke(command, args);
  }

  switch (command) {
    case "bootstrap":
      return clone(mock);
    case "check_update":
      await new Promise((resolve) => setTimeout(resolve, 620));
      return clone(mockCheck);
    case "save_settings": {
      mock.settings = { ...mock.settings, ...(args.settings || {}) };
      if (args.settings && args.settings.installRoot) mock.installRoot = args.settings.installRoot;
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
