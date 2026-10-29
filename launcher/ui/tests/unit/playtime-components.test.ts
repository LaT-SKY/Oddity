import { afterEach, describe, expect, it, vi } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import PlaytimeHeatmap from "../../src/components/playtime/PlaytimeHeatmap.vue";
import PlaytimeMonthlyChart from "../../src/components/playtime/PlaytimeMonthlyChart.vue";
import {
  buildPlaytimeStats,
  type PlaytimeMonth,
} from "../../src/core/playtime";

const HOUR = 3_600_000;
const now = new Date(2026, 9, 28, 12).getTime();
const stats = buildPlaytimeStats(
  {
    sessions: [
      {
        startedAt: now - HOUR,
        endedAt: now,
        mode: "official",
        distribution: "cn",
      },
    ],
    active: null,
  },
  now,
);
const tooltip = {
  props: ["open", "x", "y", "theme"],
  template:
    '<aside v-if="open" role="tooltip" :data-theme="theme"><slot /></aside>',
};
let wrapper: VueWrapper;
afterEach(() => wrapper?.unmount());

describe("游玩热力图", () => {
  function render() {
    wrapper = mount(PlaytimeHeatmap, {
      props: {
        heatmap: stats.heatmap,
        daysPlayed: 1,
        totalMs: HOUR,
        theme: "dark",
      },
      global: { stubs: { FloatingTooltip: tooltip } },
    });
  }

  it("展示汇总及可读取的每日记录, 格子不进入焦点顺序", () => {
    render();
    expect(wrapper.get('[role="group"]').attributes("aria-label")).toBe(
      "最近 12 个月游玩 1 小时, 已游玩 1 天",
    );
    expect(wrapper.findAll(".playtime-heatmap__cell")).toHaveLength(371);
    expect(
      wrapper.findAll(".playtime-heatmap__sr li").map((row) => row.text()),
    ).toEqual(["2026年10月28日: 1 小时"]);
    expect(wrapper.findAll(".playtime-heatmap__cell[tabindex]")).toHaveLength(
      0,
    );
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
  });

  it("悬停已游玩日期显示时长, 移到空白和未来日期时清除提示", async () => {
    render();
    const days = stats.heatmap.weeks.flat();
    const index = days.findIndex((day) => day.key === "2026-10-28");
    const cell = wrapper.get(`[data-index="${index}"]`);
    await cell.trigger("mouseover", { clientX: 100, clientY: 200 });
    expect(wrapper.get('[role="tooltip"]').text()).toContain("10月28日");
    expect(wrapper.get('[role="tooltip"]').text()).toContain("1 小时");
    expect(wrapper.get('[role="tooltip"]').attributes("data-theme")).toBe(
      "dark",
    );
    expect(cell.classes()).toContain("is-hovered");
    await wrapper.get(`[data-index="${index + 1}"]`).trigger("mouseover");
    expect(cell.classes()).not.toContain("is-hovered");
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
    await cell.trigger("mouseover");
    await wrapper.get(".playtime-heatmap__weekday").trigger("mouseover");
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
  });

  it("悬停只切换高亮, 格子不改变尺寸并在离开后取消", async () => {
    render();
    const days = stats.heatmap.weeks.flat();
    const index = days.findIndex((day) => day.key === "2026-10-28");
    const cell = wrapper.get(`[data-index="${index}"]`);
    await cell.trigger("mouseover", { clientX: 100, clientY: 200 });
    expect(cell.classes()).toContain("is-hovered");
    expect((cell.element as HTMLElement).style.transform).toBe("");
    expect((cell.element as HTMLElement).style.zIndex).toBe("");
    await wrapper.get(`[data-index="${index - 1}"]`).trigger("mouseover");
    const next = wrapper.get(`[data-index="${index - 1}"]`);
    expect(next.classes()).toContain("is-hovered");
    expect(cell.classes()).not.toContain("is-hovered");
    await wrapper.get(".playtime-heatmap__grid").trigger("mouseleave");
    expect(wrapper.findAll(".is-hovered")).toHaveLength(0);
  });

  it("空白日期提示没有记录, 离开网格后隐藏提示", async () => {
    render();
    await wrapper.get('[data-index="0"]').trigger("mouseover");
    expect(wrapper.get('[role="tooltip"]').text()).toContain("没有记录");
    await wrapper.get(".playtime-heatmap__grid").trigger("mouseleave");
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
  });

  it("无游玩日期时展示可读取的空状态", () => {
    wrapper = mount(PlaytimeHeatmap, {
      props: {
        heatmap: buildPlaytimeStats({ sessions: [], active: null }, now)
          .heatmap,
        daysPlayed: 0,
        totalMs: 0,
      },
      global: { stubs: { FloatingTooltip: tooltip } },
    });
    expect(wrapper.get(".playtime-heatmap__sr").text()).toContain(
      "暂无游玩记录",
    );
    expect(wrapper.findAll(".playtime-heatmap__sr li")).toHaveLength(0);
  });
});

describe("月度游玩图", () => {
  const months: PlaytimeMonth[] = [
    { key: "2026-08", label: "8月", ms: 0, sessions: 0 },
    { key: "2026-09", label: "9月", ms: HOUR, sessions: 2 },
    { key: "2026-10", label: "10月", ms: 2 * HOUR, sessions: 3 },
  ];
  function render(data = months) {
    wrapper = mount(PlaytimeMonthlyChart, {
      props: { months: data, theme: "dark" },
      global: { stubs: { FloatingTooltip: tooltip } },
    });
    vi.spyOn(
      wrapper.get("svg").element,
      "getBoundingClientRect",
    ).mockReturnValue({ left: 100, width: 720 } as DOMRect);
  }

  it("绘制零值及有记录月份并标记当前月", () => {
    render();
    expect(wrapper.get("svg").attributes("aria-label")).toContain(
      "月度游玩时长",
    );
    expect(
      wrapper.findAll(".playtime-chart__month").map((label) => label.text()),
    ).toEqual(["8月", "9月", "10月"]);
    const points = wrapper.findAll(".playtime-chart__dot");
    expect(points.map((point) => Number(point.attributes("cy")))).toEqual([
      190, 104, 18,
    ]);
    expect(points[2].classes()).toContain("is-current");
  });

  it.each([
    [0, "8月", "0 分钟", "0 次游玩"],
    [483, "9月", "1 小时", "2 次游玩"],
    [1000, "10月", "2 小时", "3 次游玩"],
  ])(
    "把横坐标 %i 限制在最近的月份",
    async (clientX, label, duration, count) => {
      render();
      await wrapper
        .get(".playtime-chart__hit")
        .trigger("mousemove", { clientX, clientY: 100 });
      const tip = wrapper.get('[role="tooltip"]');
      expect(tip.text()).toContain(label);
      expect(tip.text()).toContain(duration);
      expect(tip.text()).toContain(count);
      await wrapper.get(".playtime-chart__hit").trigger("mouseleave");
      expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
    },
  );

  it("单月数据在图中居中且保持有效坐标", async () => {
    render([months[1]]);
    expect(wrapper.get(".playtime-chart__dot").attributes("cx")).toBe("383");
    await wrapper
      .get(".playtime-chart__hit")
      .trigger("mouseenter", { clientX: 500 });
    expect(wrapper.get('[role="tooltip"]').text()).toContain("9月");
  });

  it("空月份不产生无效路径或悬停提示", async () => {
    render([]);
    expect(wrapper.get(".playtime-chart__line").attributes("d")).toBe("");
    expect(wrapper.get(".playtime-chart__area").attributes("d")).toBe("");
    expect(wrapper.html()).not.toMatch(/NaN|Infinity/);
    await wrapper
      .get(".playtime-chart__hit")
      .trigger("mouseenter", { clientX: 500 });
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
  });
});
