import { APP_VERSION } from "../../src/core/version";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { settings } from "./fixtures";
import type { GameStatus } from "../../src/types/launcher";

const native = vi.hoisted(() => ({
  enabled: false,
  invoke: vi.fn(),
  listen: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => native.enabled,
  invoke: native.invoke,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: native.listen }));
beforeEach(() => {
  native.enabled = false;
  native.invoke.mockReset();
  native.listen.mockReset();
  delete window.__NTE_PREVIEW__;
});

describe("浏览器命令桥接", () => {
  it("使用默认路径并返回互不共享的启动快照", async () => {
    const { invoke, listenGameState } = await import("../../src/core/api");
    const first = await invoke("bootstrap");
    expect(first.appVersion).toBe(APP_VERSION);
    expect(first.installRoot).toBe("~/Games/Neverness To Everness");
    first.settings.theme = "dark";
    first.checks.length = 0;
    const second = await invoke("bootstrap");
    expect(second.settings.theme).toBe("light");
    expect(second.checks).toHaveLength(6);
    listenGameState(vi.fn())();
    expect(native.listen).not.toHaveBeenCalled();
  });

  it("查询参数优先于本地存储和窗口配置, 并清理路径斜杠", async () => {
    window.__NTE_PREVIEW__ = {
      home: "/window/",
      dataHome: "/data/",
      protonName: "Window",
    };
    localStorage.setItem(
      "nte.preview.paths",
      JSON.stringify({
        home: "/stored/",
        steamRoot: "/steam/",
        protonName: "Stored",
      }),
    );
    history.replaceState(
      null,
      "",
      "/?home=%20/query/%20&installRoot=/game/&protonName=Query",
    );
    const { invoke } = await import("../../src/core/api");
    const data = await invoke("bootstrap");
    expect(data.installRoot).toBe("/game/");
    expect(data.launcherRoot).toBe("/game/NTELauncher");
    expect(data.protonPath).toBe("/steam/compatibilitytools.d/Query/proton");
    expect(data.protonName).toBe("Query");
  });

  it.each(["{broken", "null", "42"])("忽略无效本地配置: %s", async (stored) => {
    localStorage.setItem("nte.preview.paths", stored);
    history.replaceState(
      null,
      "",
      "/?home=++&dataHome=&steamRoot=&installRoot=&protonName=",
    );
    const { invoke } = await import("../../src/core/api");
    expect((await invoke("bootstrap")).prefix).toBe(
      "~/.local/share/oddity/prefix",
    );
  });

  it.each([
    ["Win32", "x86_64", "windows / x86_64"],
    ["MacIntel", "arm64", "macos / aarch64"],
    ["Linux", "aarch64", "linux / aarch64"],
    ["", "other", "linux / x86_64"],
  ])("显示预览平台 %s / %s", async (platform, agent, expected) => {
    vi.spyOn(navigator, "platform", "get").mockReturnValue(platform);
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue(agent);
    const { invoke } = await import("../../src/core/api");
    expect((await invoke("bootstrap")).os).toBe(
      `${expected} / browser preview`,
    );
  });

  it("检查、保存、启动、关闭及窗口命令维持预览状态", async () => {
    const { invoke } = await import("../../src/core/api");
    const update = invoke("check_update", { full: true });
    await vi.advanceTimersByTimeAsync(620);
    expect(await update).toMatchObject({
      installed: true,
      distribution: "cn",
      installRoot: "~/Games/Neverness To Everness",
      missing: 0,
      changed: 0,
      files: 667,
    });
    expect(
      await invoke("save_settings", {
        settings: { ...settings, installRoot: "/new", theme: "dark" },
      }),
    ).toMatchObject({ installRoot: "/new", settings: { theme: "dark" } });
    await invoke("save_settings", { settings: { ...settings } });
    expect((await invoke("bootstrap")).installRoot).toBe("/new");
    expect(await invoke("launch_official")).toMatchObject({
      mode: "official",
      pid: 23140,
    });
    const snapshot = await invoke("game_status");
    expect(snapshot.running).toBe(true);
    snapshot.running = false;
    expect((await invoke("game_status")).running).toBe(true);
    const closing = invoke("close_game");
    expect((await invoke("game_status")).closing).toBe(true);
    await vi.advanceTimersByTimeAsync(850);
    expect(await closing).toMatchObject({
      running: false,
      closing: false,
      pid: 0,
    });
    expect(await invoke("open_url", { url: "https://example.com" })).toBeNull();
    for (const command of [
      "win_minimize",
      "win_toggle_maximize",
      "win_close",
      "win_hide",
      "win_drag",
    ] as const) {
      expect(await invoke(command)).toBeNull();
    }
  });
});

describe("原生命令和事件", () => {
  beforeEach(() => {
    native.enabled = true;
  });
  it("原样转发命令参数、返回值和错误", async () => {
    const { invoke } = await import("../../src/core/api");
    native.invoke
      .mockResolvedValueOnce({ saved: true })
      .mockRejectedValueOnce(new Error("offline"));
    await expect(invoke("save_settings", { settings })).resolves.toEqual({
      saved: true,
    });
    expect(native.invoke).toHaveBeenCalledWith("save_settings", { settings });
    await expect(invoke("bootstrap")).rejects.toThrow("offline");
  });

  it("传递事件快照并在订阅完成后解除监听", async () => {
    const { listenGameState } = await import("../../src/core/api");
    const stop = vi.fn();
    native.listen.mockResolvedValue(stop);
    const handler = vi.fn();
    const dispose = listenGameState(handler);
    const callback = native.listen.mock.calls[0][1] as (event: {
      payload: GameStatus;
    }) => void;
    const payload = { running: true } as GameStatus;
    callback({ payload });
    expect(handler).toHaveBeenCalledWith(payload);
    await Promise.resolve();
    dispose();
    expect(stop).toHaveBeenCalledOnce();
  });

  it("在异步订阅完成前解除监听也能释放资源", async () => {
    const { listenGameState } = await import("../../src/core/api");
    let resolve!: (stop: () => void) => void;
    native.listen.mockReturnValue(
      new Promise<() => void>((done) => {
        resolve = done;
      }),
    );
    const dispose = listenGameState(vi.fn());
    dispose();
    const stop = vi.fn();
    resolve(stop);
    await Promise.resolve();
    expect(stop).toHaveBeenCalledOnce();
  });

  it("监听注册失败时不产生未处理的拒绝", async () => {
    const { listenGameState } = await import("../../src/core/api");
    native.listen.mockRejectedValue(new Error("listener failed"));
    const dispose = listenGameState(vi.fn());
    await Promise.resolve();
    await Promise.resolve();
    expect(dispose).not.toThrow();
  });
});

it("游戏状态预览覆盖全部账号, 保留零次周本且不改变签到状态", async () => {
  const { invoke } = await import("../../src/core/api");
  expect((await invoke("attendance_refresh_game_records")).accounts).toEqual(
    [],
  );
  for (const phone of ["13800138000", "13900139000"])
    await invoke("attendance_login", {
      phone,
      password: "preview-only",
      rememberPassword: false,
    });
  const result = await invoke("attendance_refresh_game_records");
  expect(result.accounts.map((a) => a.gameRecord?.weeklyRemaining)).toEqual([
    2, 0,
  ]);
  expect(result.accounts.map((a) => a.gameRecord?.stamina)).toEqual([168, 240]);
  expect(
    result.accounts.every((a) => a.lastReport === null && !a.signedToday),
  ).toBe(true);
  result.accounts.length = 0;
  expect((await invoke("attendance_status")).accounts).toHaveLength(2);
});

it("签到预览默认关闭, 手机号密码登录不保存原始凭据且不会自动开启", async () => {
  const { invoke } = await import("../../src/core/api");
  expect((await invoke("attendance_status")).preferences.trigger).toBe("off");
  await expect(invoke("attendance_run")).rejects.toThrow("请先登录");
  await expect(
    invoke("attendance_login", {
      phone: "bad",
      password: "",
      rememberPassword: false,
    }),
  ).rejects.toThrow();
  await expect(
    invoke("attendance_configure", {
      preferences: { trigger: "daily", time: "20:30" },
    }),
  ).rejects.toThrow("请先登录");
  const logged = await invoke("attendance_login", {
    phone: "13800138000",
    password: "test-password",
    rememberPassword: true,
  });
  expect(logged.accounts).toHaveLength(1);
  expect(logged.accounts[0].rememberPassword).toBe(true);
  expect(logged.preferences.trigger).toBe("off");
  expect(logged.accounts[0].maskedPhone).toBe("138****8000");
  expect(JSON.stringify(logged)).not.toContain("test-password");
  expect(JSON.stringify(logged)).not.toContain("13800138000");
  expect(localStorage.length).toBe(0);
  await invoke("attendance_keyring_check");
  await invoke("attendance_configure", {
    preferences: { trigger: "launch", time: "08:00" },
  });
  expect((await invoke("attendance_run")).accounts[0].lastReport?.ok).toBe(
    true,
  );
  const second = await invoke("attendance_login", {
    phone: "13900139000",
    password: "other-password",
    rememberPassword: false,
  });
  expect(second.accounts).toHaveLength(2);
  const relogged = await invoke("attendance_login", {
    phone: "13800138000",
    password: "new-password",
    rememberPassword: false,
  });
  expect(relogged.accounts.map((account) => account.id)).toEqual(
    second.accounts.map((account) => account.id),
  );
  expect(relogged.accounts[0].lastReport).toBeNull();
  expect(relogged.accounts[0].signedToday).toBe(false);
  const signed = await invoke("attendance_run");
  expect(signed.accounts.every((account) => account.signedToday)).toBe(true);
  const remaining = await invoke("attendance_logout", {
    accountId: second.accounts[1].id,
  });
  expect(remaining.accounts).toHaveLength(1);
  expect(remaining.preferences.trigger).toBe("launch");
  const cleared = await invoke("attendance_logout", {
    accountId: logged.accounts[0].id,
  });
  expect(cleared.accounts).toHaveLength(0);
  expect(cleared.preferences.trigger).toBe("off");
});

it("预览短信登录校验输入并更新同一账号", async () => {
  const { invoke } = await import("../../src/core/api");
  await expect(
    invoke("attendance_send_code", { phone: "123" }),
  ).rejects.toThrow("手机号码");
  const phone = "13700137000";
  const deviceId = await invoke("attendance_send_code", { phone });
  expect(await invoke("attendance_send_code", { phone })).toBe(deviceId);
  await expect(
    invoke("attendance_login_sms", { phone, code: "abc", deviceId }),
  ).rejects.toThrow("验证码");
  await expect(
    invoke("attendance_login_sms", { phone: "1", code: "123456", deviceId }),
  ).rejects.toThrow("手机号码");
  await expect(
    invoke("attendance_login_sms", {
      phone,
      code: "123456",
      deviceId: "wrong",
    }),
  ).rejects.toThrow("获取验证码");
  const status = await invoke("attendance_login_sms", {
    phone,
    code: "123456",
    deviceId,
  });
  const entry = status.accounts.find((a) => a.maskedPhone === "137****7000")!;
  expect(entry.rememberPassword).toBe(false);
  expect(JSON.stringify(status)).not.toContain("123456");
  const again = await invoke("attendance_login_sms", {
    phone,
    code: "123456",
    deviceId,
  });
  expect(again.accounts.filter((a) => a.id === entry.id)).toHaveLength(1);
  await invoke("attendance_logout", { accountId: entry.id });
});
