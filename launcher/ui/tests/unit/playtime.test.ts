import { describe, expect, it } from "vitest";
import {
  buildPlaytimeStats,
  formatClock,
  formatPlaytime,
  formatSessionClock,
  formatSessionRange,
  formatShortDate,
  latestSession,
  playModeLabel,
} from "../../src/core/playtime";
import type {
  PlaytimeHistory,
  PlaytimeSession,
} from "../../src/types/launcher";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

function at(year: number, month: number, day: number, hour = 0, minute = 0) {
  return new Date(year, month, day, hour, minute).getTime();
}

function session(
  startedAt: number,
  endedAt: number,
  mode = "background",
): PlaytimeSession {
  return { startedAt, endedAt, mode, distribution: "cn" };
}

function history(
  sessions: PlaytimeSession[],
  active: PlaytimeHistory["active"] = null,
): PlaytimeHistory {
  return { sessions, active };
}

describe("游玩时长格式化", () => {
  it("按分钟和小时显示时长", () => {
    expect(formatPlaytime(0)).toBe("0 分钟");
    expect(formatPlaytime(30_000)).toBe("不足 1 分钟");
    expect(formatPlaytime(MINUTE)).toBe("1 分钟");
    expect(formatPlaytime(HOUR)).toBe("1 小时");
    expect(formatPlaytime(2 * HOUR + 14 * MINUTE)).toBe("2 小时 14 分");
    expect(formatPlaytime(26 * HOUR + 5 * MINUTE)).toBe("26 小时 5 分");
    expect(formatPlaytime(-1)).toBe("0 分钟");
  });

  it("显示实时计时和日期", () => {
    expect(formatClock(3_723_000)).toBe("01:02:03");
    expect(formatClock(-1)).toBe("00:00:00");
    expect(formatShortDate(at(2026, 9, 28, 12))).toBe("10月28日");
  });

  it("区分同一天和跨天的会话", () => {
    expect(
      formatSessionRange(
        session(at(2026, 9, 28, 21, 5), at(2026, 9, 28, 23, 45)),
      ),
    ).toBe("10月28日 21:05 - 23:45");
    expect(
      formatSessionRange(
        session(at(2026, 9, 28, 23, 5), at(2026, 9, 29, 1, 10)),
      ),
    ).toBe("10月28日 23:05 - 10月29日 01:10");
  });

  it("标记启动方式", () => {
    expect(playModeLabel("official")).toBe("前台启动");
    expect(playModeLabel("direct")).toBe("直接启动");
    expect(playModeLabel("background")).toBe("后台启动");
    expect(playModeLabel("unknown")).toBe("后台启动");
  });
});

describe("游玩统计", () => {
  it("按本地日期拆分跨午夜会话", () => {
    const stats = buildPlaytimeStats(
      history([
        session(at(2026, 4, 10, 23, 30), at(2026, 4, 11, 1, 30)),
        session(at(2026, 4, 11, 10), at(2026, 4, 11, 11)),
      ]),
      at(2026, 4, 11, 12),
    );
    expect(stats.dayMs["2026-05-10"]).toBe(30 * MINUTE);
    expect(stats.dayMs["2026-05-11"]).toBe(2.5 * HOUR);
    expect(stats.totalMs).toBe(3 * HOUR);
    expect(stats.daysPlayed).toBe(2);
    expect(stats.sessionCount).toBe(2);
    expect(stats.longestSessionMs).toBe(2 * HOUR);
    expect(stats.averageSessionMs).toBe(1.5 * HOUR);
  });

  it("计算当前连续和最长连续天数", () => {
    const now = at(2026, 5, 15, 12);
    const played = [
      session(at(2026, 5, 15, 9), at(2026, 5, 15, 10)),
      session(at(2026, 5, 14, 9), at(2026, 5, 14, 10)),
      session(at(2026, 5, 13, 9), at(2026, 5, 13, 10)),
      session(at(2026, 5, 11, 9), at(2026, 5, 11, 10)),
    ];
    const stats = buildPlaytimeStats(history(played), now);
    expect(stats.currentStreak).toBe(3);
    expect(stats.longestStreak).toBe(3);
    expect(stats.daysPlayed).toBe(4);

    const withoutToday = buildPlaytimeStats(history(played.slice(1)), now);
    expect(withoutToday.currentStreak).toBe(2);
  });

  it("没有今天记录时保留到昨天的连续天数", () => {
    const now = at(2026, 5, 15, 12);
    const stats = buildPlaytimeStats(
      history([
        session(at(2026, 5, 14, 9), at(2026, 5, 14, 10)),
        session(at(2026, 5, 13, 9), at(2026, 5, 13, 10)),
      ]),
      now,
    );
    expect(stats.currentStreak).toBe(2);
  });

  it("把进行中的会话计入统计, 但保留上一次已结束的会话", () => {
    const now = at(2026, 5, 15, 12);
    const stats = buildPlaytimeStats(
      history([session(at(2026, 5, 14, 9), at(2026, 5, 14, 10))], {
        startedAt: at(2026, 5, 15, 10),
        lastSeenAt: now,
        mode: "official",
        distribution: "cn",
      }),
      now,
    );
    expect(stats.activeMs).toBe(2 * HOUR);
    expect(stats.totalMs).toBe(3 * HOUR);
    expect(stats.sessionCount).toBe(2);
    expect(stats.dayMs["2026-06-15"]).toBe(2 * HOUR);
    expect(stats.lastSession?.startedAt).toBe(at(2026, 5, 14, 9));
    expect(stats.lastPlayedAt).toBe(now);
    expect(stats.currentStreak).toBe(2);
  });

  it("按最近 12 个月汇总时长和场次", () => {
    const stats = buildPlaytimeStats(
      history([
        session(at(2025, 6, 15, 9), at(2025, 6, 15, 10)),
        session(at(2026, 5, 15, 9), at(2026, 5, 15, 10)),
      ]),
      at(2026, 5, 15, 12),
    );
    expect(stats.months).toHaveLength(12);
    expect(stats.months[0].key).toBe("2025-07");
    expect(stats.months[0].ms).toBe(HOUR);
    expect(stats.months[0].sessions).toBe(1);
    expect(stats.months[11].key).toBe("2026-06");
    expect(stats.months[11].ms).toBe(HOUR);
    expect(stats.months[11].label).toBe("6月");
  });

  it("按开始时间取最近的已结束会话", () => {
    const stats = buildPlaytimeStats(
      history([
        session(at(2026, 5, 14, 9), at(2026, 5, 14, 10)),
        session(at(2026, 5, 15, 8), at(2026, 5, 15, 9)),
      ]),
      at(2026, 5, 15, 12),
    );
    expect(stats.lastSession?.startedAt).toBe(at(2026, 5, 15, 8));
  });

  it("过滤无效记录并返回空统计", () => {
    const stats = buildPlaytimeStats(
      history([
        session(at(2026, 5, 15, 10), at(2026, 5, 15, 9)),
        session(Number.NaN, Number.NaN),
        session(at(2026, 5, 15, 9), at(2026, 5, 15, 10)),
      ]),
      at(2026, 5, 15, 12),
    );
    expect(stats.sessionCount).toBe(1);
    expect(stats.totalMs).toBe(HOUR);

    const empty = buildPlaytimeStats(history([]), at(2026, 5, 15, 12));
    expect(empty.totalMs).toBe(0);
    expect(empty.sessionCount).toBe(0);
    expect(empty.lastSession).toBeNull();
    expect(empty.lastPlayedAt).toBeNull();
    expect(empty.currentStreak).toBe(0);
    expect(empty.longestStreak).toBe(0);
  });
});

describe("热力图", () => {
  it("生成 53 周并让每周从星期一开始", () => {
    const now = at(2026, 9, 28, 12);
    const stats = buildPlaytimeStats(history([]), now);
    expect(stats.heatmap.weeks).toHaveLength(53);
    for (const week of stats.heatmap.weeks) {
      expect(week).toHaveLength(7);
      expect(new Date(week[0].date).getDay()).toBe(1);
    }
    expect(new Date(stats.heatmap.start).getDay()).toBe(1);
    expect(new Date(stats.heatmap.end).getDay()).toBe(0);
    expect(stats.heatmap.months.length).toBeLessThanOrEqual(12);
    expect(stats.heatmap.months.at(-1)?.label).toBe("10月");
  });

  it("标记未来的日期并按四档强度着色", () => {
    const now = at(2026, 9, 28, 12);
    const stats = buildPlaytimeStats(
      history([
        session(at(2026, 9, 27, 0), at(2026, 9, 27, 0) + 10 * MINUTE),
        session(at(2026, 9, 26, 0), at(2026, 9, 26, 0) + 45 * MINUTE),
        session(at(2026, 9, 25, 0), at(2026, 9, 25, 0) + 90 * MINUTE),
        session(at(2026, 9, 24, 0), at(2026, 9, 24, 0) + 3 * HOUR),
        session(at(2026, 9, 23, 0), at(2026, 9, 23, 0) + 5 * HOUR),
      ]),
      now,
    );
    const days = stats.heatmap.weeks.flat();
    expect(days.some((day) => day.future)).toBe(true);
    expect(days.find((day) => day.key === "2026-10-27")?.level).toBe(1);
    expect(days.find((day) => day.key === "2026-10-26")?.level).toBe(2);
    expect(days.find((day) => day.key === "2026-10-25")?.level).toBe(3);
    expect(days.find((day) => day.key === "2026-10-24")?.level).toBe(4);
    expect(days.find((day) => day.key === "2026-10-23")?.level).toBe(5);
  });
});

describe("游玩记录边界", () => {
  it.each([
    [59_999, "不足 1 分钟", "00:00:59"],
    [60_000, "1 分钟", "00:01:00"],
    [3_599_999, "59 分钟", "00:59:59"],
    [3_600_000, "1 小时", "01:00:00"],
    [100 * HOUR + 999, "100 小时", "100:00:00"],
  ])("在 %i 毫秒处向下取整且不限制小时位数", (ms, label, clock) => {
    expect(formatPlaytime(ms)).toBe(label);
    expect(formatClock(ms)).toBe(clock);
  });

  it("跨年会话显示结束日期", () => {
    expect(
      formatSessionClock(
        session(at(2025, 11, 31, 23, 59), at(2026, 0, 1, 0, 1)),
      ),
    ).toBe("23:59 - 1月1日 00:01");
  });

  it("按结束时间选择最近会话且不改变原始顺序", () => {
    const longer = session(at(2026, 5, 15, 8), at(2026, 5, 15, 12));
    const laterStart = session(at(2026, 5, 15, 10), at(2026, 5, 15, 11));
    const input = history([laterStart, longer]);
    const snapshot = structuredClone(input);
    expect(latestSession(input)).toEqual(longer);
    expect(buildPlaytimeStats(input, at(2026, 5, 15, 13)).lastSession).toEqual(
      longer,
    );
    expect(input).toEqual(snapshot);
    expect(latestSession(null)).toBeNull();
    expect(latestSession(history([]))).toBeNull();
  });

  it("会话恰在午夜结束时不增加下一天的游玩天数", () => {
    const stats = buildPlaytimeStats(
      history([session(at(2026, 5, 14, 23), at(2026, 5, 15))]),
      at(2026, 5, 15, 12),
    );
    expect(stats.dayMs).toEqual({ "2026-06-14": HOUR });
    expect(stats.daysPlayed).toBe(1);
    expect(stats.currentStreak).toBe(1);
  });

  it("跨年会话按月份拆分时长, 场次归属于开始月份", () => {
    const stats = buildPlaytimeStats(
      history([session(at(2025, 11, 31, 23, 30), at(2026, 0, 1, 1))]),
      at(2026, 0, 1, 12),
    );
    expect(stats.dayMs).toEqual({
      "2025-12-31": 30 * MINUTE,
      "2026-01-01": HOUR,
    });
    expect(stats.months.find((month) => month.key === "2025-12")).toMatchObject(
      { ms: 30 * MINUTE, sessions: 1 },
    );
    expect(stats.months.at(-1)).toMatchObject({
      key: "2026-01",
      ms: HOUR,
      sessions: 0,
    });
    expect(stats.currentStreak).toBe(2);
    expect(stats.longestStreak).toBe(2);
  });

  it("闰日跨多天会话逐日累计且只计算一次场次", () => {
    const stats = buildPlaytimeStats(
      history([session(at(2024, 1, 28, 23), at(2024, 2, 1, 1))]),
      at(2024, 2, 1, 12),
    );
    expect(stats.dayMs).toEqual({
      "2024-02-28": HOUR,
      "2024-02-29": 24 * HOUR,
      "2024-03-01": HOUR,
    });
    expect(stats.totalMs).toBe(26 * HOUR);
    expect(stats.sessionCount).toBe(1);
    expect(stats.daysPlayed).toBe(3);
    expect(stats.longestStreak).toBe(3);
  });

  it("连续记录中断后当前连续归零, 同日多场不增加天数", () => {
    const stats = buildPlaytimeStats(
      history([
        session(at(2026, 5, 10, 9), at(2026, 5, 10, 10)),
        session(at(2026, 5, 10, 11), at(2026, 5, 10, 12)),
        session(at(2026, 5, 11, 9), at(2026, 5, 11, 10)),
      ]),
      at(2026, 5, 15, 12),
    );
    expect(stats.daysPlayed).toBe(2);
    expect(stats.sessionCount).toBe(3);
    expect(stats.currentStreak).toBe(0);
    expect(stats.longestStreak).toBe(2);
  });

  it.each([
    [0, 60_000],
    [-1, 60_000],
    [Number.NaN, 1_000_000],
    [1_000_000, Number.NaN],
    [1_000_000, Number.POSITIVE_INFINITY],
    [1_000_000, 1_000_000],
    [1_000_000, 999_999],
  ])("不把无效已结束会话 %s 至 %s 计入统计", (startedAt, endedAt) => {
    const stats = buildPlaytimeStats(
      history([session(startedAt, endedAt)]),
      at(2026, 5, 15, 12),
    );
    expect(stats).toMatchObject({
      totalMs: 0,
      sessionCount: 0,
      daysPlayed: 0,
      averageSessionMs: 0,
      longestSessionMs: 0,
      lastSession: null,
    });
  });

  it.each([0, -1, at(2026, 5, 15, 12), at(2026, 5, 15, 13)])(
    "不累计尚无正时长的活动会话: %s",
    (startedAt) => {
      const stats = buildPlaytimeStats(
        history([], {
          startedAt,
          lastSeenAt: startedAt,
          mode: "direct",
          distribution: "global",
        }),
        at(2026, 5, 15, 12),
      );
      expect(stats).toMatchObject({
        activeMs: 0,
        totalMs: 0,
        sessionCount: 0,
        lastPlayedAt: null,
      });
      expect(stats.months.every((month) => month.sessions === 0)).toBe(true);
    },
  );

  it("活动会话跨月时按当前时间累计而非停在上一次心跳", () => {
    const now = at(2026, 5, 1, 1);
    const stats = buildPlaytimeStats(
      history([], {
        startedAt: at(2026, 4, 31, 23),
        lastSeenAt: at(2026, 4, 31, 23, 30),
        mode: "official",
        distribution: "cn",
      }),
      now,
    );
    expect(stats).toMatchObject({
      activeMs: 2 * HOUR,
      totalMs: 2 * HOUR,
      sessionCount: 1,
      averageSessionMs: 2 * HOUR,
      longestSessionMs: 2 * HOUR,
      lastPlayedAt: now,
      lastSession: null,
    });
    expect(stats.months.find((month) => month.key === "2026-05")).toMatchObject(
      { ms: HOUR, sessions: 1 },
    );
    expect(stats.months.at(-1)).toMatchObject({ ms: HOUR, sessions: 0 });
  });

  it("窗口外的历史计入累计时长但不计入最近月份与热力图", () => {
    const stats = buildPlaytimeStats(
      history([session(at(2020, 0, 1), at(2020, 0, 1, 1))]),
      at(2026, 5, 15, 12),
    );
    expect(stats.totalMs).toBe(HOUR);
    expect(stats.daysPlayed).toBe(1);
    expect(
      stats.months.every((month) => month.ms === 0 && month.sessions === 0),
    ).toBe(true);
    expect(
      stats.heatmap.weeks
        .flat()
        .every((day) => day.ms === 0 && day.level === 0),
    ).toBe(true);
  });

  it.each([
    [0, 0],
    [1, 1],
    [30 * MINUTE - 1, 1],
    [30 * MINUTE, 2],
    [HOUR - 1, 2],
    [HOUR, 3],
    [2 * HOUR - 1, 3],
    [2 * HOUR, 4],
    [4 * HOUR - 1, 4],
    [4 * HOUR, 5],
  ])("热力图在 %i 毫秒时使用等级 %i", (ms, level) => {
    const start = at(2026, 5, 15);
    const stats = buildPlaytimeStats(
      history([session(start, start + ms)]),
      start + 12 * HOUR,
    );
    expect(
      stats.heatmap.weeks.flat().find((day) => day.key === "2026-06-15"),
    ).toMatchObject({ ms, level, future: false });
  });

  it("热力图的今天可见, 本周未来日期隐藏时长", () => {
    const stats = buildPlaytimeStats(
      history([
        session(at(2026, 9, 28, 9), at(2026, 9, 28, 10)),
        session(at(2026, 9, 29, 9), at(2026, 9, 29, 10)),
      ]),
      at(2026, 9, 28, 12),
    );
    const days = stats.heatmap.weeks.flat();
    expect(new Set(days.map((day) => day.key)).size).toBe(371);
    expect(days.find((day) => day.key === "2026-10-28")).toMatchObject({
      ms: HOUR,
      future: false,
    });
    expect(days.find((day) => day.key === "2026-10-29")).toMatchObject({
      ms: 0,
      level: 0,
      future: true,
    });
  });
});
