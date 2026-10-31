import {
  app,
  BrowserWindow,
  dialog,
  ipcMain,
  Menu,
  net,
  protocol,
  Tray,
} from "electron";
import { mkdirSync, createWriteStream } from "node:fs";
import { access, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { Backend } from "./backend";
import { commands } from "./commands";
import type { DialogOptions } from "../src/core/desktop";
import type { Bootstrap, LauncherBackgroundMedia } from "../src/types/launcher";

const args = process.argv.slice(app.isPackaged ? 1 : 2);
const compatible = args.includes("--recovery-compatible");
const hiddenLaunch = args.includes("--launch");
const root = resolve(__dirname, "..");
const bundled = app.isPackaged || Boolean(process.env.ODDITY_RESOURCES);
const resourceRoot =
  process.env.ODDITY_RESOURCES ||
  (app.isPackaged ? process.resourcesPath : resolve(root, ".."));
const binRoot = bundled
  ? join(resourceRoot, "bin")
  : resolve(root, "../target/debug");
const backendPath =
  process.env.ODDITY_BACKEND || join(binRoot, "oddity-backend");
const uiRoot = join(root, "dist");
const profileRoot = process.env.XDG_CONFIG_HOME || join(homedir(), ".config");
app.setName("Oddity");
Menu.setApplicationMenu(null);
app.setDesktopName("oddity.desktop");
app.setPath("userData", join(profileRoot, "oddity", "chromium"));
if (compatible || process.env.NTE_SOFTWARE_RENDER)
  app.disableHardwareAcceleration();
protocol.registerSchemesAsPrivileged([
  {
    scheme: "oddity",
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      corsEnabled: true,
      stream: true,
      codeCache: true,
    },
  },
]);
let main: BrowserWindow | undefined;
let tray: Tray | undefined;
let backend: Backend;
let quitting = false;
// Wayland 不可靠回报最小化, 结合本窗口请求和恢复事件维护可见性.
let minimized = false;
let settings: Bootstrap["settings"] | undefined;
let mediaRevision = 0;
let media: LauncherBackgroundMedia = { video: null, image: null };
const knownCommands = new Set<string>(Object.keys(commands));

function authorized(event: Electron.IpcMainInvokeEvent) {
  if (
    !main ||
    event.sender !== main.webContents ||
    event.senderFrame !== main.webContents.mainFrame ||
    !event.senderFrame.url.startsWith("oddity://app/")
  )
    throw new Error("无效的窗口请求");
}
function windowEvent() {
  if (!main || main.isDestroyed()) return;
  main.webContents.send("oddity:event", "window-state", {
    maximized: main.isMaximized(),
    visible: main.isVisible() && !minimized && !main.isMinimized(),
  });
  void backend
    .call("host_state", {
      focused: main.isFocused(),
      visible: main.isVisible() && !minimized && !main.isMinimized(),
    })
    .catch((error) => console.error(error));
}
async function showMain() {
  if (!main || main.isDestroyed()) {
    main = new BrowserWindow({
      title: "Oddity",
      width: 1180,
      height: 760,
      minWidth: 980,
      minHeight: 640,
      frame: false,
      transparent: !compatible,
      resizable: true,
      show: false,
      backgroundColor: compatible ? "#101820" : "#00000000",
      icon: join(
        resourceRoot,
        bundled ? "icons/256x256.png" : "crates/nte-service/icons/256x256.png",
      ),
      webPreferences: {
        preload: join(__dirname, "preload.cjs"),
        sandbox: true,
        contextIsolation: true,
        nodeIntegration: false,
        spellcheck: false,
        additionalArguments: [
          `--oddity-appearance=${JSON.stringify({ theme: settings!.theme, reduceMotion: settings!.reduceMotion })}`,
        ],
      },
    });
    main.setMenu(null);
    main.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
    main.webContents.on("will-navigate", (event, url) => {
      if (!url.startsWith("oddity://app/")) event.preventDefault();
    });
    main.on("close", (event) => {
      if (!quitting && settings?.minimizeToTrayOnClose !== false && tray) {
        event.preventDefault();
        main!.hide();
      } else if (!quitting) app.quit();
    });
    main.on("maximize", () => {
      minimized = false;
      windowEvent();
    });
    main.on("unmaximize", windowEvent);
    main.on("resize", windowEvent);
    main.on("focus", () => {
      minimized = false;
      windowEvent();
    });
    main.on("blur", windowEvent);
    main.on("show", () => {
      minimized = false;
      windowEvent();
    });
    main.on("hide", windowEvent);
    main.on("minimize", () => {
      minimized = true;
      windowEvent();
    });
    main.on("restore", () => {
      minimized = false;
      windowEvent();
    });
    await main.loadURL("oddity://app/index.html");
  }
  minimized = false;
  main.show();
  main.restore();
  main.focus();
  windowEvent();
}

async function start() {
  const logs = join(
    process.env.XDG_DATA_HOME || join(homedir(), ".local/share"),
    "oddity",
    "logs",
  );
  mkdirSync(logs, { recursive: true, mode: 0o700 });
  const log = createWriteStream(
    join(logs, `oddity-startup-${Date.now()}-${process.pid}.log`),
    { mode: 0o600 },
  );
  const stderr = process.stderr.write.bind(process.stderr);
  process.stderr.write = ((chunk: string | Uint8Array, ...rest: unknown[]) => {
    log.write(chunk);
    return Reflect.apply(stderr, process.stderr, [chunk, ...rest]);
  }) as typeof process.stderr.write;
  await access(backendPath);
  backend = new Backend(backendPath, compatible);
  backend.on("exit", () => {
    if (!quitting) app.quit();
  });
  backend.on("event", (name: string, payload: unknown) => {
    if (name === "host-show") {
      void showMain().catch(fatal);
      return;
    }
    if (name === "host-hide") {
      main?.hide();
      return;
    }
    if (name === "host-exit") {
      app.exit(typeof payload === "number" ? payload : 1);
      return;
    }
    if (name === "settings-changed") settings = (payload as Bootstrap).settings;
    if (main && !main.isDestroyed())
      main.webContents.send("oddity:event", name, payload);
  });
  const bootstrap = (await backend.call("bootstrap")) as Bootstrap;
  settings = bootstrap.settings;
  protocol.handle("oddity", async (request) => {
    if (request.method !== "GET" && request.method !== "HEAD")
      return new Response(null, { status: 405 });
    const url = new URL(request.url);
    let file: string;
    if (url.hostname === "app") {
      file = resolve(uiRoot, "." + decodeURIComponent(url.pathname));
      if (!file.startsWith(uiRoot + sep))
        return new Response(null, { status: 403 });
    } else if (url.hostname === "media") {
      const candidate =
        url.pathname === "/video"
          ? media.video
          : url.pathname === "/image"
            ? media.image
            : null;
      if (!candidate) return new Response(null, { status: 404 });
      file = candidate;
    } else return new Response(null, { status: 404 });
    const info = await stat(file).catch(() => null);
    if (!info?.isFile()) return new Response(null, { status: 404 });
    // file: 提供分段流, HTTP 范围状态与长度由宿主补齐.
    const requestHeaders = new Headers(request.headers);
    const range = requestHeaders.get("Range");
    let start = 0,
      end = info.size - 1;
    if (range) {
      const match = /^bytes=(\d*)-(\d*)$/.exec(range);
      if (!match || (!match[1] && !match[2]))
        return new Response(null, { status: 416 });
      start = match[1]
        ? Number(match[1])
        : Math.max(0, info.size - Number(match[2]));
      end = match[1] && match[2] ? Math.min(Number(match[2]), end) : end;
      if (
        !Number.isSafeInteger(start) ||
        !Number.isSafeInteger(end) ||
        start > end
      )
        return new Response(null, {
          status: 416,
          headers: { "Content-Range": `bytes */${info.size}` },
        });
      requestHeaders.set("Range", `bytes=${start}-${end}`);
    }
    const response = await net.fetch(pathToFileURL(file).href, {
      method: request.method,
      headers: requestHeaders,
    });
    const headers = new Headers(response.headers);
    headers.set("Access-Control-Allow-Origin", "oddity://app");
    headers.set("Accept-Ranges", "bytes");
    headers.set("Content-Length", String(end - start + 1));
    if (range)
      headers.set("Content-Range", `bytes ${start}-${end}/${info.size}`);
    return new Response(response.body, {
      status: range ? 206 : response.status,
      statusText: response.statusText,
      headers,
    });
  });
  const invoke = async (
    event: Electron.IpcMainInvokeEvent,
    command: unknown,
    input: unknown,
  ) => {
    authorized(event);
    if (typeof command !== "string" || !knownCommands.has(command))
      throw new Error("无法识别的界面命令");
    if (command === "win_minimize") {
      minimized = true;
      main!.minimize();
      windowEvent();
      return null;
    }
    if (command === "win_toggle_maximize") {
      if (main!.isMaximized()) main!.unmaximize();
      else main!.maximize();
      return null;
    }
    if (command === "win_close") {
      main!.close();
      return null;
    }
    if (command === "win_hide") {
      main!.hide();
      return null;
    }
    const result = await backend.call(command, input);
    if (
      command === "bootstrap" ||
      command === "save_settings" ||
      command === "environment_create" ||
      command === "environment_action"
    )
      settings = (result as Bootstrap).settings;
    if (command === "launcher_background") {
      media = result as LauncherBackgroundMedia;
      mediaRevision++;
      return {
        video: media.video ? `oddity://media/video?v=${mediaRevision}` : null,
        image: media.image ? `oddity://media/image?v=${mediaRevision}` : null,
      };
    }
    return result;
  };
  ipcMain.handle("oddity:invoke", (event, command: unknown, input: unknown) =>
    invoke(event, command, input).then(
      (result) => ({ result }),
      (error) => ({ error: error instanceof Error ? error.message : error }),
    ),
  );
  ipcMain.handle(
    "oddity:dialog",
    async (event, kind: unknown, input: unknown) => {
      authorized(event);
      if (kind !== "open" && kind !== "save")
        throw new Error("无效的文件选择操作");
      if (!input || typeof input !== "object")
        throw new Error("无效的文件选择参数");
      const options = input as DialogOptions;
      if (
        options.directory !== undefined &&
        typeof options.directory !== "boolean"
      )
        throw new Error("无效的目录选择参数");
      if (
        options.filters !== undefined &&
        (!Array.isArray(options.filters) ||
          options.filters.some(
            (filter) =>
              !filter ||
              typeof filter.name !== "string" ||
              !Array.isArray(filter.extensions) ||
              filter.extensions.some(
                (extension) => typeof extension !== "string",
              ),
          ))
      )
        throw new Error("无效的文件类型参数");

      if (
        (options.title !== undefined && typeof options.title !== "string") ||
        (options.defaultPath !== undefined &&
          typeof options.defaultPath !== "string")
      )
        throw new Error("无效的文件选择参数");
      const common = {
        title: options.title,
        defaultPath: options.defaultPath,
        filters: options.filters,
      };
      if (kind === "save") {
        const result = await dialog.showSaveDialog(main!, common);
        return result.canceled ? null : (result.filePath ?? null);
      }
      const result = await dialog.showOpenDialog(main!, {
        ...common,
        properties: [options.directory ? "openDirectory" : "openFile"],
      });
      return result.canceled ? null : (result.filePaths[0] ?? null);
    },
  );
  ipcMain.handle("oddity:home", (event) => {
    authorized(event);
    return homedir();
  });
  ipcMain.handle("oddity:window-state", (event) => {
    authorized(event);
    return {
      maximized: main?.isMaximized() ?? false,
      visible: Boolean(main?.isVisible() && !minimized && !main.isMinimized()),
    };
  });
  const icon = join(
    resourceRoot,
    bundled ? "icons/tray-64.png" : "crates/nte-service/icons/tray-64.png",
  );
  tray = new Tray(icon);
  tray.setToolTip("Oddity");
  tray.setContextMenu(
    Menu.buildFromTemplate([
      {
        label: "显示主界面",
        click: () => {
          void showMain().catch(fatal);
        },
      },
      { label: "隐藏到后台", click: () => main?.hide() },
      { type: "separator" },
      { label: "退出 Oddity", click: () => app.quit() },
    ]),
  );
  if (hiddenLaunch)
    await backend.call("external_launch", { argv: args, cold: true });
  else await showMain();
}
function fatal(error: unknown) {
  if (quitting) return;
  console.error(error);
  dialog.showErrorBox(
    "Oddity 启动失败",
    error instanceof Error ? error.message : String(error),
  );
  app.quit();
}
app.on("window-all-closed", () => {});
app.on("before-quit", () => {
  quitting = true;
  backend?.close();
  tray?.destroy();
});
if (!app.requestSingleInstanceLock()) app.quit();
else {
  const ready = app.whenReady().then(start);
  app.on("second-instance", (_event, argv) => {
    const forwarded = argv.slice(app.isPackaged ? 1 : 2);
    void ready
      .then(() =>
        forwarded.includes("--launch")
          ? backend.call("external_launch", { argv: forwarded, cold: false })
          : showMain(),
      )
      .catch(fatal);
  });
  void ready.catch(fatal);
}
