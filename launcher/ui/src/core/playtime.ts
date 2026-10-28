import type {
  ActivePlaytime,
  PlaytimeHistory,
  PlaytimeSession,
} from "../types/launcher";

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;
const HEATMAP_WEEKS = 53;
const MONTH_WINDOW = 12;

export interface PlaytimeDay {
  key: string;
  date: number;
  ms: number;
  level: number;
  future: boolean;
}

export interface PlaytimeMonth {
  key: string;
  label: string;
  ms: number;
  sessions: number;
}

export interface HeatmapMonthLabel {
  column: number;
  label: string;
}

export interface PlaytimeHeatmap {
  weeks: PlaytimeDay[][];
  months: HeatmapMonthLabel[];
  start: number;
  end: number;
}

export interface PlaytimeStats {
  totalMs: number;
  sessionCount: number;
  daysPlayed: number;
  longestSessionMs: number;
  averageSessionMs: number;
  currentStreak: number;
  longestStreak: number;
  lastPlayedAt: number | null;
  lastSession: PlaytimeSession | null;
  active: ActivePlaytime | null;
  activeMs: number;
  dayMs: Record<string, number>;
  months: PlaytimeMonth[];
  heatmap: PlaytimeHeatmap;
}

export function formatPlaytime(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / MINUTE_MS);
  if (minutes <= 0) return ms > 0 ? "不足 1 分钟" : "0 分钟";
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (!hours) return `${rest} 分钟`;
  if (!rest) return `${hours} 小时`;
  return `${hours} 小时 ${rest} 分`;
}

export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return [hours, minutes, seconds]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");
}

export function formatShortDate(ms: number): string {
  const date = new Date(ms);
  return `${date.getMonth() + 1}月${date.getDate()}日`;
}

export function formatSessionRange(session: PlaytimeSession): string {
  const start = new Date(session.startedAt);
  return `${formatShortDate(start.getTime())} ${formatSessionClock(session)}`;
}

export function formatSessionClock(session: PlaytimeSession): string {
  const start = new Date(session.startedAt);
  const end = new Date(session.endedAt);
  const startTime = timeOfDay(start);
  const endTime = timeOfDay(end);
  if (localDayKey(start.getTime()) === localDayKey(end.getTime()))
    return `${startTime} - ${endTime}`;
  return `${startTime} - ${formatShortDate(end.getTime())} ${endTime}`;
}

export function latestSession(
  history: PlaytimeHistory | null,
): PlaytimeSession | null {
  let latest: PlaytimeSession | null = null;
  for (const session of history?.sessions ?? []) {
    if (!latest || session.endedAt >= latest.endedAt) latest = session;
  }
  return latest;
}

export function playModeLabel(mode: string): string {
  if (mode === "official") return "前台启动";
  if (mode === "direct") return "直接启动";
  return "后台启动";
}

export function buildPlaytimeStats(
  history: PlaytimeHistory,
  now = Date.now(),
): PlaytimeStats {
  const sessions = history.sessions
    .filter(
      (session) =>
        Number.isFinite(session.startedAt) &&
        Number.isFinite(session.endedAt) &&
        session.startedAt > 0 &&
        session.endedAt > session.startedAt,
    )
    .slice()
    .sort((left, right) => left.startedAt - right.startedAt);
  const active = history.active;
  const activeMs =
    active && active.startedAt > 0 && now > active.startedAt
      ? now - active.startedAt
      : 0;
  const dayMs: Record<string, number> = {};
  let totalMs = 0;
  for (const session of sessions) {
    addInterval(dayMs, session.startedAt, session.endedAt);
    totalMs += session.endedAt - session.startedAt;
  }
  if (active && activeMs > 0) {
    addInterval(dayMs, active.startedAt, now);
    totalMs += activeMs;
  }

  const sessionCount = sessions.length + (activeMs > 0 ? 1 : 0);
  const longestSessionMs = Math.max(
    activeMs,
    ...sessions.map((session) => session.endedAt - session.startedAt),
    0,
  );
  const lastSession = sessions.reduce<PlaytimeSession | null>(
    (latest, session) =>
      !latest || session.endedAt >= latest.endedAt ? session : latest,
    null,
  );
  const orderedDays = Object.keys(dayMs)
    .map((key) => keyToDayNumber(key))
    .sort((left, right) => left - right);
  const played = new Set(orderedDays);

  return {
    totalMs,
    sessionCount,
    daysPlayed: played.size,
    longestSessionMs,
    averageSessionMs: sessionCount ? totalMs / sessionCount : 0,
    currentStreak: currentStreak(played, now),
    longestStreak: longestStreak(orderedDays),
    lastPlayedAt: active && activeMs > 0 ? now : (lastSession?.endedAt ?? null),
    lastSession,
    active: active ?? null,
    activeMs,
    dayMs,
    months: buildMonths(dayMs, sessions, activeMs > 0 ? active : null, now),
    heatmap: buildHeatmap(dayMs, now),
  };
}

function addInterval(
  dayMs: Record<string, number>,
  startedAt: number,
  endedAt: number,
) {
  if (!(endedAt > startedAt)) return;
  let cursor = startedAt;
  while (cursor < endedAt) {
    const date = new Date(cursor);
    const next = new Date(
      date.getFullYear(),
      date.getMonth(),
      date.getDate() + 1,
    ).getTime();
    const stop = Math.min(endedAt, next);
    if (stop <= cursor) break;
    const key = localDayKey(cursor);
    dayMs[key] = (dayMs[key] ?? 0) + (stop - cursor);
    cursor = stop;
  }
}

function buildMonths(
  dayMs: Record<string, number>,
  sessions: PlaytimeSession[],
  active: ActivePlaytime | null,
  now: number,
): PlaytimeMonth[] {
  const months: PlaytimeMonth[] = [];
  const first = new Date(now);
  first.setDate(1);
  first.setMonth(first.getMonth() - (MONTH_WINDOW - 1));
  for (let index = 0; index < MONTH_WINDOW; index += 1) {
    const date = new Date(first.getFullYear(), first.getMonth() + index, 1);
    const key = localMonthKey(date.getTime());
    const ms = Object.entries(dayMs).reduce(
      (total, [day, value]) => total + (day.startsWith(key) ? value : 0),
      0,
    );
    const count =
      sessions.filter((session) => localMonthKey(session.startedAt) === key)
        .length + (active && localMonthKey(active.startedAt) === key ? 1 : 0);
    months.push({
      key,
      label: `${date.getMonth() + 1}月`,
      ms,
      sessions: count,
    });
  }
  return months;
}

function buildHeatmap(
  dayMs: Record<string, number>,
  now: number,
): PlaytimeHeatmap {
  const currentWeek = startOfWeek(now);
  const first = addDays(currentWeek, -7 * (HEATMAP_WEEKS - 1));
  const today = startOfDay(now);
  const weeks: PlaytimeDay[][] = [];
  const months: HeatmapMonthLabel[] = [];
  let lastMonth = -1;
  for (let week = 0; week < HEATMAP_WEEKS; week += 1) {
    const weekStart = addDays(first, week * 7);
    const days: PlaytimeDay[] = [];
    for (let day = 0; day < 7; day += 1) {
      const date = addDays(weekStart, day);
      const future = date > today;
      const key = localDayKey(date);
      const ms = future ? 0 : (dayMs[key] ?? 0);
      days.push({
        key,
        date,
        ms,
        level: heatLevel(ms),
        future,
      });
    }
    weeks.push(days);
    const month = new Date(weekStart).getMonth();
    if (month !== lastMonth) {
      months.push({ column: week, label: `${month + 1}月` });
      lastMonth = month;
    }
  }
  if (months.length > 1 && months[months.length - 1].label === months[0].label)
    months.shift();
  while (months.length > 12) months.shift();
  return {
    weeks,
    months,
    start: first,
    end: addDays(first, HEATMAP_WEEKS * 7 - 1),
  };
}

function heatLevel(ms: number): number {
  if (ms <= 0) return 0;
  if (ms < 30 * MINUTE_MS) return 1;
  if (ms < HOUR_MS) return 2;
  if (ms < 2 * HOUR_MS) return 3;
  if (ms < 4 * HOUR_MS) return 4;
  return 5;
}

function currentStreak(played: Set<number>, now: number): number {
  const today = keyToDayNumber(localDayKey(now));
  let cursor = played.has(today)
    ? today
    : played.has(today - 1)
      ? today - 1
      : null;
  let count = 0;
  while (cursor !== null && played.has(cursor)) {
    count += 1;
    cursor -= 1;
  }
  return count;
}

function longestStreak(days: number[]): number {
  let longest = 0;
  let run = 0;
  let previous: number | null = null;
  for (const day of days) {
    run = previous !== null && day === previous + 1 ? run + 1 : 1;
    longest = Math.max(longest, run);
    previous = day;
  }
  return longest;
}

function timeOfDay(date: Date): string {
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function startOfDay(ms: number): number {
  const date = new Date(ms);
  return new Date(
    date.getFullYear(),
    date.getMonth(),
    date.getDate(),
  ).getTime();
}

function addDays(ms: number, days: number): number {
  const date = new Date(ms);
  return new Date(
    date.getFullYear(),
    date.getMonth(),
    date.getDate() + days,
  ).getTime();
}

function startOfWeek(ms: number): number {
  const date = new Date(startOfDay(ms));
  return addDays(startOfDay(ms), -((date.getDay() + 6) % 7));
}

function localDayKey(ms: number): string {
  const date = new Date(ms);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function localMonthKey(ms: number): string {
  const date = new Date(ms);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}`;
}

function keyToDayNumber(key: string): number {
  const [year, month, day] = key.split("-").map(Number);
  return Math.floor(Date.UTC(year, month - 1, day) / DAY_MS);
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}
