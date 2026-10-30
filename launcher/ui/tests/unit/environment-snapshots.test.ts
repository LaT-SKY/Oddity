import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import BaseSelect from "../../src/components/BaseSelect.vue";
import EnvironmentSnapshots from "../../src/components/EnvironmentSnapshots.vue";
import type {
  SnapshotComparison,
  SnapshotOverview,
  SnapshotReference,
} from "../../src/types/launcher";

const { invoke, notify } = vi.hoisted(() => ({
  invoke: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../../src/core/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../src/core/api")>();
  return { ...actual, invoke };
});
vi.mock("../../src/core/notifications", () => ({ notify }));

let overview: SnapshotOverview;
let comparison: SnapshotComparison;
let compareArgs: unknown[] = [];
let savedLabel: string | null = null;
let reference: SnapshotReference;

function reset() {
  const now = Date.now();
  reference = {
    label: "更新驱动前",
    savedAt: now - 172_800_000,
    capturedAt: now - 172_800_000,
    environment: "测试环境",
    proton: "GE-Proton9-20",
    distribution: "cn",
    mode: "background",
  };
  overview = {
    baseline: {
      id: "baseline",
      capturedAt: now - 86_400_000,
      status: "success",
      durationMs: 42 * 60_000,
      failure: null,
      environment: "测试环境",
      proton: "GE-Proton9-20",
      distribution: "cn",
      mode: "background",
    },
    reference,
    latest: {
      id: "latest",
      capturedAt: now - 3_600_000,
      status: "failed",
      durationMs: 4_000,
      failure: { reason: "earlyExit", exitCode: 3, detail: null },
      environment: "测试环境",
      proton: "GE-Proton9-21",
      distribution: "cn",
      mode: "background",
    },
    attempts: 3,
  };
  comparison = {
    target: "baseline",
    targetLabel: "上次成功启动",
    targetTime: now - 86_400_000,
    targetEnvironment: "测试环境",
    currentEnvironment: "测试环境",
    rows: [
      {
        key: "runtime.proton",
        section: "运行环境",
        label: "Proton",
        baseline: "GE-Proton9-20",
        current: "GE-Proton9-21",
        status: "changed",
      },
      {
        key: "graphics.opengl",
        section: "图形",
        label: "OpenGL 驱动",
        baseline: "NVIDIA 615.78.08",
        current: null,
        status: "unknown",
      },
      {
        key: "system.session",
        section: "系统",
        label: "会话类型",
        baseline: "Wayland",
        current: "Wayland",
        status: "same",
      },
      {
        key: "game.version",
        section: "游戏",
        label: "游戏版本",
        baseline: "1.0.7",
        current: "1.0.7",
        status: "same",
      },
    ],
    changed: 1,
    unknown: 1,
    unchanged: 2,
  };
  compareArgs = [];
  savedLabel = null;
}

function configureMock() {
  invoke.mockReset();
  notify.mockReset();
  invoke.mockImplementation(async (command: string, args?: unknown) => {
    if (command === "environment_snapshot_overview")
      return structuredClone(overview);
    if (command === "environment_snapshot_compare") {
      compareArgs.push(args);
      const target = (args as { target: "baseline" | "reference" }).target;
      return {
        ...structuredClone(comparison),
        target,
        targetLabel: target === "reference" ? "参考快照" : "上次成功启动",
      };
    }
    if (command === "environment_snapshot_save_reference") {
      savedLabel = (args as { label: string | null }).label;
      overview.reference = { ...reference, label: savedLabel ?? "参考快照" };
      return structuredClone(overview.reference);
    }
    if (command === "environment_snapshot_delete_reference") {
      overview.reference = null;
      return null;
    }
    if (command === "environment_snapshot_clear_history") {
      overview.baseline = null;
      overview.latest = null;
      overview.attempts = 0;
      return null;
    }
    return undefined;
  });
}

function render() {
  return mount(EnvironmentSnapshots, { attachTo: document.body });
}

function settingsText() {
  return wrapper.get(".snapshot-settings").text();
}

function dialog(text?: string) {
  const elements = Array.from(
    document.querySelectorAll<HTMLElement>('[role="dialog"]'),
  );
  const element = text
    ? elements.find((element) => element.textContent?.includes(text))
    : elements.at(-1);
  if (!element) throw new Error("弹窗未打开: " + (text ?? ""));
  return element;
}

function clickButton(root: ParentNode, label: string) {
  const button = Array.from(root.querySelectorAll("button")).find(
    (button) => button.textContent?.trim() === label,
  );
  if (!button) throw new Error("找不到按钮: " + label);
  button.click();
}

let wrapper: ReturnType<typeof mount>;
beforeEach(() => {
  reset();
  configureMock();
  const app = document.createElement("div");
  app.id = "app";
  document.body.appendChild(app);
});
afterEach(() => wrapper?.unmount());

describe("环境快照", () => {
  it("显示基准, 参考快照和失败结果", async () => {
    wrapper = render();
    await flushPromises();
    expect(settingsText()).toContain("更新驱动前");
    expect(settingsText()).toContain("GE-Proton9-20");
    expect(settingsText()).toContain("启动失败");
    expect(settingsText()).toContain("提前退出 (退出码 3)");
    expect(settingsText()).toContain("上次成功启动");
  });

  it("提供 1/5/10/30 分钟的成功判定选项", async () => {
    wrapper = render();
    await flushPromises();
    const select = wrapper.getComponent(BaseSelect);
    expect(
      (select.props("options") as { value: string }[]).map(
        (option) => option.value,
      ),
    ).toEqual(["1", "5", "10", "30"]);
  });

  it("按设置显示成功判定时长", async () => {
    wrapper = mount(EnvironmentSnapshots, {
      attachTo: document.body,
      props: { successMinutes: 10 },
    });
    await flushPromises();
    expect(settingsText()).toContain("10 分钟");
  });

  it("默认对比上次成功, 区分变化, 未采集和未变化", async () => {
    wrapper = render();
    await flushPromises();
    await wrapper.get('[data-testid="compare-baseline"]').trigger("click");
    await flushPromises();
    expect(compareArgs).toEqual([{ target: "baseline" }]);
    const panel = dialog();
    expect(panel.textContent).toContain("1 项变化");
    expect(panel.textContent).toContain("1 项无法比较");
    expect(panel.textContent).toContain("Proton");
    expect(panel.textContent).toContain("OpenGL 驱动");
    expect(panel.textContent).toContain("无法比较");
    expect(panel.textContent).not.toContain("会话类型");
    const disclosure = panel.querySelector<HTMLElement>(
      ".control-disclosure__trigger",
    );
    expect(disclosure).not.toBeNull();
    disclosure!.click();
    await flushPromises();
    expect(panel.textContent).toContain("会话类型");
    expect(panel.textContent).toContain("未变化 (2)");
  });

  it("保存参考快照后可以替换和删除", async () => {
    overview.reference = null;
    wrapper = render();
    await flushPromises();
    await wrapper.get('[data-testid="save-reference"]').trigger("click");
    await flushPromises();
    const saveDialog = dialog();
    const input = saveDialog.querySelector<HTMLInputElement>(
      "#snapshot-reference-label",
    );
    expect(input).not.toBeNull();
    expect(input!.value).toContain("测试环境");
    input!.value = "手动参考";
    input!.dispatchEvent(new Event("input", { bubbles: true }));
    await flushPromises();
    clickButton(saveDialog, "保存");
    await flushPromises();
    expect(savedLabel).toBe("手动参考");
    expect(notify).toHaveBeenCalled();
    expect(settingsText()).toContain("手动参考");
    expect(wrapper.find('[data-testid="delete-reference"]').exists()).toBe(
      true,
    );

    await wrapper.get('[data-testid="delete-reference"]').trigger("click");
    await flushPromises();
    const deleteDialog = dialog("删除参考快照");
    expect(deleteDialog.textContent).toContain("删除参考快照");
    clickButton(deleteDialog, "确定删除");
    await flushPromises();
    expect(settingsText()).not.toContain("手动参考");
  });

  it("清除启动历史时保留参考快照", async () => {
    wrapper = render();
    await flushPromises();
    await wrapper.get('[data-testid="clear-history"]').trigger("click");
    await flushPromises();
    const clearDialog = dialog("参考快照会保留");
    expect(clearDialog.textContent).toContain("参考快照会保留");
    clickButton(clearDialog, "确定清除");
    await flushPromises();
    expect(settingsText()).toContain("更新驱动前");
    expect(settingsText()).toContain("还没有成功启动记录");
  });
});
