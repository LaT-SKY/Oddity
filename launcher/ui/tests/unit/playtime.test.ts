import { describe, expect, it } from "vitest";
import {
  buildPlaytimeStats,
  formatClock,
  formatPlaytime,
  formatSessionRange,
  formatShortDate,
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
