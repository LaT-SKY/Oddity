import { afterEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { ref } from "vue";
import GamepadFilePicker from "../../../launcher/ui/src/components/GamepadFilePicker.vue";
import PlaytimeHeatmap from "../../../launcher/ui/src/components/playtime/PlaytimeHeatmap.vue";
import PlaytimeMonthlyChart from "../../../launcher/ui/src/components/playtime/PlaytimeMonthlyChart.vue";
import {
  fileRequest,
  type FileListing,
} from "../../../launcher/ui/src/core/filePicker";
import { buildPlaytimeStats } from "../../../launcher/ui/src/core/playtime";

const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../../launcher/ui/src/core/api", () => api);
vi.mock("../../../launcher/ui/src/core/controller", () => ({
  form: { theme: "light", reduceMotion: true },
}));
vi.mock("../../../launcher/ui/src/composables/useControlMotion", () => ({
  useControlMotion: () => ({ reduced: ref(true) }),
}));
let wrapper: VueWrapper;
afterEach(() => {
  wrapper?.unmount();
  fileRequest.value = null;
  api.invoke.mockReset();
});
const folder = (
  path: string,
  entries: FileListing["entries"] = [],
): FileListing => ({ path, parent: "/", home: "/home", entries });
function renderPicker(save = false) {
  wrapper = mount(GamepadFilePicker, {
    global: {
      stubs: {
        BaseModal: { template: "<div><slot /></div>" },
        BaseSwitch: true,
        BaseButton: {
          props: ["busy", "disabled"],
          template:
            '<button :disabled="busy || disabled" :data-busy="busy"><slot /></button>',
        },
        BaseInput: {
          props: ["modelValue"],
          emits: ["update:modelValue", "input"],
          template:
            "<input :value=\"modelValue\" @input=\"$emit('update:modelValue', $event.target.value); $emit('input')\" />",
        },
      },
    },
  });
  const resolve = vi.fn();
  fileRequest.value = {
    title: "文件",
    directory: !save,
    save,
    extensions: save ? ["png"] : [],
    resolve,
  };
  return resolve;
}
function button(text: string) {
  return wrapper.findAll("button").find((item) => item.text() === text)!;
}
describe("文件选择器请求与最终路径", () => {
  it.each([
    ["poster", "poster.png"],
    ["poster.PNG", "poster.PNG"],
    [".png", ".png.png"],
  ])("保存 %s 时返回最终名称 %s", async (input, expected) => {
    api.invoke.mockResolvedValue(folder("/home"));
    const resolve = renderPicker(true);
    await flushPromises();
    await wrapper.get("#pad-file-name").setValue(input);
    await button("保存").trigger("click");
    expect(resolve).toHaveBeenCalledWith(`/home/${expected}`);
  });
  it("最终 PNG 路径存在时先确认覆盖", async () => {
    api.invoke.mockResolvedValue(
      folder("/home", [
        { name: "poster.png", path: "/home/poster.png", directory: false },
      ]),
    );
    const resolve = renderPicker(true);
    await flushPromises();
    await wrapper.get("#pad-file-name").setValue("poster.jpg");
    await button("保存").trigger("click");
    expect(resolve).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("再次确认将覆盖");
    await button("覆盖文件").trigger("click");
    expect(resolve).toHaveBeenCalledWith("/home/poster.png");
  });
  it("最终 PNG 名称不能覆盖目录", async () => {
    api.invoke.mockResolvedValue(
      folder("/home", [
        { name: "poster.png", path: "/home/poster.png", directory: true },
      ]),
    );
    const resolve = renderPicker(true);
    await flushPromises();
    await wrapper.get("#pad-file-name").setValue("poster");
    await button("保存").trigger("click");
    expect(resolve).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("该名称已被文件夹使用");
  });
  it("关闭后重开, 旧请求不清除新请求的忙碌状态", async () => {
    let old!: (value: FileListing) => void;
    let current!: (value: FileListing) => void;
    api.invoke
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            old = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            current = resolve;
          }),
      );
    renderPicker();
    await button("取消").trigger("click");
    fileRequest.value = {
      title: "新目录",
      directory: true,
      save: false,
      extensions: [],
      resolve: vi.fn(),
    };
    old(folder("/old"));
    await flushPromises();
    expect(button("选择此文件夹").attributes("disabled")).toBeDefined();
    expect(button("前往").attributes("data-busy")).toBe("true");
    current(folder("/new"));
    await flushPromises();
    expect(
      (wrapper.get("#pad-file-path").element as HTMLInputElement).value,
    ).toBe("/new");
  });
  it("后发浏览优先, 旧失败不覆盖成功结果", async () => {
    let rejectOld!: (cause: Error) => void;
    api.invoke
      .mockImplementationOnce(
        () =>
          new Promise((_, reject) => {
            rejectOld = reject;
          }),
      )
      .mockResolvedValueOnce(folder("/new"));
    renderPicker();
    // 请求可能由其他入口发起, 不依赖按钮的忙碌禁用状态.
    await wrapper.findComponent(GamepadFilePicker).vm.$nextTick();
    (
      wrapper.vm as unknown as { browse: (path: string) => Promise<void> }
    ).browse("/new");
    await flushPromises();
    rejectOld(new Error("旧请求失败"));
    await flushPromises();
    expect(wrapper.text()).not.toContain("旧请求失败");
    expect(
      (wrapper.get("#pad-file-path").element as HTMLInputElement).value,
    ).toBe("/new");
  });
  it("浏览失败后不能提交旧目录", async () => {
    api.invoke
      .mockResolvedValueOnce(folder("/home"))
      .mockRejectedValueOnce(new Error("无权访问"));
    const resolve = renderPicker();
    await flushPromises();
    await wrapper.get("#pad-file-path").setValue("/denied");
    await button("前往").trigger("click");
    await flushPromises();
    expect(button("选择此文件夹").attributes("disabled")).toBeDefined();
    expect(resolve).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("无权访问");
  });
});
describe("图表键盘读屏信息", () => {
  it("无记录日期仍可作为可读取的活动日期", async () => {
    const stats = buildPlaytimeStats(
      { sessions: [], active: null },
      new Date(2026, 9, 28, 12).getTime(),
    );
    wrapper = mount(PlaytimeHeatmap, {
      props: { heatmap: stats.heatmap, daysPlayed: 0, totalMs: 0 },
      global: { stubs: { FloatingTooltip: true } },
    });
    const grid = wrapper.get("[tabindex]");
    await grid.trigger("keydown", { key: "Enter" });
    const active = wrapper.get(
      `[id="${grid.attributes("aria-activedescendant")}"]`,
    );
    expect(active.attributes("aria-hidden")).toBeUndefined();
    expect(active.attributes("aria-label")).toMatch(/2026年.*0/);
    await grid.trigger("keydown", { key: "ArrowLeft" });
    expect(active.attributes("aria-hidden")).toBe("true");
    expect(wrapper.get(".playtime-heatmap__sr").text()).toContain(
      "暂无游玩记录",
    );
  });
  it("活动月份描述同步时长与场次, 退出后停止播报", async () => {
    wrapper = mount(PlaytimeMonthlyChart, {
      props: {
        months: [
          { key: "2026-09", label: "9月", ms: 3600000, sessions: 2 },
          { key: "2026-10", label: "10月", ms: 7200000, sessions: 3 },
        ],
      },
      global: { stubs: { FloatingTooltip: true } },
    });
    const chart = wrapper.get("[tabindex]");
    await chart.trigger("keydown", { key: "Enter" });
    const description = wrapper.get(
      `[id="${chart.attributes("aria-describedby")}"]`,
    );
    expect(description.text()).toContain("10月, 2 小时, 3 次游玩");
    await chart.trigger("keydown", { key: "ArrowLeft" });
    expect(description.text()).toContain("9月, 1 小时, 2 次游玩");
    await chart.trigger("keydown", { key: "Escape" });
    expect(chart.attributes("aria-describedby")).toBeUndefined();
    expect(description.text()).toBe("");
  });
});
