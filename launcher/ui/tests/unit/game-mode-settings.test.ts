import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import GamescopeSettings from "../../src/components/GamescopeSettings.vue";
import CpuSettings from "../../src/components/CpuSettings.vue";
import GpuSettings from "../../src/components/GpuSettings.vue";
import BaseSelect from "../../src/components/BaseSelect.vue";
import { form, state, queueSettingsSave } from "../../src/core/controller";
import { defaultGamescope } from "../../src/core/gameMode";
import type {
  GpuConfig,
  GpuInfo,
  GameModeTuning,
} from "../../src/types/launcher";

const { invoke, notify } = vi.hoisted(() => ({
  invoke: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../../src/core/api", () => ({ invoke }));
vi.mock("../../src/core/notifications", () => ({ notify }));
vi.mock("../../src/core/controller", async () => {
  const { reactive } = await import("vue");
  const { defaultGamescope } = await import("../../src/core/gameMode");
  return {
    form: reactive({
      theme: "light",
      reduceMotion: true,
      gameMode: { performance: false, gamescope: { ...defaultGamescope } },
    }),
    state: reactive({ page: "game-mode", game: "idle", bootstrap: {} }),
    queueSettingsSave: vi.fn(),
  };
});
const modal = {
  props: ["open"],
  template: '<section v-if="open" role="dialog"><slot /></section>',
};
let wrapper: ReturnType<typeof mount>;
let info: GpuInfo;
let tuning: GameModeTuning;
beforeEach(() => {
  tuning = { pinCores: null, parkCores: null, renice: null, ioprio: null };
  Object.assign(form.gameMode.gamescope, defaultGamescope);
  form.gameMode.performance = true;
  state.page = "game-mode";
  state.game = "idle";
  info = {
    config: {
      enabled: false,
      device: 1,
      vendor: "nvidia",
      coreOffset: -1,
      memoryOffset: -1,
      powerMode: -1,
      amdLevel: "auto",
    },
    devices: [
      { id: 1, vendor: "nvidia" },
      { id: 2, vendor: "amd" },
    ],
  };
  invoke
    .mockReset()
    .mockImplementation(
      async (command: string, args?: { config: GpuConfig | GameModeTuning }) =>
        command === "game_mode_tuning_info"
          ? structuredClone(tuning)
          : command === "game_mode_tuning_apply"
            ? structuredClone(args!.config)
            : command === "game_mode_gpu_apply"
              ? {
                  ...structuredClone(info),
                  config: structuredClone(args!.config),
                }
              : structuredClone(info),
    );
  notify.mockClear();
  vi.mocked(queueSettingsSave).mockClear();
});

it("CPU 与优先级经确认后应用, 单个核心编号不与布尔开关混淆", async () => {
  wrapper = mount(CpuSettings, {
    props: { available: true },
    global: { stubs: { BaseModal: modal } },
  });
  await flushPromises();
  const selects = wrapper.findAllComponents(BaseSelect);
  selects[0].vm.$emit("change", "custom");
  selects[1].vm.$emit("change", "no");
  selects[2].vm.$emit("change", "10");
  selects[3].vm.$emit("change", "0");
  await flushPromises();
  expect(button("应用配置").attributes("disabled")).toBeDefined();
  await wrapper.get("#cpu-pinCores-list").setValue("0");
  expect(wrapper.find("#cpu-pinCores-list").exists()).toBe(true);
  await button("应用配置").trigger("click");
  await button("取消").trigger("click");
  expect(invoke).toHaveBeenCalledTimes(1);
  await button("应用配置").trigger("click");
  await button("确认并授权").trigger("click");
  await flushPromises();
  expect(invoke).toHaveBeenCalledWith("game_mode_tuning_apply", {
    config: { pinCores: "0-0", parkCores: "no", renice: 10, ioprio: "0" },
  });
  expect(notify).toHaveBeenCalledWith("CPU 与优先级配置已应用");
  expect(form.gameMode.performance).toBe(true);
  expect(button("应用配置").attributes("disabled")).toBeDefined();
});

it("CPU 配置失败使用统一通知, 未授权或游戏运行时保留设置", async () => {
  invoke.mockRejectedValueOnce(new Error("读取 CPU 配置失败"));
  wrapper = mount(CpuSettings, {
    props: { available: true },
    global: { stubs: { BaseModal: modal } },
  });
  await flushPromises();
  expect(notify).toHaveBeenCalledWith("读取 CPU 配置失败", "error", 6500, {
    key: "game-mode-tuning",
  });
  await (wrapper.vm as unknown as { reload(): Promise<void> }).reload();
  wrapper.findAllComponents(BaseSelect)[0].vm.$emit("change", "yes");
  await flushPromises();
  invoke.mockRejectedValueOnce(new Error("授权已取消"));
  await button("应用配置").trigger("click");
  await button("确认并授权").trigger("click");
  await flushPromises();
  expect(notify).toHaveBeenCalledWith("授权已取消", "error", 6500, {
    key: "game-mode-tuning",
  });
  expect(button("应用配置").attributes("disabled")).toBeUndefined();
  state.game = "running";
  await flushPromises();
  expect(button("应用配置").attributes("disabled")).toBeDefined();
  state.game = "idle";
  await wrapper.setProps({ available: false });
  expect(wrapper.get(".game-mode-system").classes()).toContain(
    "is-unavailable",
  );
  expect(wrapper.get("fieldset").attributes("disabled")).toBeDefined();
});
afterEach(() => wrapper?.unmount());
function button(text: string) {
  return wrapper.findAll("button").find((button) => button.text() === text)!;
}

it("Gamescope 保存窗口、分辨率、帧率和缩放选项", async () => {
  wrapper = mount(GamescopeSettings, { props: { available: true } });
  expect(wrapper.find("#gamescope-fps").exists()).toBe(false);
  await wrapper.get("#game-mode-gamescope").setValue(true);
  await wrapper.get("#gamescope-gameWidth").setValue("1280");
  await wrapper.get("#gamescope-gameHeight").setValue("720");
  await wrapper.get("#gamescope-fps").setValue("60");
  wrapper.findAllComponents(BaseSelect)[0].vm.$emit("change", "borderless");
  wrapper.findAllComponents(BaseSelect)[1].vm.$emit("change", "fsr");
  await flushPromises();
  expect(form.gameMode.gamescope).toEqual({
    ...defaultGamescope,
    enabled: true,
    gameWidth: 1280,
    gameHeight: 720,
    fps: 60,
    windowMode: "borderless",
    scaling: "fsr",
  });
  expect(queueSettingsSave).toHaveBeenCalledTimes(6);
  await wrapper.get("#gamescope-fps").setValue("1001");
  expect(form.gameMode.gamescope.fps).toBe(60);
  await wrapper.setProps({ available: false });
  expect(wrapper.get("fieldset").attributes("disabled")).toBeDefined();
  expect(wrapper.find(".is-unavailable").exists()).toBe(true);
});

it("GPU 配置只有确认后才申请授权", async () => {
  wrapper = mount(GpuSettings, {
    props: { available: true },
    global: { stubs: { BaseModal: modal } },
  });
  await flushPromises();
  expect(button("应用配置").attributes("disabled")).toBeDefined();
  await wrapper.get("#gpu-enabled").setValue(true);
  await wrapper.get("#gpu-core").setValue("75");
  await button("应用配置").trigger("click");
  expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
  await button("取消").trigger("click");
  await flushPromises();
  expect(invoke).toHaveBeenCalledTimes(1);
  await button("应用配置").trigger("click");
  await button("确认并授权").trigger("click");
  await flushPromises();
  expect(invoke).toHaveBeenCalledWith("game_mode_gpu_apply", {
    config: { ...info.config, enabled: true, coreOffset: 75 },
  });
  expect(notify).toHaveBeenCalledWith("GPU 配置已应用");
  expect(form.gameMode.performance).toBe(true);
  expect(queueSettingsSave).not.toHaveBeenCalled();
  expect(button("应用配置").attributes("disabled")).toBeDefined();
});

it("GPU 读取或授权失败通过统一通知报告, 保留修改供重试", async () => {
  invoke.mockRejectedValueOnce(new Error("读取配置失败"));
  wrapper = mount(GpuSettings, {
    props: { available: true },
    global: { stubs: { BaseModal: modal } },
  });
  await flushPromises();
  expect(notify).toHaveBeenCalledWith("读取配置失败", "error", 6500, {
    key: "game-mode-gpu",
  });
  await (wrapper.vm as unknown as { reload(): Promise<void> }).reload();
  await wrapper.get("#gpu-enabled").setValue(true);
  invoke.mockRejectedValueOnce(new Error("管理员授权已取消或被拒绝"));
  await button("应用配置").trigger("click");
  await button("确认并授权").trigger("click");
  await flushPromises();
  expect(notify).toHaveBeenCalledWith(
    "管理员授权已取消或被拒绝",
    "error",
    6500,
    { key: "game-mode-gpu" },
  );
  expect(button("应用配置").attributes("disabled")).toBeUndefined();
});

it("选择 AMD 显卡只显示性能等级, 运行游戏时阻止修改", async () => {
  wrapper = mount(GpuSettings, {
    props: { available: true },
    global: { stubs: { BaseModal: modal } },
  });
  await flushPromises();
  await wrapper.get("#gpu-enabled").setValue(true);
  wrapper.findAllComponents(BaseSelect)[0].vm.$emit("change", "2");
  await flushPromises();
  expect(wrapper.find("#gpu-core").exists()).toBe(false);
  expect(wrapper.find("#gpu-amd-trigger").exists()).toBe(true);
  state.game = "running";
  await flushPromises();
  expect(wrapper.get("fieldset").attributes("disabled")).toBeDefined();
  expect(wrapper.get<HTMLInputElement>("#gpu-enabled").element.disabled).toBe(
    true,
  );
});

it("GameMode 关闭时 CPU 和 GPU 整体禁用, 重新开启保留配置", async () => {
  form.gameMode.performance = false;
  tuning.renice = 10;
  info.config.enabled = true;
  info.config.coreOffset = 75;
  wrapper = mount({
    components: { CpuSettings, GpuSettings },
    template:
      '<div><CpuSettings :available="true"/><GpuSettings :available="true"/></div>',
  });
  await flushPromises();
  expect(wrapper.get(".game-mode-system").classes()).toContain(
    "is-unavailable",
  );
  expect(wrapper.get(".game-mode-gpu").classes()).toContain("is-unavailable");
  for (const fieldset of wrapper.findAll("fieldset"))
    expect(fieldset.attributes("disabled")).toBeDefined();
  expect(wrapper.get<HTMLInputElement>("#gpu-enabled").element.disabled).toBe(
    true,
  );
  form.gameMode.performance = true;
  await flushPromises();
  expect(wrapper.get("#gpu-core").element).toHaveProperty("value", "75");
  expect(wrapper.get("#cpu-renice-trigger").text()).toContain("10");
  expect(wrapper.get(".game-mode-system").classes()).not.toContain(
    "is-unavailable",
  );
  expect(wrapper.get(".game-mode-gpu").classes()).not.toContain(
    "is-unavailable",
  );
  form.gameMode.performance = false;
  await flushPromises();
  for (const fieldset of wrapper.findAll("fieldset"))
    expect(fieldset.attributes("disabled")).toBeDefined();
  expect(
    invoke.mock.calls.every(([command]) => command.endsWith("_info")),
  ).toBe(true);
  expect(queueSettingsSave).not.toHaveBeenCalled();
});
