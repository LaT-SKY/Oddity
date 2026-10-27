import { beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { reactive, ref } from "vue";
import { bootstrap } from "./fixtures";

const controller = vi.hoisted(() => ({
  state: {} as { bootstrap: ReturnType<typeof bootstrap> },
  form: { theme: "light", reduceMotion: true },
  runtimeEnvironmentBusy: {} as ReturnType<typeof ref<boolean>>,
  runtimeEnvironmentLocked: {} as ReturnType<typeof ref<boolean>>,
  runtimeEnvironmentError: {} as ReturnType<typeof ref<string>>,
  createRuntimeEnvironment: vi.fn(),
  runtimeEnvironmentAction: vi.fn(),
}));
vi.mock("../../src/core/controller", () => controller);
const a = "a".repeat(32),
  b = "b".repeat(32);
beforeEach(() => {
  controller.state = reactive({ bootstrap: bootstrap() });
  controller.state.bootstrap.settings.runtimeEnvironments = [
    {
      id: a,
      name: "稳定 A",
      proton: { path: "/proton-a", identity: "pin" },
      distribution: "cn",
      installRoot: "/game",
      createdAt: 1,
    },
    {
      id: b,
      name: "实验 B",
      proton: { path: "/proton-b", identity: "pin-b" },
      distribution: "cn",
      installRoot: "/game",
      createdAt: 2,
    },
  ];
  controller.state.bootstrap.settings.activeEnvironment = b;
  controller.state.bootstrap.settings.stableEnvironment = a;
  controller.runtimeEnvironmentBusy = ref(false);
  controller.runtimeEnvironmentLocked = ref(false);
  controller.runtimeEnvironmentError = ref("");
  controller.createRuntimeEnvironment.mockReset().mockResolvedValue(true);
  controller.runtimeEnvironmentAction.mockReset().mockResolvedValue(true);
});
async function render() {
  const { default: Component } =
    await import("../../src/components/RuntimeEnvironments.vue");
  return mount(Component, {
    attachTo: document.body,
    global: { stubs: { transition: false, teleport: true } },
  });
}

it("首次使用时创建全新运行环境", async () => {
  controller.state.bootstrap.settings.runtimeEnvironments = [];
  controller.state.bootstrap.settings.activeEnvironment = null;
  const wrapper = await render();
  expect(wrapper.text()).toContain("请先创建 Proton 运行环境");
  expect(wrapper.find("#runtime-environment").exists()).toBe(false);
  await wrapper.get("button").trigger("click");
  await wrapper.get("#runtime-environment-name").setValue("首个环境");
  await wrapper.get("#runtime-environment-proton").setValue("/proton-a");
  await wrapper
    .findAll("button")
    .find((button) => button.text() === "创建环境")!
    .trigger("click");
  expect(controller.createRuntimeEnvironment).toHaveBeenCalledWith(
    "首个环境",
    "/proton-a",
    null,
  );
  await flushPromises();
  expect(wrapper.find("#runtime-environment-name").exists()).toBe(false);
  wrapper.unmount();
});

it("使用 B 时选中稳定 A, 副本请求使用 A 且不切换当前环境", async () => {
  const wrapper = await render();
  wrapper
    .findComponent({ name: "BaseSelect" })
    .vm.$emit("update:modelValue", a);
  await flushPromises();
  const button = (name: string) =>
    wrapper.findAll("button").find((button) => button.text() === name)!;
  await button("创建环境或副本").trigger("click");
  expect(wrapper.text()).toContain("源环境: 稳定 A");
  expect(
    (wrapper.get("#runtime-environment-proton").element as HTMLInputElement)
      .value,
  ).toBe("/proton-a");
  await wrapper.get("#runtime-environment-name").setValue("A 副本");
  controller.createRuntimeEnvironment.mockResolvedValueOnce(false);
  await button("创建副本").trigger("click");
  expect(controller.createRuntimeEnvironment).toHaveBeenCalledWith(
    "A 副本",
    "/proton-a",
    a,
  );
  expect(controller.state.bootstrap.settings.activeEnvironment).toBe(b);
  expect(wrapper.find("#runtime-environment-name").exists()).toBe(true);
  wrapper
    .findAllComponents({ name: "BaseSelect" })[1]
    .vm.$emit("update:modelValue", "");
  await flushPromises();
  expect(wrapper.text()).toContain("创建独立 prefix");
  expect(
    (wrapper.get("#runtime-environment-proton").element as HTMLInputElement)
      .value,
  ).toBe("");
  await button("取消创建").trigger("click");
  wrapper.unmount();
});

it("切换和保留指定环境, 当前和稳定环境不可删除", async () => {
  const wrapper = await render();
  const button = (name: string) =>
    wrapper.findAll("button").find((button) => button.text() === name)!;
  expect(button("删除环境").attributes("disabled")).toBeDefined();
  wrapper
    .findComponent({ name: "BaseSelect" })
    .vm.$emit("update:modelValue", a);
  await flushPromises();
  await button("切换到此环境").trigger("click");
  expect(controller.runtimeEnvironmentAction).toHaveBeenCalledWith("switch", a);
  expect(button("删除环境").attributes("disabled")).toBeDefined();
  wrapper
    .findComponent({ name: "BaseSelect" })
    .vm.$emit("update:modelValue", b);
  await flushPromises();
  await button("保留为稳定环境").trigger("click");
  expect(controller.runtimeEnvironmentAction).toHaveBeenCalledWith("keep", b);
  wrapper.unmount();
});

it("删除必须在弹窗中确认, 失败保留弹窗并允许取消和重试", async () => {
  controller.state.bootstrap.settings.activeEnvironment = a;
  const wrapper = await render();
  wrapper
    .findComponent({ name: "BaseSelect" })
    .vm.$emit("update:modelValue", b);
  await flushPromises();
  const remove = wrapper
    .findAll("button")
    .find((button) => button.text() === "删除环境")!;
  await remove.trigger("click");
  await flushPromises();
  const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]')!;
  const click = async (name: string) => {
    Array.from(dialog().querySelectorAll("button"))
      .find((button) => button.textContent?.trim() === name)!
      .click();
    await flushPromises();
  };
  expect(dialog().textContent).toContain("确定删除“实验 B”的 prefix?");
  expect(controller.runtimeEnvironmentAction).not.toHaveBeenCalled();
  await click("取消");
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  await remove.trigger("click");
  await flushPromises();
  controller.runtimeEnvironmentAction.mockResolvedValueOnce(false);
  controller.runtimeEnvironmentError.value = "磁盘写入失败";
  await click("确定删除");
  expect(dialog().textContent).toContain("磁盘写入失败");
  await click("确定删除");
  expect(controller.runtimeEnvironmentAction).toHaveBeenCalledWith(
    "discard",
    b,
  );
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  wrapper.unmount();
});

it("处理期间禁用操作并展示后端错误", async () => {
  controller.runtimeEnvironmentBusy.value = true;
  controller.runtimeEnvironmentLocked.value = true;
  controller.runtimeEnvironmentError.value = "固定的 Proton 已变化";
  const wrapper = await render();
  expect(wrapper.get('[role="status"]').text()).toContain("正在处理运行环境");
  expect(wrapper.get('[role="alert"]').text()).toContain("Proton 已变化");
  expect(
    wrapper
      .findAll("button")
      .every((button) => button.attributes("disabled") !== undefined),
  ).toBe(true);
  wrapper.unmount();
});
