import { mount, flushPromises, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type {
  AttendanceAccount,
  AttendanceStatus,
  GameRecord,
} from "../../src/types/launcher";
const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../src/core/api", () => ({
  invoke: api.invoke,
  isTauri: () => false,
}));
import * as c from "../../src/core/controller";
import Component from "../../src/components/HomeGameRecords.vue";

let wrapper: VueWrapper;
let status: AttendanceStatus;
const record = (overrides: Partial<GameRecord> = {}): GameRecord => ({
  roleId: "100000001",
  roleName: "调查员甲",
  serverName: "国服",
  stamina: 0,
  staminaMax: 240,
  cityStamina: null,
  cityStaminaMax: 700,
  activity: 60,
  weeklyRemaining: 0,
  updatedAt: 1700000000,
  ...overrides,
});
const account = (id: string): AttendanceAccount => ({
  id,
  maskedPhone: `138****800${id}`,
  rememberPassword: false,
  signedToday: false,
  failedToday: false,
  lastReport: null,
  attention: null,
});
beforeEach(() => {
  c.dispose();
  c.form.reduceMotion = true;
  Object.assign(c.attendance, {
    status: null,
    loaded: false,
    loading: false,
    busy: false,
    running: false,
    error: "",
  });
  status = {
    accounts: [account("1"), account("2")],
    preferences: { trigger: "off", time: "08:00" },
  };
  api.invoke.mockReset().mockImplementation(async (command) => {
    if (command === "attendance_refresh_game_records")
      status.accounts.forEach((a, i) => {
        a.gameRecord = record({ roleName: i ? "调查员乙" : "调查员甲" });
      });
    return structuredClone(status);
  });
});
afterEach(() => {
  wrapper?.unmount();
  c.dispose();
});
async function boot() {
  wrapper = mount(Component);
  await flushPromises();
  await wrapper.get(".game-records__capsule").trigger("click");
  await flushPromises();
}
const requests = () =>
  api.invoke.mock.calls.filter(
    ([cmd]) => cmd === "attendance_refresh_game_records",
  ).length;

it("切换查看全部账号、保留零值和未知字段, 手动刷新不执行签到", async () => {
  await boot();
  expect(wrapper.findAll(".game-records__account")).toHaveLength(1);
  expect(wrapper.text()).toContain("调查员甲");
  expect(wrapper.text()).not.toContain("调查员乙");
  await wrapper.get('[aria-label="下一个账号"]').trigger("click");
  await flushPromises();
  expect(wrapper.text()).toContain("调查员乙");
  expect(wrapper.text()).toContain("0 / 240");
  expect(wrapper.text()).toContain("-- / 700");
  expect(wrapper.text()).toContain("周本剩余0 次");
  expect(wrapper.text()).toContain("模拟数据");
  expect(requests()).toBe(1);
  await wrapper.get('[aria-label="刷新游戏状态"]').trigger("click");
  await flushPromises();
  expect(requests()).toBe(2);
  expect(api.invoke).not.toHaveBeenCalledWith("attendance_run");
});

it("空账号前往设置, 隐藏首页时停止网络刷新, 回到首页按间隔更新", async () => {
  status.accounts = [];
  await boot();
  expect(requests()).toBe(0);
  await wrapper.get(".game-records__link").trigger("click");
  expect(c.state.page).toBe("settings");
  status.accounts = [account("1")];
  c.syncAttendance(structuredClone(status));
  await flushPromises();
  expect(requests()).toBe(0);
  c.state.page = "home";
  await flushPromises();
  expect(requests()).toBe(1);
  await vi.advanceTimersByTimeAsync(29 * 60_000);
  expect(requests()).toBe(1);
  c.state.page = "settings";
  await vi.advanceTimersByTimeAsync(60_000);
  expect(requests()).toBe(1);
  c.state.page = "home";
  await flushPromises();
  expect(requests()).toBe(2);
});

it("单账号失效仍显示其他账号和上次数据, 提供重新登录入口", async () => {
  status.accounts[0].gameRecord = record();
  status.accounts[0].gameRecordError = { kind: "auth", message: "请重新登录" };
  api.invoke.mockImplementation(async () => structuredClone(status));
  await boot();
  expect(wrapper.text()).toContain("当前显示上次数据");
  expect(wrapper.findAll(".game-records__account")).toHaveLength(1);
  await wrapper.get(".game-records__link").trigger("click");
  expect(c.state.page).toBe("settings");
});

it("网络失败可重试, 恢复后清除错误, 忙碌时不重复查询", async () => {
  await boot();
  api.invoke.mockRejectedValueOnce({ message: "网络暂时不可用" });
  await c.refreshGameRecords(true);
  await flushPromises();
  expect(wrapper.text()).toContain("网络暂时不可用");
  expect(wrapper.text()).toContain("调查员甲");
  const failedRequests = requests();
  await vi.advanceTimersByTimeAsync(29 * 60_000);
  expect(requests()).toBe(failedRequests);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(requests()).toBe(failedRequests + 1);
  await c.refreshGameRecords(true);
  await flushPromises();
  expect(c.gameRecords.error).toBe("");
  c.attendance.busy = true;
  const before = requests();
  await c.refreshGameRecords(true);
  expect(requests()).toBe(before);
});

it("账号移除和组件卸载后丢弃旧请求, 防止已移除账号重新出现", async () => {
  await boot();
  let resolve!: (value: AttendanceStatus) => void;
  const old = structuredClone(status);
  api.invoke.mockImplementationOnce(
    () =>
      new Promise<AttendanceStatus>((done) => {
        resolve = done;
      }),
  );
  const pending = c.refreshGameRecords(true);
  c.syncAttendance({ ...status, accounts: [] });
  resolve(old);
  await pending;
  await flushPromises();
  expect(c.attendance.status?.accounts).toHaveLength(0);
  c.syncAttendance(old);
  await flushPromises();
  api.invoke.mockImplementationOnce(
    () =>
      new Promise<AttendanceStatus>((done) => {
        resolve = done;
      }),
  );
  const disposed = c.refreshGameRecords(true);
  wrapper.unmount();
  c.dispose();
  resolve({ ...status, accounts: [] });
  await disposed;
  expect(c.gameRecords.loading).toBe(false);
});

it("默认收起为胶囊, 胶囊与 Esc 收起, 选中账号移除后显示剩余账号", async () => {
  wrapper = mount(Component);
  await flushPromises();
  const capsule = wrapper.get(".game-records__capsule");
  expect(capsule.attributes("aria-expanded")).toBe("false");
  expect(wrapper.find(".game-records__panel").exists()).toBe(false);
  await capsule.trigger("click");
  await flushPromises();
  expect(capsule.attributes("aria-expanded")).toBe("true");
  await wrapper.get('[aria-label="下一个账号"]').trigger("click");
  await flushPromises();
  expect(wrapper.text()).toContain("调查员乙");
  status.accounts = [status.accounts[0]];
  c.syncAttendance(structuredClone(status));
  await flushPromises();
  expect(wrapper.text()).toContain("调查员甲");
  expect(wrapper.find('[aria-label="切换账号"]').exists()).toBe(false);
  await capsule.trigger("click");
  expect(capsule.attributes("aria-expanded")).toBe("false");
  await capsule.trigger("click");
  await wrapper.get(".game-records").trigger("keydown", { key: "Escape" });
  expect(capsule.attributes("aria-expanded")).toBe("false");
});
