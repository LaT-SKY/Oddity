import { beforeEach, expect, it, vi } from "vitest";
let n: typeof import("../../src/core/notifications");
beforeEach(async () => {
  n = await import("../../src/core/notifications");
  n.resetNotifications();
});

it("新通知堆叠, 后方通知保留剩余阅读时间", async () => {
  const first = n.notify("第一条", "ok", 1000);
  await vi.advanceTimersByTimeAsync(300);
  const second = n.notify("第二条", "ok", 500);
  expect(n.notifications.map((item) => item.id)).toEqual([second, first]);
  await vi.advanceTimersByTimeAsync(500);
  expect(n.notifications.map((item) => item.id)).toEqual([first]);
  await vi.advanceTimersByTimeAsync(699);
  expect(n.notifications).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(1);
  expect(n.notifications).toHaveLength(0);
});

it("悬停、聚焦与展开分别暂停计时, 全部解除后续计", async () => {
  n.notify("可操作消息", "ok", 1000);
  await vi.advanceTimersByTimeAsync(250);
  n.pauseNotifications("hover", true);
  n.pauseNotifications("expanded", true);
  await vi.advanceTimersByTimeAsync(5000);
  n.pauseNotifications("hover", false);
  await vi.advanceTimersByTimeAsync(5000);
  expect(n.notifications).toHaveLength(1);
  n.pauseNotifications("expanded", false);
  await vi.advanceTimersByTimeAsync(749);
  expect(n.notifications).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(1);
  expect(n.notifications).toHaveLength(0);
});

it("重复通知合并, 任务进度按键更新而不残留常驻消息", () => {
  const id = n.notify("正在导出", "info", 0, { key: "export" });
  n.notify("独立消息");
  expect(n.notify("导出成功", "ok", 500, { key: "export" })).toBe(id);
  expect(n.notifications).toHaveLength(2);
  n.notify("导出成功", "ok", 500, { key: "export" });
  expect(n.notifications[0].count).toBe(2);
  expect(n.notifications[0].duration).toBe(500);
});

it("动作防止重入, 失败保留通知与错误, 成功后移除", async () => {
  let reject!: (error: Error) => void;
  const run = vi.fn(
    () =>
      new Promise<void>((_, fail) => {
        reject = fail;
      }),
  );
  const id = n.notify("更新失败", "error", 1000, {
    primary: { label: "重试", icon: "i-arrows-clockwise", run },
  });
  const pending = n.runNotificationAction(id, "primary");
  await n.runNotificationAction(id, "primary");
  await vi.advanceTimersByTimeAsync(2000);
  expect(run).toHaveBeenCalledTimes(1);
  reject(new Error("网络断开"));
  await pending;
  expect(n.notifications[0].actionError).toBe("网络断开");
  expect(n.notifications[0].busy).toBe(false);
  run.mockResolvedValueOnce();
  await n.runNotificationAction(id, "primary");
  expect(n.notifications).toHaveLength(0);
});

it("动作更新同一条通知时保留新状态, 清空后忽略迟到结果", async () => {
  let complete!: () => void;
  const id = n.notify("开始", "info", 0, {
    key: "job",
    primary: {
      label: "执行",
      icon: "i-play",
      run: () =>
        new Promise<void>((done) => {
          complete = done;
        }),
    },
  });
  const pending = n.runNotificationAction(id, "primary");
  n.notify("新状态", "ok", 1000, { key: "job" });
  complete();
  await pending;
  expect(n.notifications[0].message).toBe("新状态");
  n.pauseNotifications("hover", true);
  n.resetNotifications();
  expect(vi.getTimerCount()).toBe(0);
  n.notify("再次打开", "ok", 100);
  await vi.advanceTimersByTimeAsync(100);
  expect(n.notifications).toHaveLength(0);
});
