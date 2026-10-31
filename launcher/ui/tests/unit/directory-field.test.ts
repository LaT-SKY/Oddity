import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import DirectoryField from "../../src/components/DirectoryField.vue";

const { open, homeDir, join } = vi.hoisted(() => ({
  open: vi.fn(),
  homeDir: vi.fn(async () => "/home/test"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("../../src/core/desktop", async (original) => ({
  ...(await original<typeof import("../../src/core/desktop")>()),
  open,
  homeDir,
  join,
}));

function field(modelValue = "/games/old", disabled = false) {
  return mount(DirectoryField, {
    props: { id: "path", label: "安装目录", modelValue, disabled },
  });
}

beforeEach(() => {
  open.mockReset();
});

describe("目录选择", () => {
  it("选择含空格和中文的目录后更新字段并触发保存", async () => {
    open.mockResolvedValue("/games/异环 Global");
    const wrapper = field("~/Games");
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(open).toHaveBeenCalledWith({
      title: "选择安装目录",
      directory: true,
      multiple: false,
      defaultPath: "/home/test/Games",
    });
    expect(wrapper.emitted("update:modelValue")).toEqual([
      ["/games/异环 Global"],
    ]);
    expect(wrapper.emitted("change")).toHaveLength(1);
  });

  it("取消不覆盖原值, 手动输入仍可保存", async () => {
    open.mockResolvedValue(null);
    const wrapper = field();
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    expect(wrapper.emitted("change")).toBeUndefined();
    await wrapper.get("input").setValue("/games/manual");
    expect(wrapper.emitted("update:modelValue")).toEqual([["/games/manual"]]);
    expect(wrapper.emitted("input")).toHaveLength(1);
    expect(wrapper.emitted("change")).toHaveLength(1);
  });

  it("禁用字段不能打开选择框", async () => {
    const wrapper = field("/games", true);
    await wrapper.get("button").trigger("click");
    expect(open).not.toHaveBeenCalled();
    expect(wrapper.get("input").attributes("disabled")).toBeDefined();
  });

  it("打开期间防止重复请求, 锁定后丢弃返回结果", async () => {
    let resolve!: (value: string) => void;
    open.mockReturnValue(new Promise<string>((done) => (resolve = done)));
    const wrapper = field();
    await wrapper.get("button").trigger("click");
    await wrapper.get("button").trigger("click");
    expect(open).toHaveBeenCalledTimes(1);
    await wrapper.setProps({ disabled: true });
    resolve("/games/new");
    await flushPromises();
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    expect(wrapper.emitted("change")).toBeUndefined();
  });

  it("打开失败时显示错误并允许重试", async () => {
    open.mockRejectedValueOnce(new Error("dialog unavailable"));
    const wrapper = field();
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "dialog unavailable",
    );
    expect(wrapper.get("button").attributes("disabled")).toBeUndefined();
    open.mockResolvedValue(null);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
  });
});
