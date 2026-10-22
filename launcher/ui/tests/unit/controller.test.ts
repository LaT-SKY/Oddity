import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { nextTick } from "vue";
import { APP_VERSION } from "../../src/core/version";
import { bootstrap, check, session, settings } from "./fixtures";
import type {
  Bootstrap,
  CheckResult,
  GameStatus,
  Settings,
} from "../../src/types/launcher";

const backend = vi.hoisted(() => ({
  native: true,
  unlisten: vi.fn(),
  stopResize: vi.fn(),
  invoke: vi.fn(),
  handler: (_status: GameStatus) => {},
  data: {} as Bootstrap,
  result: {} as CheckResult,
  status: {} as GameStatus,
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
}));
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
  backend.stopResize.mockClear();
  backend.data = bootstrap();
  backend.result = check();
  backend.status = session();
  backend.invoke
    .mockReset()
    .mockImplementation(
      async (command: string, args?: { settings: Settings }) => {
        switch (command) {
          case "bootstrap":
            return structuredClone(backend.data);
          case "check_update":
            return structuredClone(backend.result);
          case "game_status":
            return structuredClone(backend.status);
          case "save_settings":
            backend.data.settings = { ...args!.settings };
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

async function boot() {
  const controller = await import("../../src/core/controller");
  const { default: App } = await import("../../src/App.vue");
  wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  await vi.advanceTimersByTimeAsync(1000);
  await flushPromises();
  return controller;
}
const text = (id: string) => document.getElementById(id)?.textContent;
const button = (id: string) => document.getElementById(id) as HTMLButtonElement;
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
      bridgeRoot: "",
      prefix: "",
      protonPath: "",
      settings: { ...settings, theme: "", heroMedia: "", launchMode: "" },
    });
    const c = await boot();
    expect(button("setup-chip").hidden).toBe(false);
    expect(text("toast")).toBe("找不到游戏");
    expect(c.displayVersion.value).toBe(APP_VERSION);
    expect(text("update-state-text")).toBe("等待检查");
    expect(c.form.launchMode).toBe("background");
    expect(c.form.theme).toBe("light");
  });

  it("启动失败仍完成入场并允许手动重新检测", async () => {
    backend.invoke
      .mockRejectedValueOnce(new Error("status unavailable"))
      .mockRejectedValueOnce(new Error("bootstrap failed"));
    const c = await boot();
    expect(text("toast")).toBe("bootstrap failed");
    expect(button("app").classList.contains("is-intro-done")).toBe(true);
    await c.redetect();
    expect(c.state.bootstrap?.ok).toBe(true);
    expect(text("toast")).toBe("已重新检测环境。");
    backend.invoke.mockRejectedValueOnce("redetect failed");
    await c.redetect();
    expect(text("toast")).toBe("redetect failed");
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

  it("保存时清理路径, 持久化外观, 失败后恢复按钮", async () => {
    const c = await boot();
    Object.assign(c.form, {
      installRoot: " /new ",
      bridgeRoot: " ",
      prefix: "/prefix2",
      protonPath: "/proton2",
      launchMode: "",
      theme: "dark",
      reduceMotion: false,
      heroMedia: "photo",
    });
    await c.saveSettings();
    expect(backend.invoke).toHaveBeenCalledWith("save_settings", {
      settings: {
        installRoot: "/new",
        bridgeRoot: null,
        prefix: "/prefix2",
        protonPath: "/proton2",
        launchMode: "background",
        language: "zh-CN",
        theme: "dark",
        reduceMotion: false,
        heroMedia: "photo",
        minimizeToTrayOnClose: true,
      },
    });
    expect(text("toast")).toBe("设置已保存.");
    Object.assign(c.form, { installRoot: "", prefix: "", protonPath: "" });
    backend.invoke.mockRejectedValueOnce(new Error("disk full"));
    await c.saveSettings();
    expect(text("toast")).toBe("disk full");
    expect(button("settings-save").disabled).toBe(false);
    await vi.advanceTimersByTimeAsync(6000);
    expect(button("toast").classList.contains("is-visible")).toBe(false);
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
});

describe("更新检查", () => {
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
    expect(text("toast")).toBe("CDN offline");
    expect(button("update-quick").disabled).toBe(false);
    backend.invoke.mockRejectedValueOnce("silent error");
    await c.checkUpdate(false, true);
    expect(text("toast")).toBe("CDN offline");
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
    expect(text("toast")).toBe("游戏已退出。");
  });

  it("进程在反馈期间退出时不收起窗口", async () => {
    const c = await boot();
    c.handleLaunchAction();
    await flushPromises();
    backend.status = session();
    await vi.advanceTimersByTimeAsync(900);
    expect(c.state.game).toBe("idle");
    expect(text("toast")).toBe("游戏进程已退出，启动未完成。");
    expect(backend.invoke).not.toHaveBeenCalledWith("win_hide");
  });

  it("启动命令失败时用后端状态恢复按钮", async () => {
    const c = await boot();
    backend.invoke.mockRejectedValueOnce(new Error("spawn failed"));
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("idle");
    expect(text("toast")).toBe("spawn failed");
    backend.status = session({ running: true });
    backend.invoke.mockRejectedValueOnce("spawn uncertain");
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("running");
    expect(text("toast")).toBe("spawn uncertain");
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
    expect(text("toast")).toBe("游戏已关闭。");
  });

  it("关闭命令失败时同步仍运行、已退出和未知状态", async () => {
    const c = await boot();
    backend.status = session({ running: true });
    backend.handler(backend.status);
    backend.invoke.mockRejectedValueOnce(new Error("close failed"));
    c.handleLaunchAction();
    await flushPromises();
    expect(c.state.game).toBe("running");
    expect(text("toast")).toBe("close failed");
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
    expect(text("toast")).toBe("浏览器预览：模拟启动流程。");
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
    backend.invoke.mockClear();
    key("s", { ctrlKey: true });
    expect(backend.invoke).not.toHaveBeenCalledWith(
      "save_settings",
      expect.anything(),
    );
    key("4", { ctrlKey: true });
    key("s", { metaKey: true });
    await flushPromises();
    expect(backend.invoke).toHaveBeenCalledWith(
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
    expect(text("toast")).toBe("window failed");
    await c.openUrl("https://example.com");
    expect(backend.invoke).toHaveBeenCalledWith("open_url", {
      url: "https://example.com",
    });
    expect(text("toast")).toBe("已在浏览器打开。");
    backend.invoke.mockRejectedValueOnce("cannot open");
    await c.openUrl("https://example.com");
    expect(text("toast")).toBe("cannot open");
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
    expect(text("toast")).toBe("预览模式：已在新标签页打开。");
    backend.invoke.mockClear();
    await c.windowAction("minimize");
    c.dragWindow(new MouseEvent("mousedown"));
    expect(backend.invoke).not.toHaveBeenCalled();
  });
});

describe("主视觉和资源", () => {
  it("保持循环间隙的画面, 离页后回到首帧再绘制", async () => {
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
    expect(backend.invoke).toHaveBeenCalledWith("play_hero_video");
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
    expect(video.currentTime).toBe(0);
    expect(canvas.classList.contains("is-visible")).toBe(false);
    await vi.advanceTimersByTimeAsync(100);
    expect(draw).not.toHaveBeenCalled();
    Object.defineProperty(video, "seeking", { value: true });
    c.showPage("home");
    await flushPromises();
    await vi.advanceTimersByTimeAsync(40);
    expect(canvas.classList.contains("is-visible")).toBe(false);
    Object.defineProperty(video, "seeking", { value: false });
    await vi.advanceTimersByTimeAsync(40);
    expect(video.currentTime).toBe(0);
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

it("组件事件和双向绑定将用户操作传给控制器", async () => {
  const c = await boot();
  await wrapper!.find('[data-page="settings"]').trigger("click");
  expect(c.state.page).toBe("settings");
  await wrapper!.find("#set-install").setValue(" /component-game ");
  await wrapper!.find("#set-theme").setValue("dark");
  expect(localStorage.getItem("nte.theme")).toBe("dark");
  await wrapper!.find("#set-hero-media").setValue("photo");
  await wrapper!.find("#set-motion").setValue(false);
  await wrapper!.find("#settings-form").trigger("submit");
  await flushPromises();
  expect(backend.data.settings.installRoot).toBe("/component-game");
  expect(backend.data.settings.theme).toBe("dark");
  expect(backend.data.settings.reduceMotion).toBe(false);
  await wrapper!.find("#settings-redetect").trigger("click");
  await flushPromises();
  expect(text("toast")).toBe("已重新检测环境。");
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
  await wrapper!.find("#check-btn").trigger("click");
  await flushPromises();
  expect(c.state.page).toBe("update");
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
  expect(wrapper!.find(".bulletin-number").text()).toBe("6.5.4");
  expect(wrapper!.find(".bulletin-copy").text()).toContain(
    "3.2.1 / build abcd",
  );
  backend.data = bootstrap({
    appVersion: "",
    gameVersion: "",
    launcherVersion: "",
    launcherBuild: "",
    gameExists: false,
  });
  await c.redetect();
  expect(c.displayVersion.value).toBe(APP_VERSION);
  expect(wrapper!.find(".bulletin-number").text()).toBe("未检测");
  expect(wrapper!.find(".bulletin-copy").text()).toContain(
    "尚未检测到游戏资源",
  );
  expect(wrapper!.find(".bulletin-copy").text()).toContain(
    "启动器版本 未知 / build 未知",
  );
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
  expect(backend.stopResize).toHaveBeenCalledOnce();
  expect(revoke).toHaveBeenCalledWith(url);
  expect(vi.getTimerCount()).toBe(0);
  backend.invoke.mockClear();
  window.dispatchEvent(new Event("focus"));
  key("s", { ctrlKey: true });
  await flushPromises();
  expect(backend.invoke).not.toHaveBeenCalled();
});

it("卸载后返回的初始化快照不会重新触发界面或入场", async () => {
  let resolve!: (data: Bootstrap) => void;
  const pending = new Promise<Bootstrap>((done) => {
    resolve = done;
  });
  const invoke = backend.invoke.getMockImplementation()!;
  backend.invoke.mockImplementation(
    (command: string, args?: { settings: Settings }) =>
      command === "bootstrap" ? pending : invoke(command, args),
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
    const pending =
      command === "check_update"
        ? c.checkUpdate(true)
        : command === "save_settings"
          ? c.saveSettings()
          : c.redetect();
    wrapper!.unmount();
    wrapper = undefined;
    resolve(command === "check_update" ? check({ changed: 2 }) : bootstrap());
    await pending;
    expect(c.state.bootstrap).toBeNull();
    expect(c.state.check).toBeNull();
    expect(c.ui.toast.visible).toBe(false);
    expect(c.ui.saving).toBe(false);
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
  expect(wrapper!.find("#settings-save svg use").attributes("href")).toBe(
    "#i-floppy-disk",
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

it("页面保存先保存通用设置, 再保存已加载的签到偏好", async () => {
  const c = await boot();
  c.syncAttendance({
    accounts: [],
    preferences: { trigger: "off", time: "08:00" },
  });
  c.attendance.loaded = true;
  c.attendanceForm.enabled = true;
  c.attendanceForm.trigger = "daily";
  c.attendanceForm.time = "20:30";
  const original = backend.invoke.getMockImplementation()!;
  backend.invoke.mockImplementation(async (command, args) =>
    command === "attendance_configure"
      ? { accounts: [], preferences: { trigger: "daily", time: "20:30" } }
      : original(command, args),
  );
  backend.invoke.mockClear();
  await c.saveSettings();
  expect(backend.invoke.mock.calls.map(([command]) => command)).toEqual([
    "save_settings",
    "attendance_configure",
  ]);
  expect(backend.invoke).toHaveBeenLastCalledWith("attendance_configure", {
    preferences: { trigger: "daily", time: "20:30" },
  });
  expect(c.ui.toast.message).toBe("设置已保存.");
});

it("签到未加载时页面保存不提交签到偏好", async () => {
  const c = await boot();
  backend.invoke.mockClear();
  await c.saveSettings();
  expect(backend.invoke.mock.calls.map(([command]) => command)).toEqual([
    "save_settings",
  ]);
});

it("签到保存失败明确提示通用设置已经保存", async () => {
  const c = await boot();
  c.syncAttendance({
    accounts: [],
    preferences: { trigger: "off", time: "08:00" },
  });
  c.attendance.loaded = true;
  c.attendanceForm.enabled = true;
  const original = backend.invoke.getMockImplementation()!;
  backend.invoke.mockImplementation(async (command, args) => {
    if (command === "attendance_configure")
      throw { message: "定时任务设置失败" };
    return original(command, args);
  });
  await c.saveSettings();
  expect(c.ui.toast.message).toBe(
    "设置已保存, 签到设置保存失败: 定时任务设置失败",
  );
  expect(c.ui.toast.kind).toBe("error");
  expect(c.ui.saving).toBe(false);
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
    preferences: { trigger: "off", time: "08:00" },
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
