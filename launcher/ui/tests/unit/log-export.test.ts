import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import LogExport from "../../src/components/LogExport.vue";
import type { LogPreview } from "../../src/types/launcher";

const { open, invoke, isDesktop, notify } = vi.hoisted(() => ({
  open: vi.fn(),
  invoke: vi.fn(),
  isDesktop: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../../src/core/desktop", () => ({ open }));
vi.mock("../../src/core/api", () => ({ invoke, isDesktop }));
vi.mock("../../src/core/notifications", () => ({ notify }));

const preview: LogPreview = {
  id: "1",
  fileCount: 2,
  redactions: { home: 3, phone: 1, credentials: 2, jwt: 0 },
  samples: [
    {
      file: "launch.log",
      line: 3,
      text: "prefix=$HOME/Games token=[凭据已移除]",
    },
  ],
  normalizedFiles: 1,
};
let wrapper: ReturnType<typeof mount>;
function render() {
  wrapper = mount(LogExport, {
    global: {
      stubs: {
        BaseModal: {
          props: ["open", "title"],
          template:
            '<section v-if="open" role="dialog"><h2>{{ title }}</h2><slot /></section>',
        },
      },
    },
  });
  return wrapper;
}
async function prepare() {
  render();
  await wrapper.get("#settings-export-logs").trigger("click");
  await flushPromises();
}
async function save() {
  await wrapper
    .get('[role="dialog"] .log-export__actions button:last-child')
    .trigger("click");
  await flushPromises();
}

beforeEach(() => {
  open.mockReset();
  invoke.mockReset();
  notify.mockReset();
  isDesktop.mockReturnValue(true);
  invoke.mockImplementation(async (command: string) =>
    command === "prepare_log_export"
      ? structuredClone(preview)
      : command === "save_log_export"
        ? "/tmp/日志 导出/logs.zip"
        : undefined,
  );
  open.mockResolvedValue("/tmp/日志 导出");
});
afterEach(() => wrapper?.unmount());

describe("日志隐私检查与导出", () => {
  it("展示统计并默认折叠预览, 确认后保存同一快照", async () => {
    await prepare();
    expect(open).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledExactlyOnceWith("prepare_log_export");
    expect(wrapper.get('[role="dialog"]').text()).toContain("2 个日志文件");
    expect(wrapper.find(".log-export__samples").exists()).toBe(false);
    await wrapper.get(".control-disclosure__trigger").trigger("click");
    expect(wrapper.text()).toContain("token=[凭据已移除]");
    await wrapper.get(".control-disclosure__trigger").trigger("click");
    expect(
      wrapper.get(".control-disclosure__trigger").attributes("aria-expanded"),
    ).toBe("false");
    expect(wrapper.text()).toContain("1 个文件存在编码异常");
    await save();
    expect(open).toHaveBeenCalledWith({
      title: "选择日志导出文件夹",
      directory: true,
      multiple: false,
    });
    expect(invoke).toHaveBeenCalledWith("save_log_export", {
      id: "1",
      destination: "/tmp/日志 导出",
    });
    expect(invoke).toHaveBeenLastCalledWith("open_directory", {
      path: "/tmp/日志 导出",
    });
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(wrapper.get('[role="status"]').text()).toContain(
      "/tmp/日志 导出/logs.zip",
    );
  });

  it("取消预览释放快照, 不创建导出文件", async () => {
    await prepare();
    await wrapper
      .get(".log-export__actions button:first-child")
      .trigger("click");
    await flushPromises();
    expect(invoke).toHaveBeenLastCalledWith("discard_log_export", { id: "1" });
    expect(open).not.toHaveBeenCalled();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(
      wrapper.get("#settings-export-logs").attributes("disabled"),
    ).toBeUndefined();
  });

  it("取消目录选择保留预览, 可以再次保存", async () => {
    open.mockResolvedValueOnce(null);
    await prepare();
    await save();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
    expect(invoke).toHaveBeenCalledTimes(1);
    await save();
    expect(wrapper.get('[role="status"]').text()).toContain("logs.zip");
  });

  it("检查期间不能重复提交, 检查失败可以重试", async () => {
    let reject!: (cause: string) => void;
    invoke.mockReturnValueOnce(new Promise((_, fail) => (reject = fail)));
    await prepare();
    expect(
      wrapper.get("#settings-export-logs").attributes("disabled"),
    ).toBeDefined();
    await wrapper.get("#settings-export-logs").trigger("click");
    expect(invoke).toHaveBeenCalledTimes(1);
    reject("暂无可导出的日志");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("暂无可导出的日志");
    expect(
      wrapper.get("#settings-export-logs").attributes("disabled"),
    ).toBeUndefined();
    await wrapper.get("#settings-export-logs").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
  });

  it("保存失败保留预览并使用相同 ID 重试", async () => {
    await prepare();
    invoke.mockRejectedValueOnce("权限不足");
    await save();
    expect(wrapper.get('[role="alert"]').text()).toContain("权限不足");
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
    await save();
    expect(
      invoke.mock.calls.filter(([command]) => command === "save_log_export"),
    ).toHaveLength(2);
    expect(wrapper.get('[role="status"]').text()).toContain("logs.zip");
  });

  it("选择目录失败可重试, 打开目录失败仍显示已保存结果", async () => {
    await prepare();
    open.mockRejectedValueOnce(new Error("dialog unavailable"));
    await save();
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "dialog unavailable",
    );
    invoke.mockImplementation(async (command: string) => {
      if (command === "open_directory") throw new Error("opener failed");
      return "/tmp/logs.zip";
    });
    await save();
    expect(wrapper.get('[role="status"]').text()).toContain("/tmp/logs.zip");
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "日志已导出, 但无法打开文件夹",
    );
  });

  it("没有命中时仍提供明确预览, 取消失败保留状态", async () => {
    invoke.mockResolvedValueOnce({
      ...preview,
      samples: [],
      normalizedFiles: 0,
    });
    await prepare();
    await wrapper.get(".control-disclosure__trigger").trigger("click");
    expect(wrapper.text()).toContain("未检测到敏感信息");
    invoke.mockRejectedValueOnce("取消失败");
    await wrapper
      .get(".log-export__actions button:first-child")
      .trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("取消失败");
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
  });

  it("卸载组件释放预览, 检查在卸载后完成也会释放", async () => {
    await prepare();
    wrapper.unmount();
    expect(invoke).toHaveBeenLastCalledWith("discard_log_export", { id: "1" });
    let resolve!: (value: LogPreview) => void;
    invoke.mockReturnValueOnce(new Promise((done) => (resolve = done)));
    await prepare();
    wrapper.unmount();
    resolve(preview);
    await flushPromises();
    expect(invoke).toHaveBeenLastCalledWith("discard_log_export", { id: "1" });
  });

  it("浏览器预览明确提示需要桌面应用", async () => {
    isDesktop.mockReturnValue(false);
    await prepare();
    expect(invoke).not.toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
    expect(wrapper.get('[role="alert"]').text()).toContain("桌面应用");
  });
});
