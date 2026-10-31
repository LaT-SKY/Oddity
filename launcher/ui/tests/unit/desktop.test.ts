import { afterEach, expect, it, vi } from "vitest";
import {
  isDesktop,
  nativeInvoke,
  listen,
  desktopWindow,
  homeDir,
  join,
  open,
  save,
  type DesktopBridge,
} from "../../src/core/desktop";

afterEach(() => {
  delete window.oddity;
});
it("原生命令和结构化错误透传, 事件订阅可解除", async () => {
  expect(isDesktop()).toBe(false);
  const callbacks = new Map<string, (payload: unknown) => void>();
  const unlisten = vi.fn();
  const bridge = {
    invoke: vi.fn().mockResolvedValue({ running: false }),
    listen: vi.fn((event, callback) => {
      callbacks.set(event, callback);
      return unlisten;
    }),
    windowState: vi.fn().mockResolvedValue({ maximized: true, visible: true }),
    homeDir: vi.fn().mockResolvedValue("/home/player"),
    open: vi.fn().mockResolvedValue("/game"),
    save: vi.fn().mockResolvedValue(null),
  } as unknown as DesktopBridge;
  window.oddity = bridge;
  expect(isDesktop()).toBe(true);
  expect(await nativeInvoke("game_status")).toEqual({ running: false });
  const failure = { kind: "keyring", message: "无法解锁密钥环" };
  vi.mocked(bridge.invoke).mockRejectedValueOnce(failure);
  await expect(nativeInvoke("attendance_status")).rejects.toBe(failure);
  const event = vi.fn();
  const unsubscribe = await listen("game-state", event);
  callbacks.get("game-state")!({ running: true });
  expect(event).toHaveBeenCalledWith({ payload: { running: true } });
  unsubscribe();
  expect(unlisten).toHaveBeenCalledOnce();
  expect(await desktopWindow.getState()).toEqual({
    maximized: true,
    visible: true,
  });
  const resize = vi.fn();
  const stopResize = await desktopWindow.onResized(resize);
  callbacks.get("window-state")!({ maximized: false });
  expect(resize).toHaveBeenCalledOnce();
  stopResize();
  expect(await homeDir()).toBe("/home/player");
  expect(await join("/home/player/", "/Games/", "NTE")).toBe(
    "/home/player/Games/NTE",
  );
  expect(await open({ directory: true })).toBe("/game");
  expect(await save({ title: "导出日志" })).toBeNull();
});
