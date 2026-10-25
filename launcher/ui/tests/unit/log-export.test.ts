import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import LogExport from "../../src/components/LogExport.vue";

const { open, invoke, isTauri, toast } = vi.hoisted(() => ({
  open: vi.fn(),
  invoke: vi.fn(),
  isTauri: vi.fn(),
  toast: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));
vi.mock("../../src/core/api", () => ({ invoke, isTauri }));
vi.mock("../../src/core/controller", () => ({ toast }));

beforeEach(() => {
  open.mockReset();
  invoke.mockReset();
  toast.mockReset();
  isTauri.mockReturnValue(true);
});

describe("日志导出", () => {
  it("选择目录后导出并显示实际路径", async () => {
    open.mockResolvedValue("/tmp/日志 导出");
    invoke.mockResolvedValue("/tmp/日志 导出/oddity-logs.zip");
    const wrapper = mount(LogExport);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(open).toHaveBeenCalledWith({
      title: "选择日志导出文件夹",
      directory: true,
      multiple: false,
    });
    expect(invoke).toHaveBeenCalledWith("export_logs", {
      destination: "/tmp/日志 导出",
    });
    expect(toast).toHaveBeenNthCalledWith(1, "正在导出日志...", "ok", 0);
    expect(toast).toHaveBeenNthCalledWith(2, "日志导出成功, 正在打开文件夹.");
    expect(invoke).toHaveBeenNthCalledWith(2, "open_directory", {
      path: "/tmp/日志 导出",
    });
    expect(wrapper.get('[role="status"]').text()).toContain(
      "/tmp/日志 导出/oddity-logs.zip",
    );
  });

  it("取消不导出也不报错", async () => {
    open.mockResolvedValue(null);
    const wrapper = mount(LogExport);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(invoke).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalled();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.get("button").attributes("disabled")).toBeUndefined();
  });

  it("导出期间禁用按钮, 失败后可以重试", async () => {
    open.mockResolvedValue("/tmp");
    let reject!: (cause: string) => void;
    invoke.mockReturnValueOnce(new Promise((_, fail) => (reject = fail)));
    const wrapper = mount(LogExport);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get("button").attributes("disabled")).toBeDefined();
    expect(toast).toHaveBeenCalledWith("正在导出日志...", "ok", 0);
    await wrapper.get("button").trigger("click");
    expect(invoke).toHaveBeenCalledTimes(1);
    reject("暂无可导出的日志");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("暂无可导出的日志");
    expect(invoke).not.toHaveBeenCalledWith(
      "open_directory",
      expect.anything(),
    );
    expect(toast).toHaveBeenLastCalledWith(
      expect.stringContaining("暂无可导出的日志"),
      "error",
      6500,
    );
    expect(wrapper.get("button").attributes("disabled")).toBeUndefined();
    invoke.mockResolvedValue("/tmp/logs.zip");
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.get('[role="status"]').text()).toContain("/tmp/logs.zip");
  });

  it("选择框失败时显示错误", async () => {
    open.mockRejectedValue(new Error("dialog unavailable"));
    const wrapper = mount(LogExport);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "dialog unavailable",
    );
  });

  it("打开文件夹失败仍保留导出结果并通知用户", async () => {
    open.mockResolvedValue("/tmp");
    invoke
      .mockResolvedValueOnce("/tmp/logs.zip")
      .mockRejectedValueOnce("opener failed");
    const wrapper = mount(LogExport);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="status"]').text()).toContain("/tmp/logs.zip");
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "日志已导出, 但无法打开文件夹",
    );
    expect(toast).toHaveBeenLastCalledWith(
      expect.stringContaining("opener failed"),
      "error",
      6500,
    );
  });

  it("浏览器预览明确提示需要桌面应用", async () => {
    isTauri.mockReturnValue(false);
    const wrapper = mount(LogExport);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(open).not.toHaveBeenCalled();
    expect(wrapper.get('[role="alert"]').text()).toContain("桌面应用");
  });
});
