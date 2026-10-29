import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PlaytimeHistory } from "../../src/types/launcher";

const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../src/core/api", () => ({
  invoke: api.invoke,
  isTauri: () => false,
}));
import * as controller from "../../src/core/controller";

const history: PlaytimeHistory = {
  sessions: [
    {
      startedAt: 1_000_000,
      endedAt: 1_120_000,
      mode: "background",
      distribution: "cn",
    },
  ],
  active: null,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  controller.dispose();
  api.invoke.mockReset();
});
afterEach(() => controller.dispose());

describe("游玩记录请求", () => {
  it("加载期间保留原记录, 成功后清除错误", async () => {
    controller.playtime.history = history;
    controller.playtime.error = "旧错误";
    const request = deferred<PlaytimeHistory>();
    api.invoke.mockReturnValueOnce(request.promise);
    const task = controller.loadPlaytime();
    expect(controller.playtime.loading).toBe(true);
    expect(controller.playtime.history).toEqual(history);
    const next = { sessions: [], active: null };
    request.resolve(next);
    expect(await task).toEqual(next);
    expect(api.invoke).toHaveBeenCalledExactlyOnceWith("playtime_history");
    expect(controller.playtime).toMatchObject({
      history: next,
      loading: false,
      loaded: true,
      error: "",
    });
  });

  it.each([new Error("读取失败"), "读取失败", { message: "读取失败" }])(
    "刷新失败保留之前已加载的记录并允许重试: %s",
    async (error) => {
      api.invoke
        .mockResolvedValueOnce(history)
        .mockRejectedValueOnce(error)
        .mockResolvedValueOnce({ sessions: [], active: null });
      await controller.loadPlaytime();
      expect(await controller.loadPlaytime()).toBeNull();
      expect(controller.playtime).toMatchObject({
        history,
        loaded: true,
        loading: false,
        error: "读取失败",
      });
      await controller.loadPlaytime();
      expect(controller.playtime).toMatchObject({
        history: { sessions: [], active: null },
        error: "",
        loading: false,
      });
    },
  );

  it("首次加载失败不标记为已加载", async () => {
    api.invoke.mockRejectedValueOnce(new Error("offline"));
    expect(await controller.loadPlaytime()).toBeNull();
    expect(controller.playtime).toMatchObject({
      history: null,
      loaded: false,
      loading: false,
      error: "offline",
    });
  });

  it.each(["resolve", "reject"] as const)(
    "旧请求先%s时不结束新请求的加载状态",
    async (settle) => {
      const old = deferred<PlaytimeHistory>();
      const current = deferred<PlaytimeHistory>();
      api.invoke
        .mockReturnValueOnce(old.promise)
        .mockReturnValueOnce(current.promise);
      const first = controller.loadPlaytime();
      const second = controller.loadPlaytime();
      if (settle === "resolve") old.resolve(history);
      else old.reject(new Error("过期错误"));
      expect(await first).toBeNull();
      expect(controller.playtime).toMatchObject({
        loading: true,
        history: null,
        error: "",
      });
      current.resolve(history);
      expect(await second).toEqual(history);
      expect(controller.playtime.loading).toBe(false);
    },
  );

  it.each(["resolve", "reject"] as const)(
    "旧请求后%s时不覆盖较新结果",
    async (settle) => {
      const old = deferred<PlaytimeHistory>();
      api.invoke
        .mockReturnValueOnce(old.promise)
        .mockResolvedValueOnce(history);
      const first = controller.loadPlaytime();
      await controller.loadPlaytime();
      if (settle === "resolve") old.resolve({ sessions: [], active: null });
      else old.reject(new Error("过期错误"));
      expect(await first).toBeNull();
      expect(controller.playtime).toMatchObject({
        history,
        loaded: true,
        loading: false,
        error: "",
      });
    },
  );

  it.each(["resolve", "reject"] as const)(
    "dispose 后%s的请求不污染新一轮状态",
    async (settle) => {
      const old = deferred<PlaytimeHistory>();
      const current = deferred<PlaytimeHistory>();
      api.invoke
        .mockReturnValueOnce(old.promise)
        .mockReturnValueOnce(current.promise);
      const first = controller.loadPlaytime();
      controller.dispose();
      const second = controller.loadPlaytime();
      if (settle === "resolve") old.resolve(history);
      else old.reject(new Error("过期错误"));
      expect(await first).toBeNull();
      expect(controller.playtime).toMatchObject({
        history: null,
        loading: true,
        loaded: false,
        error: "",
      });
      current.resolve(history);
      await second;
      expect(controller.playtime.history).toEqual(history);
    },
  );

  it("进入记录页才加载记录并同步地址", async () => {
    api.invoke.mockResolvedValue(history);
    controller.showPage("home");
    expect(api.invoke).not.toHaveBeenCalled();
    controller.showPage("records");
    await Promise.resolve();
    expect(controller.state.page).toBe("records");
    expect(window.location.hash).toBe("#records");
    expect(api.invoke).toHaveBeenCalledExactlyOnceWith("playtime_history");
  });
});
