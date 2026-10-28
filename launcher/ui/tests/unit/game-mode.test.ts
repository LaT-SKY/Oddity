import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import GameModePage from "../../src/pages/GameModePage.vue";
import { form, state, ui, queueSettingsSave } from "../../src/core/controller";
import { bootstrap } from "./fixtures";
import type { GameModeStatus } from "../../src/types/launcher";

const { invoke, notify } = vi.hoisted(() => ({
  invoke: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../../src/core/notifications", () => ({ notify }));
vi.mock("../../src/core/api", () => ({ invoke, isTauri: () => true }));
vi.mock("../../src/core/controller", async () => {
  const { reactive } = await import("vue");
  return {
    form: reactive({
      reduceMotion: true,
      gameMode: {
        gamescope: {
          enabled: false,
          windowMode: "fullscreen",
          gameWidth: 0,
          gameHeight: 0,
          outputWidth: 0,
          outputHeight: 0,
          fps: 0,
          scaling: "linear",
        },
        performance: false,
        inhibit: false,
        notifications: false,
        overlay: false,
      },
    }),
    state: reactive({ page: "game-mode", game: "idle", bootstrap: null }),
    ui: reactive({ saveState: "idle" }),
    queueSettingsSave: vi.fn(),
  };
});
let wrapper: ReturnType<typeof mount>;
let result: GameModeStatus;
beforeEach(() => {
  notify.mockClear();
  state.page = "game-mode";
  state.game = "idle";
  state.bootstrap = bootstrap();
  ui.saveState = "idle";
  Object.assign(form.gameMode, {
    performance: false,
    inhibit: false,
    notifications: false,
    overlay: false,
  });
  vi.mocked(queueSettingsSave).mockClear();
  result = {
    capabilities: (
      ["performance", "inhibit", "notifications", "overlay"] as const
    ).map((id) => ({ id, available: true, state: "ready", detail: "可请求" })),
    session: null,
  };
  invoke.mockReset().mockImplementation(async (command: string) =>
    command === "game_mode_tuning_info"
      ? { pinCores: null, parkCores: null, renice: null, ioprio: null }
      : command === "game_mode_gpu_info"
        ? {
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
          }
        : structuredClone(result),
  );
});
afterEach(() => wrapper?.unmount());

it("独立保存游戏模式选项", async () => {
  result.session = result.capabilities.map((item) => ({
    ...item,
    state: "disabled",
    detail: "本轮未启用",
  }));
  wrapper = mount(GameModePage);
  await flushPromises();
  await wrapper.get("#game-mode-performance").setValue(true);
  expect(form.gameMode.performance).toBe(true);
  expect(queueSettingsSave).toHaveBeenCalledExactlyOnceWith(0);
  ui.saveState = "saved";
  await flushPromises();
  expect(wrapper.get('[role="status"]').text()).toBe("已保存");
});

it("按分类浏览, 搜索定位设置并支持键盘清除", async () => {
  wrapper = mount(GameModePage, { attachTo: document.body });
  await flushPromises();
  await wrapper
    .get('[data-category="general"]')
    .trigger("keydown", { key: "End" });
  expect(
    wrapper.get('[data-category="gamescope"]').attributes("aria-current"),
  ).toBe("true");
  await wrapper
    .get('[data-category="gamescope"]')
    .trigger("keydown", { key: "Home" });
  expect(
    wrapper.get('[data-category="general"]').attributes("aria-current"),
  ).toBe("true");
  await wrapper.get("#game-mode-search").setValue("renice");
  expect(wrapper.findAll(".settings-result")).toHaveLength(1);
  await wrapper
    .get("#game-mode-search")
    .trigger("keydown", { key: "ArrowDown" });
  expect(document.activeElement).toBe(wrapper.get(".settings-result").element);
  const target = wrapper.get('[data-game-setting="cpu-renice"]')
    .element as HTMLElement;
  target.scrollIntoView = vi.fn();
  await wrapper.get(".settings-result").trigger("click");
  await flushPromises();
  expect(target.scrollIntoView).toHaveBeenCalled();
  expect(wrapper.get('[data-category="cpu"]').attributes("aria-current")).toBe(
    "true",
  );
  expect(document.activeElement).toBe(target);
  await wrapper.get("#game-mode-search").setValue("不存在的设置");
  expect(wrapper.text()).toContain("未找到相关设置");
  await wrapper.get("#game-mode-search").trigger("keydown", { key: "Escape" });
  expect(wrapper.find("#game-mode-results").exists()).toBe(false);
  expect(state.page).toBe("game-mode");
});

it("缺少能力时阻止开启, 仍允许关闭已有配置", async () => {
  result.capabilities[0].available = false;
  result.capabilities[0].state = "unavailable";
  result.capabilities[0].detail = "未连接到 GameMode 服务";
  wrapper = mount(GameModePage);
  await flushPromises();
  expect(
    wrapper.get<HTMLInputElement>("#game-mode-performance").element.disabled,
  ).toBe(true);
  expect(wrapper.text()).toContain("需安装并启动 GameMode.");
  form.gameMode.performance = true;
  await flushPromises();
  expect(
    wrapper.get<HTMLInputElement>("#game-mode-performance").element.disabled,
  ).toBe(false);
  await wrapper.get("#game-mode-performance").setValue(false);
  expect(form.gameMode.performance).toBe(false);
});

it("检测失败时重试恢复控件", async () => {
  invoke.mockRejectedValueOnce(new Error("总线断开"));
  wrapper = mount(GameModePage);
  await flushPromises();
  expect(notify).toHaveBeenCalledWith("总线断开", "error", 6500, {
    key: "game-mode-detect",
  });
  expect(
    wrapper.get<HTMLInputElement>("#game-mode-inhibit").element.disabled,
  ).toBe(true);
  await wrapper.get(".page-head button").trigger("click");
  await flushPromises();
  expect(invoke).toHaveBeenCalledWith("game_mode_status");
  expect(
    invoke.mock.calls.filter(([command]) => command === "game_mode_gpu_info"),
  ).toHaveLength(2);
  expect(
    invoke.mock.calls.filter(
      ([command]) => command === "game_mode_tuning_info",
    ),
  ).toHaveLength(2);
  expect(
    wrapper.get<HTMLInputElement>("#game-mode-inhibit").element.disabled,
  ).toBe(false);
});

it("对应功能请求失败时提示重试, 离开页面后停止轮询", async () => {
  state.game = "running";
  result.session = result.capabilities.map((item) => ({
    ...item,
    state: "requested",
    detail: "已请求, 等待桌面处理",
  }));
  wrapper = mount(GameModePage);
  await flushPromises();
  result.session[0].state = "error";
  result.session[0].detail = "GameMode 拒绝请求";
  await vi.advanceTimersByTimeAsync(2500);
  await flushPromises();
  expect(notify).toHaveBeenCalledWith("GameMode 拒绝请求", "error", 6500, {
    key: "game-mode-performance",
  });
  await vi.advanceTimersByTimeAsync(2500);
  expect(notify).toHaveBeenCalledTimes(1);
  state.page = "settings";
  await flushPromises();
  const count = invoke.mock.calls.length;
  await vi.advanceTimersByTimeAsync(5000);
  expect(invoke.mock.calls.length).toBe(count);
});
