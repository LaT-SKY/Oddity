import { mount, flushPromises, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type {
  AttendanceAccount,
  AttendanceStatus,
} from "../../src/types/launcher";
const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../../src/core/api", () => ({
  invoke: api.invoke,
  isTauri: () => false,
}));
import * as c from "../../src/core/controller";
import { bootstrap } from "./fixtures";
import Component from "../../src/components/TaygedoSettings.vue";
let wrapper: VueWrapper;
let status: AttendanceStatus;
const account = (id: string): AttendanceAccount => ({
  id,
  maskedPhone: id === "1" ? "138****8000" : "139****9000",
  rememberPassword: false,
  signedToday: false,
  failedToday: false,
  lastReport: null,
  attention: null,
});
beforeEach(() => {
  Object.assign(c.attendance, {
    status: null,
    loaded: false,
    loading: false,
    busy: false,
    running: false,
    error: "",
  });
  status = { accounts: [], preferences: { trigger: "off", time: "08:00" } };
  api.invoke.mockReset().mockImplementation(async (command, args) => {
    if (command === "save_settings") return bootstrap();
    if (command === "attendance_send_code") return "device-test";
    if (command === "attendance_login" || command === "attendance_login_sms")
      status.accounts.push(account(String(status.accounts.length + 1)));
    if (command === "attendance_logout")
      status.accounts = status.accounts.filter((a) => a.id !== args.accountId);
    if (command === "attendance_configure")
      status.preferences = args.preferences;
    if (command === "attendance_run")
      status.accounts.forEach((a) => {
        a.signedToday = true;
        a.failedToday = false;
      });
    return structuredClone(status);
  });
});
afterEach(() => {
  wrapper?.unmount();
  document.body.innerHTML = "";
});
async function boot() {
  wrapper = mount(Component, { attachTo: document.body });
  await flushPromises();
}
function el(selector: string) {
  return document.querySelector<HTMLElement>(selector)!;
}
async function click(selector: string) {
  el(selector).click();
  await flushPromises();
}
async function login() {
  for (const [id, value] of [
    ["attendance-phone", "13800138000"],
    ["attendance-password", "test-password"],
  ]) {
    const input = el(`#${id}`) as HTMLInputElement;
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }
  el("[role=dialog] form").dispatchEvent(
    new Event("submit", { bubbles: true, cancelable: true }),
  );
  await flushPromises();
}
it("多账号添加、统一保存、签到和两次确认移除", async () => {
  await boot();
  expect(wrapper.get("#attendance-action").text()).toBe("立即登录");
  await click("#attendance-action");
  await login();
  expect(wrapper.find("#attendance-manage").exists()).toBe(true);
  expect(wrapper.text()).not.toContain("138****8000");
  expect(el("[role=dialog]").textContent).toContain("138****8000");
  expect(el("[role=dialog]").textContent).toContain("今日未签到");
  await click('[aria-label="关闭弹窗"]');
  await wrapper.get("#attendance-enabled").setValue(true);
  c.attendanceForm.trigger = "daily";
  c.attendanceForm.time = "20:30";
  await c.saveSettings();
  expect(api.invoke).toHaveBeenCalledWith("attendance_configure", {
    preferences: { trigger: "daily", time: "20:30" },
  });
  await click("#attendance-action");
  expect(wrapper.get("#attendance-action").text()).toBe("今日已签到");
  expect((el("#attendance-action") as HTMLButtonElement).disabled).toBe(true);
  await click("#attendance-manage");
  const add = Array.from(
    document.querySelectorAll<HTMLButtonElement>("[role=dialog] button"),
  ).find((b) => b.textContent?.includes("添加账号"))!;
  add.click();
  await flushPromises();
  await login();
  expect(document.querySelectorAll(".account-row")).toHaveLength(2);
  for (let i = 0; i < 2; i++) {
    const buttons = () =>
      Array.from(
        document.querySelectorAll<HTMLButtonElement>("[role=dialog] button"),
      );
    buttons()
      .find((b) => b.textContent?.trim() === "移除")!
      .click();
    await flushPromises();
    expect(status.accounts).toHaveLength(2 - i);
    buttons()
      .find((b) => b.textContent?.trim() === "确认移除")!
      .click();
    await flushPromises();
  }
  await click('[aria-label="关闭弹窗"]');
  expect(wrapper.get("#attendance-action").text()).toBe("立即登录");
  expect(wrapper.find("#attendance-manage").exists()).toBe(false);
  expect(wrapper.text()).not.toMatch(/签到已完成|社区签到:/);
});
it("验证原因仅在弹窗显示, 登录失败也清空密码", async () => {
  status.accounts = [account("1")];
  status.accounts[0].attention = {
    kind: "verification",
    message: "需要人工验证",
  };
  await boot();
  expect(wrapper.get("#attendance-action").text()).toBe("重新登录");
  expect(wrapper.text()).not.toContain("需要人工验证");
  await click("#attendance-action");
  expect(el("[role=dialog]").textContent).toContain("需要人工验证");
  Array.from(
    document.querySelectorAll<HTMLButtonElement>("[role=dialog] button"),
  )
    .find((b) => b.textContent?.trim() === "重新登录")!
    .click();
  await flushPromises();
  api.invoke.mockRejectedValueOnce({ message: "登录失败" });
  await login();
  expect((el("#attendance-password") as HTMLInputElement).value).toBe("");
  expect(el("[role=dialog] [role=alert]").textContent).toBe("登录失败");
});
it("签到与轮询保留未保存偏好, 弹窗循环焦点并还原", async () => {
  status.accounts = [account("1")];
  await boot();
  c.attendanceForm.enabled = true;
  c.attendanceForm.time = "21:30";
  await c.runAttendance();
  await c.loadAttendance();
  expect(c.attendanceForm.time).toBe("21:30");
  const opener = el("#attendance-manage");
  opener.focus();
  await click("#attendance-manage");
  const close = el('[aria-label="关闭弹窗"]');
  expect(document.activeElement).toBe(close);
  close.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Tab",
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    }),
  );
  expect(document.activeElement?.textContent).toContain("添加账号");
  el("[role=dialog]").dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
  );
  await flushPromises();
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(document.activeElement).toBe(opener);
});
it("设置页只显示失败汇总, 账号原因留在弹窗", async () => {
  status.accounts = [account("1")];
  status.accounts[0].failedToday = true;
  status.accounts[0].lastReport = {
    at: Math.floor(Date.now() / 1000),
    ok: false,
    steps: [
      {
        label: "社区签到",
        ok: false,
        message: "网络失败",
        kind: "network",
      },
    ],
  };
  await boot();
  expect(wrapper.text()).toContain("1 个账号签到失败.");
  expect(wrapper.text()).not.toContain("网络失败");
  expect(wrapper.text()).not.toContain("138****8000");
  await click("#attendance-failure-details");
  expect(el("[role=dialog]").textContent).toContain("网络失败");
});

async function input(selector: string, value: string) {
  const field = el(selector) as HTMLInputElement;
  field.value = value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await flushPromises();
}
async function button(text: string) {
  Array.from(
    document.querySelectorAll<HTMLButtonElement>("[role=dialog] button"),
  )
    .find((b) => b.textContent?.trim() === text)!
    .click();
  await flushPromises();
}
async function submitSms() {
  el("[role=dialog] form").dispatchEvent(
    new Event("submit", { bubbles: true, cancelable: true }),
  );
  await flushPromises();
}
it("短信登录传递设备身份, 隐藏保存密码并清空失败验证码", async () => {
  await boot();
  await click("#attendance-action");
  await button("短信验证码");
  expect(document.querySelector(".attendance__remember")).toBeNull();
  await input("#attendance-phone", "13800138000");
  await button("发送验证码");
  expect(api.invoke).toHaveBeenCalledWith("attendance_send_code", {
    phone: "13800138000",
  });
  expect(el("[role=dialog]").textContent).toContain("重新发送 (60s)");
  await input("#attendance-code", "123456");
  api.invoke.mockRejectedValueOnce({ message: "验证码错误" });
  await submitSms();
  expect((el("#attendance-code") as HTMLInputElement).value).toBe("");
  expect(el("[role=dialog] [role=alert]").textContent).toBe("验证码错误");
  await input("#attendance-code", "123456");
  await submitSms();
  expect(api.invoke).toHaveBeenCalledWith("attendance_login_sms", {
    phone: "13800138000",
    code: "123456",
    deviceId: "device-test",
  });
  expect(document.querySelectorAll(".account-row")).toHaveLength(1);
});
it("手机号变化要求重新发送, 切换登录方式清理凭据和倒计时", async () => {
  await boot();
  await click("#attendance-action");
  await button("短信验证码");
  await input("#attendance-phone", "13800138000");
  await button("发送验证码");
  await input("#attendance-phone", "13900139000");
  expect(el("[role=dialog]").textContent).not.toContain("重新发送 (");
  await input("#attendance-code", "123456");
  await submitSms();
  expect(el("[role=dialog] [role=alert]").textContent).toBe(
    "请先为当前手机号获取验证码.",
  );
  expect(api.invoke).not.toHaveBeenCalledWith(
    "attendance_login_sms",
    expect.anything(),
  );
  await button("密码登录");
  await button("短信验证码");
  expect((el("#attendance-phone") as HTMLInputElement).value).toBe(
    "13900139000",
  );
  expect(el("[role=dialog]").textContent).not.toContain("重新发送 (");
  expect(document.querySelector("[role=dialog] [role=alert]")).toBeNull();
});
it("开机时选项可以统一保存", async () => {
  status.accounts = [account("1")];
  await boot();
  await wrapper.get("#attendance-enabled").setValue(true);
  await click("#attendance-trigger-trigger");
  const option = Array.from(
    document.querySelectorAll<HTMLElement>("[role=option]"),
  ).find((e) => e.textContent?.trim() === "开机时");
  expect(option).toBeDefined();
  option!.click();
  await flushPromises();
  expect(wrapper.text()).toContain("用户登录后执行一次");
  await c.saveSettings();
  expect(api.invoke).toHaveBeenCalledWith("attendance_configure", {
    preferences: { trigger: "boot", time: "08:00" },
  });
});

it("短信冷却在 60 秒结束, 关闭和卸载清理计时器", async () => {
  vi.useFakeTimers();
  try {
    await boot();
    await click("#attendance-action");
    await button("短信验证码");
    await input("#attendance-phone", "13800138000");
    await button("发送验证码");
    await vi.advanceTimersByTimeAsync(59000);
    expect(el("[role=dialog]").textContent).toContain("重新发送 (1s)");
    await vi.advanceTimersByTimeAsync(1000);
    await button("发送验证码");
    expect(vi.getTimerCount()).toBe(2);
    await click('[aria-label="关闭弹窗"]');
    expect(vi.getTimerCount()).toBe(1);
    await click("#attendance-action");
    expect(el("[role=tab][aria-selected=true]").textContent).toBe("密码登录");
    await button("短信验证码");
    await input("#attendance-phone", "13800138000");
    await button("发送验证码");
    wrapper.unmount();
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.getTimerCount()).toBe(0);
  } finally {
    vi.useRealTimers();
  }
});
