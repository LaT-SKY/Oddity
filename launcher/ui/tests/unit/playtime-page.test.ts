import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { nextTick } from "vue";
import type { PlaytimeHistory } from "../../src/types/launcher";

const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../src/core/api", () => ({
  invoke: api.invoke,
  isDesktop: () => false,
}));
import * as c from "../../src/core/controller";
import PlaytimePage from "../../src/pages/PlaytimePage.vue";

let wrapper: VueWrapper;
let history: PlaytimeHistory;
const now = new Date(2026, 9, 28, 12).getTime();
beforeEach(() => {
  vi.setSystemTime(now);
  c.dispose();
  c.form.reduceMotion = true;
  history = { sessions: [], active: null };
  api.invoke
    .mockReset()
    .mockImplementation(async () => structuredClone(history));
});
afterEach(() => {
  wrapper?.unmount();
  c.dispose();
});

async function render(page = "records") {
  c.state.page = page;
  wrapper = mount(PlaytimePage, {
    global: {
      stubs: {
        PlaytimeHeatmap: true,
        PlaytimeMonthlyChart: true,
        PlaytimePoster: {
          props: ["open"],
          template: '<div v-if="open" data-poster />',
        },
      },
    },
  });
  await flushPromises();
}
const posterButton = () => wrapper.get(".page-head__actions button");

it("未进入记录页不加载记录或渲染图表", async () => {
  await render("home");
  expect(api.invoke).not.toHaveBeenCalled();
  expect(wrapper.find(".playtime-content").exists()).toBe(false);
  expect(wrapper.attributes("aria-hidden")).toBe("true");
  expect(wrapper.attributes()).toHaveProperty("inert");
});

it("空记录禁用生成海报并可返回主页", async () => {
  await render();
  expect(wrapper.get(".playtime-empty").text()).toContain("还没有游玩记录");
  expect(posterButton().attributes()).toHaveProperty("disabled");
  expect(wrapper.find("playtime-heatmap-stub").exists()).toBe(false);
  await wrapper.get(".playtime-empty button").trigger("click");
  expect(c.state.page).toBe("home");
});

it("加载失败显示错误且重试后显示统计", async () => {
  api.invoke.mockRejectedValueOnce(new Error("读取记录失败"));
  await render();
  expect(wrapper.get('[role="status"]').text()).toContain("读取记录失败");
  history.sessions = [
    {
      startedAt: now - 3_600_000,
      endedAt: now,
      mode: "official",
      distribution: "cn",
    },
  ];
  await wrapper.get(".playtime-error button").trigger("click");
  await flushPromises();
  expect(wrapper.find(".playtime-error").exists()).toBe(false);
  expect(wrapper.get(".playtime-summary").text()).toContain("1 小时");
  expect(wrapper.find("playtime-heatmap-stub").exists()).toBe(true);
  await posterButton().trigger("click");
  expect(wrapper.find("[data-poster]").exists()).toBe(true);
});

it("加载期间显示忙碌状态且不显示空记录提示", async () => {
  let resolve!: (history: PlaytimeHistory) => void;
  api.invoke.mockReturnValueOnce(
    new Promise<PlaytimeHistory>((done) => {
      resolve = done;
    }),
  );
  await render();
  expect(wrapper.attributes("aria-busy")).toBe("true");
  expect(wrapper.find(".playtime-empty").exists()).toBe(false);
  resolve(history);
  await flushPromises();
  expect(wrapper.attributes("aria-busy")).toBe("false");
  expect(wrapper.find(".playtime-empty").exists()).toBe(true);
});

it("秒级更新活动计时, 30 秒后更新汇总且不轮询后端", async () => {
  history.active = {
    startedAt: now - 59_000,
    lastSeenAt: now - 1_000,
    mode: "direct",
    distribution: "global",
  };
  c.state.game = "running";
  await render();
  expect(wrapper.get('[role="timer"]').text()).toContain("00:00:59");
  expect(wrapper.get(".playtime-summary").text()).toContain("不足 1 分钟");
  api.invoke.mockClear();
  await vi.advanceTimersByTimeAsync(1000);
  expect(wrapper.get('[role="timer"]').text()).toContain("00:01:00");
  expect(wrapper.get(".playtime-summary").text()).toContain("不足 1 分钟");
  await vi.advanceTimersByTimeAsync(29_000);
  expect(wrapper.get(".playtime-summary").text()).toContain("1 分钟");
  expect(api.invoke).not.toHaveBeenCalled();
  c.state.page = "home";
  await nextTick();
  await vi.advanceTimersByTimeAsync(600);
  expect(wrapper.find(".playtime-content").exists()).toBe(false);
  expect(api.invoke).not.toHaveBeenCalled();
});

it("离页后等待过渡再卸载, 快速返回保留内容和海报", async () => {
  history.sessions = [
    {
      startedAt: now - 3_600_000,
      endedAt: now,
      mode: "official",
      distribution: "cn",
    },
  ];
  await render();
  await posterButton().trigger("click");
  c.state.page = "home";
  await nextTick();
  await vi.advanceTimersByTimeAsync(599);
  expect(wrapper.find(".playtime-content").exists()).toBe(true);
  c.state.page = "records";
  await nextTick();
  await vi.advanceTimersByTimeAsync(1);
  expect(wrapper.find("[data-poster]").exists()).toBe(true);
  c.state.page = "home";
  await nextTick();
  await vi.advanceTimersByTimeAsync(600);
  expect(wrapper.find("[data-poster]").exists()).toBe(false);
  c.state.page = "records";
  await flushPromises();
  expect(wrapper.find(".playtime-content").exists()).toBe(true);
  expect(wrapper.find("[data-poster]").exists()).toBe(false);
});

it("游戏停止或组件卸载后释放计时器", async () => {
  history.active = {
    startedAt: now - 60_000,
    lastSeenAt: now,
    mode: "background",
    distribution: "cn",
  };
  c.state.game = "running";
  await render();
  const clear = vi.spyOn(window, "clearInterval");
  c.state.game = "idle";
  await nextTick();
  expect(clear).toHaveBeenCalledOnce();
  c.state.game = "running";
  await nextTick();
  wrapper.unmount();
  expect(clear).toHaveBeenCalledTimes(2);
});
