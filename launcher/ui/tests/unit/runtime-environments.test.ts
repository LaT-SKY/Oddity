import { beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { reactive, ref } from "vue";
import { bootstrap } from "./fixtures";

const controller = vi.hoisted(() => ({
  state: {} as { bootstrap: ReturnType<typeof bootstrap> },
  form: { protonPath: "/proton-original" },
  runtimeEnvironmentBusy: {} as ReturnType<typeof ref<boolean>>,
  runtimeEnvironmentLocked: {} as ReturnType<typeof ref<boolean>>,
  runtimeEnvironmentError: {} as ReturnType<typeof ref<string>>,
  createRuntimeEnvironment: vi.fn(),
  runtimeEnvironmentAction: vi.fn(),
}));
vi.mock("../../src/core/controller", () => controller);

beforeEach(() => {
  controller.state = reactive({ bootstrap: bootstrap() });
  controller.state.bootstrap.settings.runtimeEnvironments = [
    {
      id: "a".repeat(32),
      name: "新版本测试",
      proton: { path: "/proton-new", identity: "pin" },
      distribution: "cn",
      installRoot: "/game",
      createdAt: 1,
    },
  ];
  controller.runtimeEnvironmentBusy = ref(false);
  controller.runtimeEnvironmentLocked = ref(false);
  controller.runtimeEnvironmentError = ref("");
  controller.createRuntimeEnvironment.mockReset().mockResolvedValue(true);
  controller.runtimeEnvironmentAction.mockReset().mockResolvedValue(true);
});

async function render() {
  const { default: Component } =
    await import("../../src/components/RuntimeEnvironments.vue");
  return mount(Component);
}

it("展示共享边界, 并从所选环境创建指定版本的副本", async () => {
  const wrapper = await render();
  expect(wrapper.text()).toContain("共享游戏安装");
  const button = (name: string) =>
    wrapper.findAll("button").find((button) => button.text() === name)!;
  await button("从所选环境创建副本").trigger("click");
  await wrapper.get("#runtime-environment-name").setValue("新版验证");
  await wrapper.get("#runtime-environment-proton").setValue("/proton-b");
  await button("创建实验环境").trigger("click");
  await flushPromises();
  expect(controller.createRuntimeEnvironment).toHaveBeenCalledWith(
    "新版验证",
    "/proton-b",
    null,
  );
  expect(wrapper.find("#runtime-environment-name").exists()).toBe(false);
});

it("使用 B 时选中稳定 A, 创建表单与请求均使用 A 且不切换当前环境", async () => {
  const a = "a".repeat(32);
  const b = "b".repeat(32);
  controller.state.bootstrap.settings.runtimeEnvironments.push({
    id: b,
    name: "B",
    proton: { path: "/proton-b", identity: "pin-b" },
    distribution: "cn",
    installRoot: "/game",
    createdAt: 2,
  });
  controller.state.bootstrap.settings.activeEnvironment = b;
  controller.state.bootstrap.settings.stableEnvironment = a;
  controller.state.bootstrap.protonPath = "/proton-b";
  const wrapper = await render();
  wrapper
    .findComponent({ name: "BaseSelect" })
    .vm.$emit("update:modelValue", a);
  await flushPromises();
  const button = (name: string) =>
    wrapper.findAll("button").find((button) => button.text() === name)!;
  await button("从所选环境创建副本").trigger("click");
  expect(wrapper.text()).toContain("源环境: 新版本测试");
  expect(
    (wrapper.get("#runtime-environment-proton").element as HTMLInputElement)
      .value,
  ).toBe("/proton-new");
  await wrapper.get("#runtime-environment-name").setValue("A 的副本");
  await button("创建实验环境").trigger("click");
  expect(controller.createRuntimeEnvironment).toHaveBeenCalledWith(
    "A 的副本",
    "/proton-new",
    a,
  );
  expect(controller.state.bootstrap.settings.activeEnvironment).toBe(b);
});

it("切换和保留指定环境, 当前和稳定环境不可删除", async () => {
  const wrapper = await render();
  const id = "a".repeat(32);
  const select = wrapper.findComponent({ name: "BaseSelect" });
  select.vm.$emit("update:modelValue", id);
  await flushPromises();
  const button = (name: string) =>
    wrapper.findAll("button").find((button) => button.text() === name)!;
  await button("切换到此环境").trigger("click");
  expect(controller.runtimeEnvironmentAction).toHaveBeenCalledWith(
    "switch",
    id,
  );
  await button("保留为稳定环境").trigger("click");
  expect(controller.runtimeEnvironmentAction).toHaveBeenCalledWith("keep", id);
  controller.state.bootstrap.settings.stableEnvironment = id;
  await flushPromises();
  expect(button("删除实验环境").attributes("disabled")).toBeDefined();
  controller.state.bootstrap.settings.stableEnvironment = null;
  controller.state.bootstrap.settings.activeEnvironment = id;
  await flushPromises();
  expect(button("删除实验环境").attributes("disabled")).toBeDefined();
});

it("删除之前确认名称, 可以取消并重试", async () => {
  const wrapper = await render();
  wrapper
    .findComponent({ name: "BaseSelect" })
    .vm.$emit("update:modelValue", "a".repeat(32));
  await flushPromises();
  const button = (name: string) =>
    wrapper.findAll("button").find((button) => button.text() === name)!;
  await button("删除实验环境").trigger("click");
  expect(wrapper.text()).toContain("确定删除“新版本测试”的 prefix?");
  expect(controller.runtimeEnvironmentAction).not.toHaveBeenCalled();
  await button("取消").trigger("click");
  expect(wrapper.text()).not.toContain("确定删除“");
  await button("删除实验环境").trigger("click");
  await button("确定删除").trigger("click");
  expect(controller.runtimeEnvironmentAction).toHaveBeenCalledWith(
    "discard",
    "a".repeat(32),
  );
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
});
