import { defaultGamescope } from "../../src/core/gameMode";
import { defaultAttendanceTasks } from "../../src/core/attendanceTasks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { nextTick } from "vue";
import { APP_VERSION } from "../../src/core/version";
import { bootstrap, check, session, settings } from "./fixtures";
import type {
  Bootstrap,
  CheckResult,
  GameStatus,
  DownloadStatus,
  LauncherBackgroundMedia,
  Settings,
} from "../../src/types/launcher";

const backend = vi.hoisted(() => ({
  native: true,
  unlisten: vi.fn(),
  downloadUnlisten: vi.fn(),
  stopResize: vi.fn(),
  invoke: vi.fn(),
  handler: (_status: GameStatus) => {},
  downloadHandler: (_status: DownloadStatus) => {},
  downloadStatus: {} as DownloadStatus,
  startStatus: null as DownloadStatus | null,
  startFail: false,
  data: {} as Bootstrap,
  result: {} as CheckResult,
  status: {} as GameStatus,
  background: { video: null, image: null } as LauncherBackgroundMedia,
  isMaximized: vi.fn(),
  onResized: vi.fn(),
  resize: async () => {},
}));
vi.mock("../../src/core/api", () => ({
  isTauri: () => backend.native,
  invoke: backend.invoke,
  listenGameState: (handler: (status: GameStatus) => void) => {
    backend.handler = handler;
    return backend.unlisten;
  },
  listenDownloadState: (handler: (status: DownloadStatus) => void) => {
    backend.downloadHandler = handler;
    return backend.downloadUnlisten;
  },
}));
const notificationProjection = vi.hoisted(() => ({
  items: [] as import("../../src/core/notifications").NotificationItem[],
}));
// 控制器测试检查通知数据, 运动轨迹由浏览器用例验证。
vi.mock("../../src/composables/useNotificationArrival", async () => {
  const { computed, ref } = await import("vue");
  return {
    useNotificationArrival: () => ({
      displayed: computed(() => notificationProjection.items),
      retiring: ref([]),
      landed: computed(() =>
        notificationProjection.items.map((item) => item.id),
      ),
      incoming: ref(null),
      revealed: ref(null),
      phase: ref(0),
      settled: computed(() => notificationProjection.items),
    }),
  };
});
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isMaximized: backend.isMaximized,
    onResized: backend.onResized,
  }),
}));
let wrapper: VueWrapper | undefined;
let listeners: [
  EventTarget,
  string,
  EventListenerOrEventListenerObject | null,
  boolean | AddEventListenerOptions | undefined,
][];

beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    drawImage: vi.fn(),
  } as unknown as CanvasRenderingContext2D);
  backend.native = true;
  backend.unlisten.mockClear();
  backend.downloadUnlisten.mockClear();
  backend.stopResize.mockClear();
  backend.data = bootstrap();
  backend.result = check();
  backend.status = session();
  backend.background = { video: null, image: null };
  backend.downloadStatus = {
    mode: "check",
    phase: "idle",
    distribution: "cn",
    installRoot: "/game",
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
    diskAvailableBytes: 1024 * 1024 * 1024,
    diskRequiredBytes: 0,
    error: "",
  };
  backend.startStatus = structuredClone(backend.downloadStatus);
  backend.startFail = false;
  backend.invoke
    .mockReset()
    .mockImplementation(
      async (
        command: string,
        args?: { settings: Settings; mode: "check" | "apply" },
      ) => {
        switch (command) {
          case "attendance_status":
            return {
              accounts: [],
              preferences: {
                trigger: "off",
                time: "08:00",
                tasks: { ...defaultAttendanceTasks },
              },
            };
          case "launcher_background":
            return structuredClone(backend.background);
          case "bootstrap":
            return structuredClone(backend.data);
          case "check_update":
            return structuredClone(backend.result);
          case "game_status":
            return structuredClone(backend.status);
          case "game_download_status":
            return structuredClone(backend.downloadStatus);
          case "game_download_start": {
            if (backend.startFail) throw new Error("quick check offline");
            if (backend.startStatus) {
              backend.downloadStatus = structuredClone(backend.startStatus);
              return structuredClone(backend.downloadStatus);
            }
            backend.downloadStatus = {
              ...backend.downloadStatus,
              mode: args!.mode,
              phase: "checking",
              distribution: backend.data.settings.distribution,
              installRoot:
                backend.data.settings.distribution === "global"
                  ? backend.data.settings.globalInstallRoot ||
                    backend.data.installRoot
                  : backend.data.settings.installRoot ||
                    backend.data.installRoot,
            };
            return structuredClone(backend.downloadStatus);
          }
          case "game_download_cancel":
            backend.downloadStatus = {
              ...backend.downloadStatus,
              phase: "cancelled",
            };
            return structuredClone(backend.downloadStatus);
          case "save_settings":
            backend.data.settings = { ...args!.settings };
            backend.data.installRoot =
              args!.settings.distribution === "global"
                ? args!.settings.globalInstallRoot || "/games/global"
                : args!.settings.installRoot || "/game";
            return structuredClone(backend.data);
          case "launch_official":
            backend.status = session({ running: true, pid: 123 });
            return { pid: 123 };
          case "close_game":
            backend.status = session();
            return structuredClone(backend.status);
          default:
            return null;
        }
      },
    );
  backend.isMaximized.mockReset().mockResolvedValue(false);
  backend.onResized
    .mockReset()
    .mockImplementation(async (callback: () => Promise<void>) => {
      backend.resize = callback;
      return backend.stopResize;
    });
  listeners = [];
  const add = EventTarget.prototype.addEventListener;
  vi.spyOn(EventTarget.prototype, "addEventListener").mockImplementation(
    function (this: EventTarget, type, callback, options) {
      listeners.push([this, type, callback, options]);
      add.call(this, type, callback, options);
    },
  );
});
afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
  for (const [target, type, callback, options] of listeners)
    target.removeEventListener(type, callback, options);
});

async function boot(
  beforeCheck?: (
    controller: typeof import("../../src/core/controller"),
  ) => void,
) {
  const controller = await import("../../src/core/controller");
  const { default: App } = await import("../../src/App.vue");
  notificationProjection.items = controller.notifications;
  wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  beforeCheck?.(controller);
  await vi.advanceTimersByTimeAsync(1000);
  await flushPromises();
  backend.startStatus = null;
  backend.startFail = false;
  return controller;
}
const text = (id: string) => document.getElementById(id)?.textContent;
const noticeMessage = () =>
  document.querySelector(".notification-message")?.textContent;
const button = (id: string) => document.getElementById(id) as HTMLButtonElement;
const primary = () => wrapper!.find("#game-download-primary");
const primaryText = () => primary().text();
it("环境操作先保存修改, 处理期间拒绝重入并同步新登记", async () => {
  const c = await boot();
  const base = backend.invoke.getMockImplementation()!;
  let finish!: (data: Bootstrap) => void;
  let releaseSave!: () => void;
  const saving = new Promise<void>((resolve) => {
    releaseSave = resolve;
  });
  backend.invoke.mockClear();
  backend.invoke.mockImplementation((command: string, args: never) =>
    command === "save_settings"
      ? saving.then(() => base(command, args))
      : command === "environment_create"
        ? new Promise<Bootstrap>((resolve) => {
            finish = resolve;
          })
        : base(command, args),
  );
  c.form.theme = "dark";
  c.form.globalInstallRoot = "/new-install";
  const task = c.createRuntimeEnvironment(
    "测试环境",
    "/new-proton",
    null,
    "cn",
  );
  await flushPromises();
  expect(c.runtimeEnvironmentBusy.value).toBe(true);
  await c.checkUpdate();
  await c.startGameDownload("check");
  expect(backend.invoke.mock.calls.map(([command]) => command)).toEqual([
    "save_settings",
  ]);
  expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
    settings: expect.objectContaining({ globalInstallRoot: "/new-install" }),
  });
  releaseSave();
  await flushPromises();
  expect(backend.invoke).toHaveBeenCalledWith("environment_create", {
    distribution: "cn",
    name: "测试环境",
    protonPath: "/new-proton",
    sourceId: null,
  });
  expect(c.runtimeEnvironmentBusy.value).toBe(true);
  expect(c.settingsPathsLocked.value).toBe(true);
  expect(
    await c.createRuntimeEnvironment("重复", "/new-proton", null, "cn"),
  ).toBe(false);
  const next = bootstrap({
    settings: {
      ...backend.data.settings,
      runtimeEnvironments: [
        {
          id: "a".repeat(32),
          name: "测试环境",
          proton: { path: "/new-proton", identity: "pin" },
          installRoot: "/game",
          distribution: "cn",
          createdAt: 1,
        },
      ],
    },
  });
  finish(next);
  expect(await task).toBe(true);
  expect(c.runtimeEnvironmentBusy.value).toBe(false);
  expect(c.state.bootstrap?.settings.runtimeEnvironments).toHaveLength(1);
  c.form.reduceMotion = false;
  await c.saveSettings();
  expect(backend.data.settings.runtimeEnvironments[0].name).toBe("测试环境");
});

it("环境操作失败时保留原选择并释放等待状态", async () => {
  const c = await boot();
  backend.invoke.mockRejectedValueOnce(new Error("固定的 Proton 已变化"));
  expect(await c.runtimeEnvironmentAction("switch", "missing")).toBe(false);
  expect(c.runtimeEnvironmentError.value).toContain("Proton 已变化");
  expect(c.runtimeEnvironmentBusy.value).toBe(false);
  expect(c.state.bootstrap?.settings.activeEnvironment).toBeNull();
});
const key = (value: string, options: KeyboardEventInit = {}) =>
  document.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: value,
      bubbles: true,
      cancelable: true,
      ...options,
    }),
  );

describe("启动与设置", () => {
  it("加载配置、诊断和版本, 并自动检查更新", async () => {
    const c = await boot();
    expect(c.attendance.loaded).toBe(true);
    expect(c.attendance.loading).toBe(false);
    expect(c.form.installRoot).toBe("/game");
    expect(wrapper!.findAll(".check-row")).toHaveLength(1);
    expect(c.displayVersion.value).toBe("0.1.0");
    expect(text("update-state-title")).toBe("已是最新");
    expect(button("app").classList.contains("is-intro-done")).toBe(true);
    backend.isMaximized.mockResolvedValueOnce(true);
    await backend.resize();
    expect(button("app").classList.contains("is-maximized")).toBe(true);
  });

  it("缺少环境和版本时显示配置提示和默认值", async () => {
    backend.data = bootstrap({
      ok: false,
      error: "找不到游戏",
      gameExists: false,
      appVersion: "",
      launcherVersion: "",
      installRoot: "",
      prefix: "",
      protonPath: "",
      settings: { ...settings, theme: "", heroMedia: "", launchMode: "" },
    });
    const c = await boot();
    expect(button("setup-chip").hidden).toBe(false);
    expect(noticeMessage()).toBe("找不到游戏");
    expect(c.displayVersion.value).toBe(APP_VERSION);
    expect(text("update-state-text")).toBe("等待检查");
    expect(c.form.launchMode).toBe("background");
    expect(c.form.theme).toBe("light");
  });

  it("未创建运行环境时, 启动操作转到设置并提示创建", async () => {
    backend.data = bootstrap({
      prefix: "",
      prefixExists: false,
      protonPath: "",
    });
    const c = await boot();
    backend.invoke.mockClear();
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.page).toBe("settings");
    expect(noticeMessage()).toContain("请先在设置中创建");
    expect(backend.invoke).not.toHaveBeenCalledWith("launch_official");
  });

  it("启动失败仍完成入场并允许手动重新检测", async () => {
    const invoke = backend.invoke.getMockImplementation()!;
    let failedStatus = false;
    let failedBootstrap = false;
    backend.invoke.mockImplementation(async (command, args) => {
      if (command === "game_status" && !failedStatus) {
        failedStatus = true;
        throw new Error("status unavailable");
      }
      if (command === "bootstrap" && !failedBootstrap) {
        failedBootstrap = true;
        throw new Error("bootstrap failed");
      }
      return invoke(command, args);
    });
    const c = await boot();
    expect(noticeMessage()).toBe("bootstrap failed");
    expect(button("app").classList.contains("is-intro-done")).toBe(true);
    await c.redetect();
    expect(c.state.bootstrap?.ok).toBe(true);
    expect(noticeMessage()).toBe("已重新检测环境。");
    backend.invoke.mockRejectedValueOnce("redetect failed");
    await c.redetect();
    expect(noticeMessage()).toBe("redetect failed");
    expect(button("settings-redetect").disabled).toBe(false);
  });

  it("重新检测前保存失败时保留表单并停止刷新", async () => {
    const c = await boot();
    c.form.globalInstallRoot = "/draft-prefix";
    backend.invoke.mockClear();
    backend.invoke.mockRejectedValueOnce(new Error("disk full"));
    await c.redetect();
    expect(noticeMessage()).toBe("disk full");
    expect(c.form.globalInstallRoot).toBe("/draft-prefix");
    expect(c.ui.saveState).toBe("error");
    expect(backend.invoke.mock.calls.map(([command]) => command)).toEqual([
      "save_settings",
    ]);
    expect(button("settings-redetect").disabled).toBe(false);
  });

  it("无本地存储权限时仍能应用外观", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    const c = await boot();
    expect(c.applyTheme("invalid")).toBe("light");
    expect(c.applyTheme("dark")).toBe("dark");
    await nextTick();
    expect(button("app").dataset.theme).toBe("dark");
    expect(c.applyReduceMotion(false)).toBe(false);
    await nextTick();
    expect(button("app").classList.contains("reduce-motion")).toBe(false);
  });

  it("保存时清理路径, 持久化外观, 失败后保留错误状态", async () => {
    const c = await boot();
    Object.assign(c.form, {
      installRoot: " /new ",
      launchMode: "",
      theme: "dark",
      reduceMotion: false,
      heroMedia: "photo",
    });
    await c.saveSettings();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: {
        ...settings,
        installRoot: "/new",
        globalInstallRoot: null,
        distribution: "cn",
        launchMode: "background",
        language: "zh-CN",
        theme: "dark",
        reduceMotion: false,
        heroMedia: "photo",
        minimizeToTrayOnClose: true,
        showLauncherAfterGameExit: true,
      },
    });
    expect(c.ui.saveState).toBe("saved");
    await vi.advanceTimersByTimeAsync(2000);
    expect(c.ui.saveState).toBe("idle");
    Object.assign(c.form, { installRoot: "", globalInstallRoot: "" });
    backend.invoke.mockRejectedValueOnce(new Error("disk full"));
    await c.saveSettings();
    expect(noticeMessage()).toBe("disk full");
    expect(c.ui.saveState).toBe("error");
    await vi.advanceTimersByTimeAsync(6000);
    expect(document.querySelector(".notification-stack")).toBeNull();
  });

  it("文本输入防抖自动保存, 保存状态随后回到空闲", async () => {
    const c = await boot();
    backend.invoke.mockClear();
    const input = document.querySelector<HTMLInputElement>("#set-install")!;
    input.value = " /auto-game ";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    expect(c.ui.saveState).toBe("idle");
    await vi.advanceTimersByTimeAsync(499);
    expect(backend.invoke).not.toHaveBeenCalledWith(
      "save_settings",
      expect.anything(),
    );
    await vi.advanceTimersByTimeAsync(1);
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({ installRoot: "/auto-game" }),
    });
    expect(c.ui.saveState).toBe("saved");
    await vi.advanceTimersByTimeAsync(2000);
    expect(c.ui.saveState).toBe("idle");
  });

  it("修改安装目录后统一刷新启动器与游戏资源", async () => {
    await boot();
    backend.invoke.mockClear();
    const input = document.querySelector<HTMLInputElement>("#set-install")!;
    input.value = "/new-game";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({ installRoot: "/new-game" }),
    });
    expect(backend.invoke).toHaveBeenCalledWith("check_update", {
      full: false,
    });
    expect(backend.invoke).toHaveBeenCalledWith("game_download_start", {
      mode: "check",
      full: false,
    });
  });

  it("更新页修改安装目录也会统一刷新", async () => {
    const c = await boot();
    c.showPage("update");
    await nextTick();
    backend.invoke.mockClear();
    const input = document.querySelector<HTMLInputElement>(
      "#download-install-root",
    )!;
    input.value = "/update-game";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({ installRoot: "/update-game" }),
    });
    expect(backend.invoke).toHaveBeenCalledWith("check_update", {
      full: false,
    });
    expect(backend.invoke).toHaveBeenCalledWith("game_download_start", {
      mode: "check",
      full: false,
    });
  });

  it("切换到没有启动器的目录时两张状态卡同时显示未安装", async () => {
    backend.data = bootstrap({ launcherRoot: "", launcherVersion: "" });
    const c = await boot();
    backend.invoke.mockClear();
    const input = document.querySelector<HTMLInputElement>("#set-install")!;
    input.value = "/empty-game";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await vi.advanceTimersByTimeAsync(500);
    await flushPromises();

    expect(c.updateView.value.title).toBe("尚未安装");
    await c.showPage("update");
    await nextTick();
    expect(text("game-download-state-title")).toBe("需要安装启动器文件");
    expect(primaryText()).toBe("安装游戏");
    expect(text("update-state-title")).toBe("尚未安装");
    expect(backend.invoke).not.toHaveBeenCalledWith("check_update", {
      full: false,
    });
    expect(backend.invoke).not.toHaveBeenCalledWith("game_download_start", {
      mode: "check",
      full: false,
    });
  });

  it("开关修改立即自动保存", async () => {
    const c = await boot();
    const checkbox = document.querySelector<HTMLInputElement>(
      "#set-minimize-to-tray",
    )!;
    checkbox.checked = false;
    checkbox.dispatchEvent(new Event("change", { bubbles: true }));
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({ minimizeToTrayOnClose: false }),
    });
    expect(c.ui.saveState).toBe("saved");
  });

  it("游戏模式选项通过统一保存队列持久化并从后端恢复", async () => {
    const c = await boot();
    Object.assign(c.form.gameMode, {
      performance: true,
      inhibit: true,
      notifications: false,
      overlay: true,
      gamescope: { ...defaultGamescope },
    });
    c.queueSettingsSave(0);
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        gameMode: {
          performance: true,
          inhibit: true,
          notifications: false,
          overlay: true,
          gamescope: { ...defaultGamescope },
        },
      }),
    });
    expect(c.ui.saveState).toBe("saved");
    await c.redetect();
    expect(c.form.gameMode).toEqual({
      performance: true,
      inhibit: true,
      notifications: false,
      overlay: true,
      gamescope: { ...defaultGamescope },
    });
  });

  it("下载写入期间锁住地区和安装目录, 其他设置仍可修改", async () => {
    const c = await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      mode: "apply",
      phase: "game",
    });
    await nextTick();
    expect(c.settingsPathsLocked.value).toBe(true);
    expect(
      document.querySelector<HTMLSelectElement>("#set-distribution")!.disabled,
    ).toBe(true);
    expect(
      document.querySelector<HTMLButtonElement>("#set-distribution-trigger")!
        .disabled,
    ).toBe(true);
    expect(
      document.querySelector<HTMLInputElement>("#set-install")!.disabled,
    ).toBe(true);
    expect(
      document.querySelector<HTMLInputElement>("#set-global-install")!.disabled,
    ).toBe(true);
    expect(
      document.querySelector<HTMLSelectElement>("#set-theme")!.disabled,
    ).toBe(false);
    expect(
      document.querySelector<HTMLSelectElement>("#set-mode")!.disabled,
    ).toBe(false);
    expect(wrapper!.find(".settings-lock").text()).toContain(
      "暂时无法修改游戏地区和安装目录",
    );
    backend.invoke.mockClear();
    c.applyTheme("dark");
    c.queueSettingsSave(0);
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        installRoot: null,
        globalInstallRoot: null,
        theme: "dark",
      }),
    });
    expect(c.ui.saveState).toBe("saved");
    backend.downloadHandler({
      ...backend.downloadStatus,
      mode: "check",
      phase: "checking",
    });
    await nextTick();
    expect(c.settingsPathsLocked.value).toBe(false);
    expect(
      document.querySelector<HTMLInputElement>("#set-install")!.disabled,
    ).toBe(false);
    expect(wrapper!.find(".settings-lock").exists()).toBe(false);
  });

  it("保存中的连续修改串行提交最新快照", async () => {
    const c = await boot();
    let finish!: (data: Bootstrap) => void;
    backend.invoke.mockImplementationOnce(
      () =>
        new Promise<Bootstrap>((resolve) => {
          finish = resolve;
        }),
    );
    c.form.globalInstallRoot = "/first";
    c.queueSettingsSave(0);
    await flushPromises();
    c.form.globalInstallRoot = "/second";
    c.queueSettingsSave(0);
    await flushPromises();
    finish(bootstrap());
    await flushPromises();
    const saves = backend.invoke.mock.calls.filter(
      ([command]) => command === "save_settings",
    );
    expect(saves).toHaveLength(2);
    expect(saves[1][1]).toMatchObject({
      settings: { globalInstallRoot: "/second" },
    });
    expect(c.ui.saveState).toBe("saved");
  });

  it("自动保存失败后在下一次修改时重试", async () => {
    const c = await boot();
    backend.invoke.mockRejectedValueOnce(new Error("disk full"));
    c.form.globalInstallRoot = "/blocked";
    c.queueSettingsSave(0);
    await flushPromises();
    expect(noticeMessage()).toBe("disk full");
    expect(c.ui.saveState).toBe("error");
    c.form.globalInstallRoot = "/retry";
    c.queueSettingsSave(0);
    await flushPromises();
    expect(c.ui.saveState).toBe("saved");
    expect(backend.invoke).toHaveBeenLastCalledWith(
      "save_settings",
      expect.objectContaining({
        settings: expect.objectContaining({ globalInstallRoot: "/retry" }),
      }),
    );
  });

  it("关闭窗口时最小化到托盘配置支持切换与保存", async () => {
    const c = await boot();
    expect(c.form.minimizeToTrayOnClose).toBe(true);
    const checkbox = document.querySelector<HTMLInputElement>(
      "#set-minimize-to-tray",
    )!;
    expect(checkbox.checked).toBe(true);
    c.form.minimizeToTrayOnClose = false;
    await c.saveSettings();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        minimizeToTrayOnClose: false,
      }),
    });
  });

  it("游戏退出后打开启动器配置支持切换与自动保存", async () => {
    const c = await boot();
    expect(c.form.showLauncherAfterGameExit).toBe(true);
    const checkbox = document.querySelector<HTMLInputElement>(
      "#set-show-after-game-exit",
    )!;
    expect(checkbox.checked).toBe(true);
    checkbox.checked = false;
    checkbox.dispatchEvent(new Event("change", { bubbles: true }));
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        showLauncherAfterGameExit: false,
      }),
    });
    expect(c.ui.saveState).toBe("saved");
  });
});

describe("下载大小显示", () => {
  it("按 B、KiB、MiB、GiB 格式化并处理零值", async () => {
    const c = await boot();
    expect(c.formatSize(0)).toBe("0 B");
    expect(c.formatSize(1024)).toBe("1.0 KiB");
    expect(c.formatSize(1536)).toBe("1.5 KiB");
    expect(c.formatSize(330 * 1024 * 1024)).toBe("330 MiB");
    expect(c.formatSize(3.4 * 1024 * 1024 * 1024)).toBe("3.4 GiB");
  });

  it("检查后展示空间需求并标出缺口", async () => {
    const c = await boot();
    await c.showPage("update");
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
      pendingFiles: 2,
      totalFiles: 2,
      diskAvailableBytes: 800 * 1024 * 1024,
      diskRequiredBytes: 1024 * 1024 * 1024,
    });
    await nextTick();
    expect(c.diskBudget.value).toEqual({
      availableText: "800 MiB",
      requiredText: "1.0 GiB",
      shortfallText: "224 MiB",
      insufficient: true,
      percent: 100,
    });
    expect(document.body.textContent).toContain("预计需要 1.0 GiB");
    expect(document.body.textContent).toContain("可用 800 MiB");
    expect(document.body.textContent).toContain("还差 224 MiB");
    expect(document.body.textContent).toContain("清理磁盘空间或更换安装目录");
  });

  it("空间充足时按占用比例展示且不提示缺口", async () => {
    const c = await boot();
    await c.showPage("update");
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
      pendingFiles: 1,
      totalFiles: 1,
      diskAvailableBytes: 2 * 1024 * 1024 * 1024,
      diskRequiredBytes: 512 * 1024 * 1024,
    });
    await nextTick();
    expect(c.diskBudget.value).toMatchObject({
      insufficient: false,
      percent: 25,
    });
    expect(document.body.textContent).not.toContain("还差");
    expect(document.body.textContent).not.toContain(
      "清理磁盘空间或更换安装目录",
    );
  });

  it("空间读取失败或没有待处理资源时不展示预算", async () => {
    const c = await boot();
    await c.showPage("update");
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
      pendingFiles: 1,
      diskAvailableBytes: null,
      diskRequiredBytes: 100,
    });
    await nextTick();
    expect(c.diskBudget.value).toBeNull();

    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
      pendingFiles: 1,
      diskAvailableBytes: 0,
      diskRequiredBytes: 100,
    });
    await nextTick();
    expect(c.diskBudget.value).toMatchObject({
      insufficient: true,
      percent: 100,
      shortfallText: "100 B",
    });

    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "ready",
      pendingFiles: 0,
      diskAvailableBytes: 100,
      diskRequiredBytes: 0,
    });
    await nextTick();
    expect(c.diskBudget.value).toBeNull();
  });
});

describe("更新检查", () => {
  it("启动检查不持久化探测路径或尚未保存的表单", async () => {
    const c = await boot((controller) => {
      expect(controller.form.globalInstallRoot).toBe("");
      controller.form.globalInstallRoot = "/draft-prefix";
    });
    expect(backend.invoke).not.toHaveBeenCalledWith(
      "save_settings",
      expect.anything(),
    );
    expect(c.form.globalInstallRoot).toBe("/draft-prefix");
  });

  it.each(["failed", "cancelled", "ready"] as const)(
    "开始响应不能覆盖先收到的 %s 事件",
    async (phase) => {
      const c = await boot();
      const original = backend.invoke.getMockImplementation()!;
      let resolve!: (status: DownloadStatus) => void;
      backend.invoke.mockImplementation((command, args) =>
        command === "game_download_start"
          ? new Promise<DownloadStatus>((done) => {
              resolve = done;
            })
          : original(command, args),
      );
      const pending = c.startGameDownload("check");
      await flushPromises();
      backend.downloadHandler({ ...backend.downloadStatus, phase });
      resolve({ ...backend.downloadStatus, phase: "checking" });
      await pending;
      expect(c.state.download?.phase).toBe(phase);
      expect(c.downloadBusy()).toBe(false);
    },
  );

  it("取消响应不能覆盖先收到的终止事件", async () => {
    const c = await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      mode: "apply",
      phase: "game",
    });
    let resolve!: (status: DownloadStatus) => void;
    backend.invoke.mockImplementationOnce(
      () =>
        new Promise<DownloadStatus>((done) => {
          resolve = done;
        }),
    );
    const pending = c.cancelGameDownload();
    backend.downloadHandler({ ...backend.downloadStatus, phase: "cancelled" });
    resolve({ ...backend.downloadStatus, mode: "apply", phase: "game" });
    await pending;
    expect(c.state.download?.phase).toBe("cancelled");
    expect(c.downloadBusy()).toBe(false);
  });

  it("就绪后的环境刷新保留等待期间的设置编辑和主题", async () => {
    const c = await boot();
    let resolve!: (data: Bootstrap) => void;
    backend.invoke.mockImplementationOnce(
      () =>
        new Promise<Bootstrap>((done) => {
          resolve = done;
        }),
    );
    backend.downloadHandler({ ...backend.downloadStatus, phase: "ready" });
    c.form.installRoot = "/unsaved/game";
    c.form.globalInstallRoot = "/unsaved/prefix";
    c.applyTheme("dark");
    resolve(bootstrap({ gameVersion: "2.0" }));
    await flushPromises();
    expect(c.state.bootstrap?.gameVersion).toBe("2.0");
    expect(c.form.installRoot).toBe("/unsaved/game");
    expect(c.form.globalInstallRoot).toBe("/unsaved/prefix");
    expect(c.form.theme).toBe("dark");
    expect(primaryText()).toBe("检查资源");
  });

  it("就绪刷新不能覆盖在请求期间保存的新地区", async () => {
    const c = await boot();
    let resolve!: (data: Bootstrap) => void;
    backend.invoke.mockImplementationOnce(
      () =>
        new Promise<Bootstrap>((done) => {
          resolve = done;
        }),
    );
    backend.downloadHandler({ ...backend.downloadStatus, phase: "ready" });
    c.form.distribution = "global";
    await c.saveSettings();
    resolve(bootstrap());
    await flushPromises();
    expect(c.state.bootstrap?.settings.distribution).toBe("global");
    expect(c.form.distribution).toBe("global");
  });

  it("只读检查可取消且不阻止启动和保存, 安装检查仍阻止启动", async () => {
    const c = await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      mode: "check",
      phase: "checking",
    });
    await nextTick();
    expect(document.body.textContent).toContain("取消检查");
    expect(c.launchView.value.busy).toBe(false);
    expect(await c.saveSettings()).toBe(true);
    c.handleLaunchAction();
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("launch_official");
    backend.downloadHandler({
      ...backend.downloadStatus,
      mode: "apply",
      phase: "checking",
    });
    expect(c.launchView.value.busy).toBe(true);
  });

  it("首页更新提示使用已保存地区", async () => {
    const c = await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
    });
    c.form.distribution = "global";
    expect(c.homeDownloadNotice.value.visible).toBe(true);
    expect(c.launchView.value.label).toBe("更新游戏");
    backend.downloadHandler({
      ...backend.downloadStatus,
      distribution: "global",
      phase: "needs-update",
    });
    expect(c.homeDownloadNotice.value.visible).toBe(false);
    expect(c.launchView.value.label).toBe("启动游戏");
  });

  it("为国际服保存独立目录并展示可暂停的下载任务", async () => {
    const c = await boot();
    c.form.distribution = "global";
    c.form.globalInstallRoot = "/games/global";
    await c.startGameDownload("apply", true);
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        distribution: "global",
        globalInstallRoot: "/games/global",
      }),
    });
    expect(backend.invoke).toHaveBeenCalledWith("game_download_start", {
      mode: "apply",
      full: true,
    });
    expect(c.state.download?.phase).toBe("checking");
    expect(document.body.textContent).not.toContain("暂停下载");
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "game",
      currentFile: "Client/data.pak",
      currentFileBytes: 300 * 1024 * 1024,
      downloadedBytes: 4,
      totalBytes: 10,
      patchBytes: 6,
      fullBytes: 4,
      completedFiles: 1,
      totalFiles: 2,
      pendingFiles: 2,
      stage: "patch",
    });
    await nextTick();
    expect(document.body.textContent).toContain("暂停下载");
    expect(c.downloadView.value.progress).toBe(40);
    expect(c.downloadView.value.sizeText).toBe("本次需下载 10 B");
    expect(c.downloadView.value.stageText).toBe("应用补丁");
    expect(c.downloadView.value.splitText).toBe("补丁 6 B / 完整文件 4 B");
    expect(document.body.textContent).toContain("本次需下载 10 B");
    expect(document.body.textContent).toContain("补丁 6 B / 完整文件 4 B");
    expect(document.body.textContent).toContain("正在应用补丁");
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "game",
      downloadedBytes: 8,
      totalBytes: 10,
      completedFiles: 0,
      totalFiles: 2,
      pendingFiles: 2,
    });
    await nextTick();
    expect(c.downloadView.value.progress).toBe(80);
    expect(c.downloadView.value.downloadedText).toBe("8 B / 10 B");
    expect(button("download-install-root").disabled).toBe(true);
    await c.cancelGameDownload();
    expect(backend.invoke).toHaveBeenCalledWith("game_download_cancel");
  });

  it("主按钮按检查结果显示检查通过、安装或更新", async () => {
    const c = await boot();
    expect(primaryText()).toBe("检查资源");
    await primary().trigger("click");
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("game_download_start", {
      mode: "check",
      full: false,
    });
    expect(document.body.textContent).not.toContain("暂停下载");

    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
      installed: true,
      pendingFiles: 1,
      totalFiles: 1,
    });
    await flushPromises();
    expect(primaryText()).toBe("更新游戏");
    c.state.game = "running";
    await nextTick();
    expect(button("game-download-primary").disabled).toBe(true);
    c.state.game = "idle";
    await nextTick();
    expect(button("game-download-primary").disabled).toBe(false);
    await primary().trigger("click");
    await flushPromises();
    expect(backend.invoke).toHaveBeenLastCalledWith("game_download_start", {
      mode: "apply",
      full: false,
    });
    expect(document.body.textContent).not.toContain("暂停下载");

    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
      installed: false,
      pendingFiles: 1,
      totalFiles: 1,
    });
    await flushPromises();
    expect(primaryText()).toBe("安装游戏");

    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "ready",
      installed: true,
      pendingFiles: 0,
      totalFiles: 1,
    });
    await flushPromises();
    expect(primaryText()).toBe("检查通过");
    expect(button("game-download-primary").disabled).toBe(false);
    expect(
      button("game-download-primary").classList.contains("is-passed"),
    ).toBe(true);
    expect(document.body.textContent).not.toContain("暂停下载");

    // 安装目录改变后，旧结果不再适用。
    c.form.installRoot = "/other";
    await flushPromises();
    expect(primaryText()).toBe("检查资源");

    // 新目录开始检查时应立即显示检查中，而不是沿用旧结果。
    await primary().trigger("click");
    await flushPromises();
    expect(primaryText()).toBe("正在检查");
    expect(text("game-download-state-title")).toBe("正在检查游戏资源");
  });

  it("首次安装只显示启动器文件体积并说明后续检查", async () => {
    const c = await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-install",
      installed: false,
      pendingFiles: 683,
      totalFiles: 683,
      totalBytes: 274636864,
    });
    await flushPromises();
    expect(c.downloadView.value.title).toBe("需要安装启动器文件");
    expect(c.downloadView.value.detail).toBe(
      "启动器文件尚未安装, 完成后将继续检查游戏资源",
    );
    expect(c.downloadView.value.sizeText).toBe("启动器文件 262 MiB");
    expect(primaryText()).toBe("安装游戏");
    expect(document.body.textContent).toContain("启动器文件 262 MiB");
    expect(document.body.textContent).not.toContain("本次需下载 262 MiB");
  });

  it("启动时静默快速检查并在发现小版本更新时更新入口", async () => {
    backend.startStatus = {
      ...backend.downloadStatus,
      phase: "needs-update",
      installed: true,
      pendingFiles: 2,
      totalFiles: 2,
      totalBytes: 2048,
    };
    const c = await boot();
    expect(backend.invoke).toHaveBeenCalledWith("game_download_start", {
      mode: "check",
      full: false,
    });
    expect(c.state.download?.phase).toBe("needs-update");
    expect(c.launchView.value.label).toBe("更新游戏");
    expect(primaryText()).toBe("更新游戏");
    expect(document.querySelector(".notification-stack")).toBeNull();
  });

  it("未安装游戏时启动不执行资源检查", async () => {
    backend.data = bootstrap({ gameExists: false });
    const c = await boot();
    expect(backend.invoke).not.toHaveBeenCalledWith("game_download_start", {
      mode: "check",
      full: false,
    });
    expect(c.state.download?.phase ?? "idle").toBe("idle");
  });

  it("启动时的快速检查失败保持静默", async () => {
    backend.startFail = true;
    const c = await boot();
    expect(c.state.download?.phase ?? "idle").toBe("idle");
    expect(document.querySelector(".notification-stack")).toBeNull();
  });

  it("完整校验发现损坏后，主按钮继续提交完整校验", async () => {
    const c = await boot();
    await wrapper!
      .findAll("#page-update button")
      .find((item) => item.text() === "完整校验")!
      .trigger("click");
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith("game_download_start", {
      mode: "check",
      full: true,
    });
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
      full: true,
      pendingFiles: 1,
      totalFiles: 1,
    });
    await nextTick();
    expect(primaryText()).toBe("更新游戏");
    await primary().trigger("click");
    await flushPromises();
    expect(backend.invoke).toHaveBeenLastCalledWith("game_download_start", {
      mode: "apply",
      full: true,
    });
    expect(c.state.download?.phase).toBe("checking");
  });

  it("未知大文件显示待验证提示，并阻止首页直接启动游戏", async () => {
    const c = await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-verify",
      unverifiedFiles: 2,
    });
    await nextTick();
    expect(c.downloadView.value.title).toBe("存在尚未验证的游戏资源");
    expect(c.downloadView.value.detail).toContain("2 个文件尚未验证");
    expect(c.launchView.value.label).toBe("更新游戏");
    c.handleLaunchAction();
    expect(c.state.page).toBe("update");
    expect(backend.invoke).not.toHaveBeenCalledWith("launch_official");
  });

  it("检查通过后主按钮显示通过状态", async () => {
    await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "ready",
      full: true,
    });
    await flushPromises();
    expect(primaryText()).toBe("检查通过");
    expect(button("game-download-primary").disabled).toBe(false);
    expect(
      button("game-download-primary").classList.contains("is-passed"),
    ).toBe(true);
  });

  it("下载进度始终按体积推进, 完成后才显示 100%", async () => {
    const c = await boot();
    const status: DownloadStatus = {
      ...backend.downloadStatus,
      phase: "game",
      completedFiles: 1,
      totalFiles: 2,
      totalBytes: 100,
      downloadedBytes: 100,
      stage: "patch",
    };
    backend.downloadHandler(status);
    expect(c.downloadView.value.progress).toBe(99);
    backend.downloadHandler({ ...status, downloadedBytes: 101 });
    expect(c.downloadView.value.progress).toBe(99);
    backend.downloadHandler({
      ...status,
      downloadedBytes: 80,
      stage: "download",
    });
    expect(c.downloadView.value.progress).toBe(80);
    backend.downloadHandler({
      ...status,
      downloadedBytes: 150,
      stage: "download",
    });
    expect(c.downloadView.value.progress).toBe(99);
    backend.downloadHandler({ ...status, completedFiles: 2, stage: "install" });
    expect(c.downloadView.value.progress).toBe(99);
    backend.downloadHandler({
      ...status,
      phase: "ready",
      completedFiles: 2,
      stage: "",
    });
    expect(c.downloadView.value.progress).toBe(100);
  });

  it("暂停后仍按已下载体积显示进度", async () => {
    const c = await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "cancelled",
      completedFiles: 192,
      totalFiles: 334,
      totalBytes: Math.round(76.6 * 1024 ** 3),
      downloadedBytes: 579 * 1024 ** 2,
      stage: "install",
    });
    expect(c.downloadView.value.progress).toBe(1);
  });

  it("未完成时文件数回退也不会显示 100%", async () => {
    const c = await boot();
    backend.downloadHandler({
      ...backend.downloadStatus,
      phase: "needs-update",
      totalBytes: 0,
      downloadedBytes: 0,
      completedFiles: 3,
      totalFiles: 3,
    });
    expect(c.downloadView.value.progress).toBe(99);
  });

  it.each([
    [
      check({
        missing: 2,
        changed: 1,
        missingPreview: ["a"],
        changedPreview: ["b"],
      }),
      "3 个文件待更新",
      false,
    ],
    [
      check({
        changed: 1,
        changedPreview: ["QtQuick/Controls/Styles/Flat/plugins.qmltypes"],
      }),
      "已是最新",
      true,
    ],
    [check({ files: 0, localVersion: "" }), "已是最新", true],
  ])("展示缺失、变更和已知例外", async (result, expected, hidden) => {
    const c = await boot();
    backend.result = result;
    await c.checkUpdate(true);
    await nextTick();
    expect(text("update-state-title")).toBe(expected);
    expect(button("update-files").hidden).toBe(hidden);
    if (!hidden) {
      expect(wrapper!.findAll(".update-file")).toHaveLength(2);
      expect(wrapper!.find(".update-file__more").text()).toBe(
        "另有 1 个文件未展开",
      );
      expect(text("home-status-text")).toBe("3 个文件待更新");
    }
  });

  it("未安装启动器时显示尚未安装, 目录变化后旧结果失效", async () => {
    const c = await boot();
    backend.data = bootstrap({
      ok: false,
      error: "所选安装目录尚未安装启动器文件",
      launcherRoot: "",
      launcherVersion: "",
    });
    await c.redetect();
    expect(c.updateView.value.title).toBe("尚未安装");

    backend.result = check({
      installed: false,
      installRoot: "/game",
      files: 0,
      localVersion: "",
    });
    await c.checkUpdate(true);
    await nextTick();
    expect(c.updateView.value.title).toBe("尚未安装");
    expect(c.updateView.value.showFiles).toBe(false);

    // 已安装状态下的旧结果也不能沿用到新目录.
    backend.data = bootstrap();
    await c.redetect();
    backend.result = check({ installRoot: "/game" });
    await c.checkUpdate(true);
    await nextTick();
    expect(c.updateView.value.title).toBe("已是最新");
    c.form.installRoot = "/other";
    await nextTick();
    expect(c.updateView.value.title).toBe("尚未检查");
  });

  it("检查期间阻止重复请求, 失败后恢复按钮并按需提示", async () => {
    const c = await boot();
    let reject!: (error: Error) => void;
    backend.invoke.mockReturnValueOnce(
      new Promise((_resolve, fail) => {
        reject = fail;
      }),
    );
    const pending = c.checkUpdate();
    await nextTick();
    expect(button("update-quick").disabled).toBe(true);
    backend.invoke.mockClear();
    await c.checkUpdate();
    expect(backend.invoke).not.toHaveBeenCalled();
    reject(new Error("CDN offline"));
    await pending;
    expect(text("update-state-title")).toBe("检查失败");
    expect(text("home-status-text")).toBe("检查失败，打开更新页");
    expect(noticeMessage()).toBe("CDN offline");
    expect(button("update-quick").disabled).toBe(false);
    backend.invoke.mockRejectedValueOnce("silent error");
    await c.checkUpdate(false, true);
    expect(noticeMessage()).toBe("CDN offline");
  });
});

describe("游戏会话", () => {
  it("启动反馈期间忽略事件和重复操作, 启动成功后收起窗口", async () => {
    const c = await boot();
    c.handleLaunchAction();
    expect(c.state.game).toBe("launching");
    backend.handler(session());
    c.handleLaunchAction();
    window.dispatchEvent(new Event("focus"));
    expect(c.state.game).toBe("launching");
    await vi.advanceTimersByTimeAsync(900);
    expect(c.state.game).toBe("running");
    expect(backend.invoke).toHaveBeenCalledWith("win_hide");
    backend.handler(session());
    expect(c.state.game).toBe("idle");
    await nextTick();
    expect(noticeMessage()).toBe("游戏已退出。");
  });

  it("进程在反馈期间退出时不收起窗口", async () => {
    const c = await boot();
    c.handleLaunchAction();
    await flushPromises();
    backend.status = session();
    await vi.advanceTimersByTimeAsync(900);
    expect(c.state.game).toBe("idle");
    expect(noticeMessage()).toBe("游戏进程已退出，启动未完成。");
    expect(backend.invoke).not.toHaveBeenCalledWith("win_hide");
  });

  it("启动命令失败时用后端状态恢复按钮", async () => {
    const c = await boot();
    backend.invoke.mockRejectedValueOnce(new Error("spawn failed"));
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("idle");
    expect(noticeMessage()).toBe("spawn failed");
    backend.status = session({ running: true });
    backend.invoke.mockRejectedValueOnce("spawn uncertain");
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("running");
    expect(noticeMessage()).toBe("spawn uncertain");
  });

  it("启动状态查询或隐藏窗口失败仍保持可用的关闭按钮", async () => {
    const c = await boot();
    backend.invoke
      .mockResolvedValueOnce({ pid: 123 })
      .mockRejectedValueOnce(new Error("status failed"))
      .mockRejectedValueOnce(new Error("hide failed"));
    c.handleLaunchAction();
    await vi.advanceTimersByTimeAsync(900);
    expect(c.state.game).toBe("running");
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("idle");
    expect(noticeMessage()).toBe("游戏已关闭。");
  });

  it("关闭命令失败时同步仍运行、已退出和未知状态", async () => {
    const c = await boot();
    backend.status = session({ running: true });
    backend.handler(backend.status);
    backend.invoke.mockRejectedValueOnce(new Error("close failed"));
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("running");
    expect(noticeMessage()).toBe("close failed");
    backend.invoke
      .mockRejectedValueOnce("close unknown")
      .mockRejectedValueOnce(new Error("status failed"));
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("running");
    backend.status = session();
    backend.invoke.mockRejectedValueOnce("already closed");
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("idle");
  });

  it("托盘恢复同步运行和关闭状态, 查询失败不破坏按钮", async () => {
    const c = await boot();
    backend.status = session({ running: true, closing: true });
    window.dispatchEvent(new Event("focus"));
    await flushPromises();
    expect(c.state.game).toBe("closing");
    window.dispatchEvent(new Event("focus"));
    c.state.game = "idle";
    backend.invoke.mockRejectedValueOnce(new Error("probe failed"));
    window.dispatchEvent(new Event("focus"));
    await flushPromises();
    expect(c.state.game).toBe("idle");
  });

  it("浏览器预览不调用原生窗口命令", async () => {
    backend.native = false;
    const c = await boot();
    c.handleLaunchAction();
    await nextTick();
    expect(noticeMessage()).toBe("浏览器预览：模拟启动流程。");
    await vi.advanceTimersByTimeAsync(900);
    expect(c.state.game).toBe("running");
    expect(backend.invoke).not.toHaveBeenCalledWith("win_hide");
  });
});

describe("导航和窗口交互", () => {
  it("导航校验、深链接、快捷键和表单提交", async () => {
    history.replaceState(null, "", "/#settings");
    const c = await boot();
    expect(c.state.page).toBe("settings");
    c.showPage(undefined);
    c.showPage("invalid");
    expect(c.state.page).toBe("settings");
    key("1", { ctrlKey: true });
    await nextTick();
    expect(wrapper!.find("#page-home").classes()).toContain("is-active");
    key("2", { ctrlKey: true, altKey: true });
    key("2", { ctrlKey: true, metaKey: true });
    key("0", { ctrlKey: true });
    expect(c.state.page).toBe("home");
    key("3", { ctrlKey: true });
    expect(c.state.page).toBe("game-mode");
    key("5", { ctrlKey: true });
    expect(c.state.page).toBe("settings");
    backend.invoke.mockClear();
    key("s", { ctrlKey: true });
    await flushPromises();
    expect(backend.invoke).not.toHaveBeenCalledWith(
      "save_settings",
      expect.anything(),
    );
    history.replaceState(null, "", "/#bulletin");
    window.dispatchEvent(new Event("hashchange"));
    expect(c.state.page).toBe("bulletin");
    vi.spyOn(history, "replaceState").mockImplementation(() => {
      throw new Error("unsupported protocol");
    });
    expect(() => c.showPage("update")).not.toThrow();
  });

  it("保留文本输入行为并阻止页面拖拽和上下文菜单", async () => {
    await boot();
    for (const type of [
      "dragstart",
      "dragover",
      "drop",
      "selectstart",
      "contextmenu",
    ]) {
      const event = new Event(type, { bubbles: true, cancelable: true });
      document.body.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(true);
    }
    for (const type of ["selectstart", "contextmenu"]) {
      const event = new Event(type, { bubbles: true, cancelable: true });
      document.getElementById("set-install")!.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(false);
    }
    expect(key("Tab")).toBe(false);
    expect(key("a")).toBe(true);
  });

  it("窗口按钮、标题栏拖动、外链及原生命令错误", async () => {
    const c = await boot();
    await c.windowAction("minimize");
    await c.windowAction("maximize");
    await c.windowAction("close");
    expect(backend.invoke).toHaveBeenCalledWith("win_minimize");
    expect(backend.invoke).toHaveBeenCalledWith("win_toggle_maximize");
    expect(backend.invoke).toHaveBeenCalledWith("win_close");
    backend.invoke.mockRejectedValueOnce(new Error("window failed"));
    await c.windowAction("close");
    expect(noticeMessage()).toBe("window failed");
    await c.openUrl("https://example.com");
    expect(backend.invoke).toHaveBeenCalledWith("open_url", {
      url: "https://example.com",
    });
    expect(noticeMessage()).toBe("已在浏览器打开。");
    backend.invoke.mockRejectedValueOnce("cannot open");
    await c.openUrl("https://example.com");
    expect(noticeMessage()).toBe("cannot open");
    wrapper!
      .find("#titlebar")
      .element.dispatchEvent(
        new MouseEvent("mousedown", { button: 1, bubbles: true }),
      );
    button("launch-btn").dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true }),
    );
    expect(backend.invoke).not.toHaveBeenCalledWith("win_drag");
    await wrapper!.find("#titlebar").trigger("mousedown", { button: 0 });
    expect(backend.invoke).toHaveBeenCalledWith("win_drag");
    backend.native = false;
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    await c.openUrl("https://example.com");
    expect(open).toHaveBeenCalledWith(
      "https://example.com",
      "_blank",
      "noopener,noreferrer",
    );
    expect(noticeMessage()).toBe("预览模式：已在新标签页打开。");
    backend.invoke.mockClear();
    await c.windowAction("minimize");
    c.dragWindow(new MouseEvent("mousedown"));
    expect(backend.invoke).not.toHaveBeenCalled();
  });
});

describe("主视觉和资源", () => {
  it("保持循环间隙的画面, 离页暂停并保留当前帧", async () => {
    backend.data.settings = {
      ...settings,
      reduceMotion: false,
      heroMedia: "video",
    };
    const c = await boot();
    const video = document.getElementById("hero-video") as HTMLVideoElement;
    const canvas = document.getElementById("hero-canvas") as HTMLCanvasElement;
    const draw = vi.mocked(canvas.getContext("2d")!.drawImage);
    expect(video.src).toContain("blob:");
    expect(backend.invoke).toHaveBeenCalledWith("play_hero_video");
    expect(canvas.classList.contains("is-visible")).toBe(false);
    Object.defineProperties(video, {
      readyState: { value: 2, writable: true },
      paused: { value: false, writable: true },
      seeking: { value: false, writable: true },
      duration: { value: 13 },
      videoWidth: { value: 1920 },
      videoHeight: { value: 1080 },
    });
    Object.defineProperties(canvas, {
      clientWidth: { value: 800 },
      clientHeight: { value: 600 },
    });
    await vi.advanceTimersByTimeAsync(40);
    expect(canvas.classList.contains("is-visible")).toBe(true);
    expect(draw).toHaveBeenLastCalledWith(
      video,
      expect.closeTo(259.2),
      0,
      1440,
      1080,
      0,
      0,
      800,
      600,
    );
    vi.stubGlobal("devicePixelRatio", 4);
    await vi.advanceTimersByTimeAsync(40);
    expect(canvas.width).toBe(1440);
    expect(canvas.height).toBe(1080);
    vi.stubGlobal("devicePixelRatio", 1);
    await vi.advanceTimersByTimeAsync(40);
    video.currentTime = 12;
    backend.invoke.mockClear();
    await vi.advanceTimersByTimeAsync(40);
    expect(video.currentTime).toBe(0);
    expect(canvas.classList.contains("is-visible")).toBe(true);
    Object.defineProperty(video, "seeking", { value: true });
    draw.mockClear();
    await vi.advanceTimersByTimeAsync(100);
    expect(draw).not.toHaveBeenCalled();
    expect(canvas.classList.contains("is-visible")).toBe(true);
    Object.defineProperty(video, "seeking", { value: false });
    Object.defineProperty(video, "readyState", { value: 1 });
    await vi.advanceTimersByTimeAsync(100);
    expect(draw).not.toHaveBeenCalled();
    expect(canvas.classList.contains("is-visible")).toBe(true);
    Object.defineProperty(video, "readyState", { value: 2 });
    video.currentTime = 6;
    c.showPage("settings");
    await nextTick();
    expect(c.media.looping).toBe(false);
    expect(video.currentTime).toBe(6);
    expect(canvas.classList.contains("is-visible")).toBe(true);
    await vi.advanceTimersByTimeAsync(100);
    expect(draw).not.toHaveBeenCalled();
    Object.defineProperty(video, "seeking", { value: true });
    c.showPage("home");
    await flushPromises();
    await vi.advanceTimersByTimeAsync(40);
    expect(canvas.classList.contains("is-visible")).toBe(true);
    Object.defineProperty(video, "seeking", { value: false });
    await vi.advanceTimersByTimeAsync(40);
    expect(video.currentTime).toBe(6);
    expect(canvas.classList.contains("is-visible")).toBe(true);
    c.applyHeroMedia("photo");
    await nextTick();
    expect(canvas.classList.contains("is-visible")).toBe(false);
    c.applyHeroMedia("video");
    await flushPromises();
    c.applyReduceMotion(true);
    c.applyHeroMedia("video");
    await nextTick();
    expect(canvas.classList.contains("is-visible")).toBe(false);
    expect(
      vi
        .mocked(fetch)
        .mock.calls.filter(([url]) => String(url).includes("hero-loop")),
    ).toHaveLength(1);
  });

  it("窗口隐藏时停止解码, 恢复可见后从头播放", async () => {
    backend.data.settings = {
      ...settings,
      reduceMotion: false,
      heroMedia: "video",
    };
    const c = await boot();
    const video = document.getElementById("hero-video") as HTMLVideoElement;
    video.currentTime = 6;
    const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    document.dispatchEvent(new Event("visibilitychange"));
    await nextTick();
    expect(c.media.looping).toBe(false);
    expect(video.currentTime).toBe(0);
    hidden.mockReturnValue(false);
    document.dispatchEvent(new Event("visibilitychange"));
    await flushPromises();
    expect(c.media.looping).toBe(true);
    expect(video.currentTime).toBe(0);
  });

  it("资源或原生窗口接口失败时仍显示主页", async () => {
    vi.mocked(fetch).mockRejectedValue(new Error("asset missing"));
    backend.isMaximized.mockRejectedValue(new Error("window unavailable"));
    backend.data.settings = {
      ...settings,
      reduceMotion: false,
      heroMedia: "video",
    };
    const c = await boot();
    expect(c.state.bootstrap?.ok).toBe(true);
    expect(button("app").classList.contains("is-intro-done")).toBe(true);
    expect(button("hero-canvas").classList.contains("is-visible")).toBe(false);
  });

  it("不支持视频编码时保留照片", async () => {
    vi.mocked(HTMLMediaElement.prototype.canPlayType).mockReturnValue("");
    backend.data.settings = {
      ...settings,
      reduceMotion: false,
      heroMedia: "video",
    };
    await boot();
    expect(button("hero-canvas").classList.contains("is-visible")).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });
});

it("官方启动器视频通过本地流交给双缓冲循环", async () => {
  const official = "http://127.0.0.1:4321/video?token=abc&v=1";
  backend.background = { video: official, image: null };
  backend.data.settings = {
    ...settings,
    reduceMotion: false,
    heroMedia: "video",
  };
  const c = await boot();
  const video = document.getElementById("hero-video") as HTMLVideoElement;
  expect(video.src).toBe(official);
  expect(video.loop).toBe(false);
  expect(c.media.officialVideo).toBe(true);
  expect(c.media.cropX).toBe(0.5);
  expect(c.media.cropY).toBe(0.5);
  expect(
    vi
      .mocked(fetch)
      .mock.calls.some(([url]) => String(url).includes("hero-loop")),
  ).toBe(false);
});

it("官方静态壁纸用于照片模式并使用居中构图", async () => {
  const official = "http://127.0.0.1:4321/image?token=abc&v=1";
  backend.background = { video: null, image: official };
  backend.data.settings = {
    ...settings,
    reduceMotion: true,
    heroMedia: "photo",
  };
  const c = await boot();
  expect(c.media.officialImage).toBe(true);
  expect(c.media.image).toBe(official);
  const art = document.querySelector(".hero__art") as HTMLElement;
  expect(art.classList.contains("hero__art--official")).toBe(true);
  expect(art.style.backgroundImage).toContain(official);
});

it("没有官方视频时回退内置视频", async () => {
  backend.background = { video: null, image: null };
  backend.data.settings = {
    ...settings,
    reduceMotion: false,
    heroMedia: "video",
  };
  const c = await boot();
  expect(fetch).toHaveBeenCalledWith("assets/hero-loop.mp4", {
    cache: "no-store",
  });
  expect(c.media.officialVideo).toBe(false);
  expect(c.media.cropX).toBeCloseTo(0.54);
  expect(c.media.cropY).toBeCloseTo(0.38);
});

it("切换地区后重新读取背景素材", async () => {
  const cn = "http://127.0.0.1:4321/video?token=abc&v=1";
  const global = "http://127.0.0.1:4321/video?token=abc&v=2";
  backend.background = { video: cn, image: null };
  backend.data.settings = {
    ...settings,
    reduceMotion: false,
    heroMedia: "video",
  };
  const c = await boot();
  const video = document.getElementById("hero-video") as HTMLVideoElement;
  expect(video.src).toBe(cn);

  backend.background = { video: global, image: null };
  backend.data.launcherRoot = "/games/global/launcher";
  backend.data.settings = {
    ...backend.data.settings,
    distribution: "global",
    globalInstallRoot: "/games/global",
  };
  c.form.distribution = "global";
  c.form.globalInstallRoot = "/games/global";
  await c.redetect();
  await flushPromises();

  expect(video.src).toBe(global);
  expect(c.media.officialVideo).toBe(true);
  expect(c.media.cropX).toBe(0.5);
});

it("在视频结尾切换到预载的备播元素", async () => {
  backend.data.settings = {
    ...settings,
    reduceMotion: false,
    heroMedia: "video",
  };
  const c = await boot();
  const a = document.getElementById("hero-video") as HTMLVideoElement;
  const b = document.getElementById("hero-video-b") as HTMLVideoElement;
  const canvas = document.getElementById("hero-canvas") as HTMLCanvasElement;
  const draw = vi.mocked(canvas.getContext("2d")!.drawImage);
  Object.defineProperties(a, {
    readyState: { value: 2, writable: true },
    paused: { value: false, writable: true },
    seeking: { value: false, writable: true },
    duration: { value: 13 },
    videoWidth: { value: 1920 },
    videoHeight: { value: 1080 },
    currentTime: { value: 12, writable: true },
  });
  Object.defineProperties(b, {
    readyState: { value: 2, writable: true },
    paused: { value: false, writable: true },
    seeking: { value: false, writable: true },
    duration: { value: 13 },
    videoWidth: { value: 1920 },
    videoHeight: { value: 1080 },
    currentTime: { value: 0, writable: true },
  });
  Object.defineProperties(canvas, {
    clientWidth: { value: 800 },
    clientHeight: { value: 600 },
  });
  await vi.advanceTimersByTimeAsync(40);
  await vi.advanceTimersByTimeAsync(40);
  expect(draw).toHaveBeenLastCalledWith(
    b,
    expect.closeTo(259.2),
    0,
    1440,
    1080,
    0,
    0,
    800,
    600,
  );
  expect(c.media.looping).toBe(true);
});

it("组件事件和双向绑定将用户操作传给控制器", async () => {
  const c = await boot();
  await wrapper!.find('[data-page="settings"]').trigger("click");
  expect(c.state.page).toBe("settings");
  await wrapper!.find("#set-install").setValue(" /component-game ");
  await wrapper!.find("#set-install").trigger("change");
  await wrapper!.find("#set-theme").setValue("dark");
  expect(localStorage.getItem("nte.theme")).toBe("dark");
  await wrapper!.find("#set-hero-media").setValue("photo");
  await wrapper!.find("#set-motion").setValue(false);
  await flushPromises();
  expect(backend.data.settings.installRoot).toBe("/component-game");
  expect(backend.data.settings.theme).toBe("dark");
  expect(backend.data.settings.reduceMotion).toBe(false);
  // 目录切换触发的统一刷新会启动只读资源检查, 这里模拟它完成.
  backend.downloadHandler({ ...backend.downloadStatus, phase: "ready" });
  await flushPromises();
  await wrapper!.find("#settings-redetect").trigger("click");
  await flushPromises();
  expect(noticeMessage()).toBe("已重新检测环境。");
  for (const id of [
    "open-official",
    "open-contract",
    "open-privacy",
    "open-support",
  ]) {
    await wrapper!.find(`#${id}`).trigger("click");
    await flushPromises();
  }
  expect(
    backend.invoke.mock.calls.filter(([command]) => command === "open_url"),
  ).toHaveLength(4);
  for (const action of ["minimize", "maximize", "close"])
    await wrapper!.find(`[data-win="${action}"]`).trigger("click");
  await wrapper!.find("#setup-chip").trigger("click");
  expect(c.state.page).toBe("settings");
  await wrapper!.find("#home-status").trigger("click");
  expect(c.state.page).toBe("update");
  await wrapper!.find("#update-quick").trigger("click");
  await flushPromises();
  await wrapper!.find("#update-full").trigger("click");
  await flushPromises();
  expect(backend.invoke).toHaveBeenCalledWith("check_update", { full: true });
  await wrapper!.find('[data-page="bulletin"]').trigger("click");
  expect(c.state.page).toBe("bulletin");
  await wrapper!.find('[data-page="home"]').trigger("click");
  await wrapper!.find('[data-page="update"]').trigger("click");
  await flushPromises();
  expect(c.state.page).toBe("update");
  backend.downloadHandler({ ...backend.downloadStatus, phase: "ready" });
  await flushPromises();
  await wrapper!.find('[data-page="update"]').trigger("click");
  await wrapper!.find("#launch-btn").trigger("click");
  await vi.advanceTimersByTimeAsync(900);
  expect(c.state.game).toBe("running");
});

it("版本显示随后台快照刷新, 缺失值显示未知而非固定版本", async () => {
  const c = await boot();
  backend.data = bootstrap({
    appVersion: "9.8.7",
    gameVersion: "6.5.4",
    launcherVersion: "3.2.1",
    launcherBuild: "abcd",
  });
  await c.redetect();
  expect(c.displayVersion.value).toBe("9.8.7");
  backend.data = bootstrap({
    appVersion: "",
    gameVersion: "",
    launcherVersion: "",
    launcherBuild: "",
    gameExists: false,
  });
  await c.redetect();
  expect(c.displayVersion.value).toBe(APP_VERSION);
});

it("后台会话事件通过响应式绑定更新按钮文字、图标和禁用状态", async () => {
  await boot();
  backend.handler(session({ running: true }));
  await nextTick();
  expect(text("launch-btn")).toContain("关闭游戏");
  expect(wrapper!.find("#launch-btn use").attributes("href")).toBe("#i-power");
  expect(wrapper!.find("#launch-btn").classes()).toContain("is-running");
  backend.handler(session({ running: true, closing: true }));
  await nextTick();
  expect(text("launch-btn")).toContain("正在关闭");
  expect(button("launch-btn").disabled).toBe(true);
  expect(button("launch-btn").getAttribute("aria-busy")).toBe("true");
  backend.handler(session());
  // 关闭期间由命令维护中间状态, 避免后台事件覆盖反馈。
  expect(button("launch-btn").disabled).toBe(true);
});

it("组件卸载时释放原生订阅、视频 URL、定时器和全局事件", async () => {
  backend.data.settings = {
    ...settings,
    reduceMotion: false,
    heroMedia: "video",
  };
  const c = await boot();
  const url = c.media.url;
  const revoke = vi.spyOn(URL, "revokeObjectURL");
  await c.openUrl("https://example.com");
  expect(vi.getTimerCount()).toBeGreaterThan(0);
  wrapper!.unmount();
  wrapper = undefined;
  expect(backend.unlisten).toHaveBeenCalledOnce();
  expect(backend.downloadUnlisten).toHaveBeenCalledOnce();
  expect(backend.stopResize).toHaveBeenCalledOnce();
  expect(revoke).toHaveBeenCalledWith(url);
  expect(vi.getTimerCount()).toBe(0);
  backend.invoke.mockClear();
  window.dispatchEvent(new Event("focus"));
  key("s", { ctrlKey: true });
  await flushPromises();
  expect(backend.invoke).not.toHaveBeenCalled();
});

it("卸载时取消尚未触发的自动保存", async () => {
  const c = await boot();
  backend.invoke.mockClear();
  c.form.globalInstallRoot = "/pending";
  c.queueSettingsSave();
  wrapper!.unmount();
  wrapper = undefined;
  await vi.advanceTimersByTimeAsync(1000);
  await flushPromises();
  expect(backend.invoke).not.toHaveBeenCalledWith(
    "save_settings",
    expect.anything(),
  );
  expect(c.ui.saveState).toBe("idle");
});

it("卸载后返回的初始化快照不会重新触发界面或入场", async () => {
  let resolve!: (data: Bootstrap) => void;
  const pending = new Promise<Bootstrap>((done) => {
    resolve = done;
  });
  const invoke = backend.invoke.getMockImplementation()!;
  backend.invoke.mockImplementation(
    (
      command: string,
      args?: { settings: Settings; mode: "check" | "apply" },
    ) => (command === "bootstrap" ? pending : invoke(command, args)),
  );
  const c = await boot();
  wrapper!.unmount();
  wrapper = undefined;
  resolve(bootstrap());
  await flushPromises();
  expect(c.state.bootstrap).toBeNull();
  expect(c.ui.introDone).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["check_update", "save_settings", "bootstrap"] as const)(
  "卸载后忽略延迟的 %s 结果",
  async (command) => {
    const c = await boot();
    let resolve!: (value: unknown) => void;
    backend.invoke.mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    if (command === "save_settings") c.form.globalInstallRoot = "/late";
    const pending =
      command === "check_update"
        ? c.checkUpdate(true)
        : command === "save_settings"
          ? c.saveSettings()
          : c.redetect();
    await flushPromises();
    wrapper!.unmount();
    wrapper = undefined;
    resolve(command === "check_update" ? check({ changed: 2 }) : bootstrap());
    await pending;
    expect(c.state.bootstrap).toBeNull();
    expect(c.state.check).toBeNull();
    expect(Boolean(c.notifications.length)).toBe(false);
    expect(c.ui.saveState).toBe("idle");
    expect(c.ui.redetecting).toBe(false);
  },
);

it("启动反馈期间卸载会取消等待, 不再隐藏窗口", async () => {
  const c = await boot();
  c.handleLaunchAction();
  await flushPromises();
  wrapper!.unmount();
  wrapper = undefined;
  backend.invoke.mockClear();
  await vi.advanceTimersByTimeAsync(900);
  expect(c.state.game).toBe("idle");
  expect(backend.invoke).not.toHaveBeenCalled();
});

it("设置页面标题与按钮图标正确, 且能计算滚动模糊边缘状态", async () => {
  const c = await boot();
  c.showPage("settings");
  await nextTick();
  expect(text("settings-title")).toBe("设置");
  expect(wrapper!.find("#settings-redetect svg use").attributes("href")).toBe(
    "#i-arrows-clockwise",
  );
  expect(wrapper!.find("#settings-save").exists()).toBe(false);
  expect(wrapper!.find("#settings-autosave").exists()).toBe(false);
  expect(wrapper!.find("#page-settings").text()).not.toContain("自动保存");
  expect(wrapper!.find("#settings-form").attributes("data-save-state")).toBe(
    "idle",
  );

  const activePage = document.querySelector<HTMLElement>(".page.is-active")!;
  Object.defineProperty(activePage, "scrollHeight", {
    value: 1000,
    configurable: true,
  });
  Object.defineProperty(activePage, "clientHeight", {
    value: 600,
    configurable: true,
  });
  Object.defineProperty(activePage, "scrollTop", {
    value: 0,
    configurable: true,
    writable: true,
  });

  c.syncScrollEdges();
  expect(c.ui.scrollEdge.top).toBe(false);
  expect(c.ui.scrollEdge.bottom).toBe(true);

  activePage.scrollTop = 100;
  c.syncScrollEdges();
  expect(c.ui.scrollEdge.top).toBe(true);
  expect(c.ui.scrollEdge.bottom).toBe(true);

  activePage.scrollTop = 400;
  c.syncScrollEdges();
  expect(c.ui.scrollEdge.top).toBe(true);
  expect(c.ui.scrollEdge.bottom).toBe(false);

  c.showPage("home");
  await nextTick();
  c.syncScrollEdges();
  expect(c.ui.scrollEdge.top).toBe(false);
  expect(c.ui.scrollEdge.bottom).toBe(false);
});

it("保存先提交通用设置, 再提交已加载的签到偏好", async () => {
  const c = await boot();
  c.syncAttendance({
    accounts: [],
    preferences: {
      trigger: "off",
      time: "08:00",
      tasks: { ...defaultAttendanceTasks },
    },
  });
  c.attendance.loaded = true;
  c.attendanceForm.enabled = true;
  c.attendanceForm.trigger = "daily";
  c.attendanceForm.time = "20:30";
  c.form.globalInstallRoot = "/changed";
  const original = backend.invoke.getMockImplementation()!;
  backend.invoke.mockImplementation(async (command, args) =>
    command === "attendance_configure"
      ? {
          accounts: [],
          preferences: {
            trigger: "daily",
            time: "20:30",
            tasks: { ...defaultAttendanceTasks },
          },
        }
      : original(command, args),
  );
  backend.invoke.mockClear();
  await c.saveSettings();
  expect(backend.invoke.mock.calls.map(([command]) => command)).toEqual([
    "save_settings",
    "attendance_configure",
  ]);
  expect(backend.invoke).toHaveBeenLastCalledWith("attendance_configure", {
    preferences: {
      trigger: "daily",
      time: "20:30",
      tasks: { ...defaultAttendanceTasks },
    },
  });
  expect(c.ui.saveState).toBe("saved");
});

it("签到未加载时保存不提交签到偏好", async () => {
  const c = await boot();
  c.attendance.loaded = false;
  c.attendanceForm.enabled = true;
  c.form.globalInstallRoot = "/changed";
  backend.invoke.mockClear();
  await c.saveSettings();
  expect(backend.invoke.mock.calls.map(([command]) => command)).toEqual([
    "save_settings",
  ]);
});

it("签到保存失败明确提示并保留错误状态", async () => {
  const c = await boot();
  c.syncAttendance({
    accounts: [],
    preferences: {
      trigger: "off",
      time: "08:00",
      tasks: { ...defaultAttendanceTasks },
    },
  });
  c.attendance.loaded = true;
  c.attendanceForm.enabled = true;
  c.form.globalInstallRoot = "/changed";
  const original = backend.invoke.getMockImplementation()!;
  backend.invoke.mockImplementation(async (command, args) => {
    if (command === "attendance_configure")
      throw { message: "定时任务设置失败" };
    return original(command, args);
  });
  await c.saveSettings();
  expect(c.notifications[0]?.message).toBe(
    "签到设置保存失败: 定时任务设置失败",
  );
  expect(c.notifications[0]?.kind).toBe("error");
  expect(c.ui.saveState).toBe("error");
  expect(c.attendance.busy).toBe(false);
});

it("短信控制器保留未保存偏好, 发送失败复位并阻止并发", async () => {
  const c = await import("../../src/core/controller");
  Object.assign(c.attendance, { busy: false, running: false, error: "" });
  c.attendanceForm.enabled = true;
  c.attendanceForm.trigger = "boot";
  c.attendanceForm.time = "21:30";
  let finish!: (value: string) => void;
  backend.invoke.mockImplementationOnce(
    () =>
      new Promise<string>((resolve) => {
        finish = resolve;
      }),
  );
  const sending = c.sendAttendanceCode(" 13800138000 ");
  expect(c.attendance.busy).toBe(true);
  await expect(c.sendAttendanceCode("13800138000")).rejects.toThrow("正在进行");
  finish("test-device");
  await expect(sending).resolves.toBe("test-device");
  expect(backend.invoke).toHaveBeenCalledWith("attendance_send_code", {
    phone: "13800138000",
  });
  expect(c.attendance.busy).toBe(false);
  backend.invoke.mockRejectedValueOnce({ message: "发送失败" });
  await expect(c.sendAttendanceCode("13800138000")).rejects.toEqual({
    message: "发送失败",
  });
  expect(c.attendance.error).toBe("发送失败");
  expect(c.attendance.busy).toBe(false);
  backend.invoke.mockResolvedValueOnce({
    accounts: [],
    preferences: {
      trigger: "off",
      time: "08:00",
      tasks: { ...defaultAttendanceTasks },
    },
  });
  await c.loginAttendanceSms(" 13800138000 ", "123456", "test-device");
  expect(backend.invoke).toHaveBeenLastCalledWith("attendance_login_sms", {
    phone: "13800138000",
    code: "123456",
    deviceId: "test-device",
  });
  expect(c.attendanceForm).toMatchObject({
    enabled: true,
    trigger: "boot",
    time: "21:30",
    tasks: { ...defaultAttendanceTasks },
  });
  expect(c.attendance.error).toBe("");
  expect(c.attendance.busy).toBe(false);
  backend.invoke.mockRejectedValueOnce({ message: "验证码错误" });
  await expect(
    c.loginAttendanceSms("13800138000", "123456", "test-device"),
  ).rejects.toEqual({ message: "验证码错误" });
  expect(c.attendance.error).toBe("验证码错误");
  expect(c.attendance.busy).toBe(false);
});

it("区服切换后隔离公告缓存并丢弃旧请求", async () => {
  const c = await boot();
  localStorage.removeItem("nte.bulletins.v2.cn");
  const original = backend.invoke.getMockImplementation()!;
  let finishCn!: (feed: { items: []; errors: string[] }) => void;
  const globalFeed = { items: [], errors: ["global feed"] };
  backend.invoke.mockImplementation((command: string, args?: unknown) => {
    if (command === "fetch_bulletins") {
      if ((args as { distribution: string }).distribution === "cn")
        return new Promise((resolve) => {
          finishCn = resolve;
        });
      return Promise.resolve(globalFeed);
    }
    return original(command, args as Parameters<typeof original>[1]);
  });
  const pending = c.fetchBulletins(true, true);
  c.form.distribution = "global";
  c.form.globalInstallRoot = "/games/global";
  await c.saveSettings();
  await flushPromises();
  expect(backend.data.settings.launchMode).toBe("background");
  expect(c.state.bulletins).toEqual(globalFeed);
  finishCn({ items: [], errors: ["old cn feed"] });
  await pending;
  expect(c.state.bulletins).toEqual(globalFeed);
  expect(localStorage.getItem("nte.bulletins.v2.global")).toBeNull();
  expect(localStorage.getItem("nte.bulletins.v2.cn")).toBeNull();
  expect(wrapper!.findComponent({ name: "HomeGameRecords" }).exists()).toBe(
    false,
  );
});

it("公告来源失败不缓存, 下次读取会重新请求", async () => {
  const c = await boot();
  localStorage.removeItem("nte.bulletins.v2.cn");
  backend.invoke.mockResolvedValueOnce({
    items: [],
    errors: ["B站动态: 暂时不可用"],
  });
  await c.fetchBulletins(true, true);
  expect(c.state.bulletins?.errors).toHaveLength(1);
  expect(localStorage.getItem("nte.bulletins.v2.cn")).toBeNull();
  backend.invoke.mockResolvedValueOnce({ items: [], errors: [] });
  await c.fetchBulletins();
  expect(c.state.bulletins?.errors).toEqual([]);
  expect(localStorage.getItem("nte.bulletins.v2.cn")).toContain('"errors":[]');
  backend.invoke.mockResolvedValueOnce({ items: [], errors: ["临时失败"] });
  await c.fetchBulletins(true, true);
  backend.invoke.mockResolvedValueOnce({ items: [], errors: [] });
  await c.fetchBulletins();
  expect(c.state.bulletins?.errors).toEqual([]);
});
