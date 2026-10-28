import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import GameModePage from "../../src/pages/GameModePage.vue";
import { form, state, ui, queueSettingsSave } from "../../src/core/controller";
import { bootstrap } from "./fixtures";
import type { GameModeStatus } from "../../src/types/launcher";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../src/core/api", () => ({ invoke, isTauri: () => true }));
vi.mock("../../src/core/controller", async () => {
  const { reactive } = await import("vue");
  return {
    form: reactive({
      reduceMotion: true,
      gameMode: {
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
  invoke.mockReset().mockImplementation(async () => structuredClone(result));
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
  expect(wrapper.get('[role="alert"]').text()).toBe("检测失败, 请重试.");
  expect(
    wrapper.get<HTMLInputElement>("#game-mode-inhibit").element.disabled,
  ).toBe(true);
  await wrapper.get(".page-head button").trigger("click");
  await flushPromises();
  expect(wrapper.find('[role="alert"]').exists()).toBe(false);
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
  await vi.advanceTimersByTimeAsync(2500);
  await flushPromises();
  expect(wrapper.text()).toContain("启用失败, 请重试.");
  state.page = "settings";
  await flushPromises();
  const count = invoke.mock.calls.length;
  await vi.advanceTimersByTimeAsync(5000);
  expect(invoke.mock.calls.length).toBe(count);
});
