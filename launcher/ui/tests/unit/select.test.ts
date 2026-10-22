import { describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import BaseSelect from "../../src/components/BaseSelect.vue";

describe("BaseSelect", () => {
  const options = [
    { label: "选项A", value: "a" },
    { label: "选项B", value: "b" },
    { label: "选项C", value: "c" },
  ];

  it("渲染当前选中项并在点击时展开/收起下拉选项", async () => {
    const update = vi.fn();
    const change = vi.fn();
    const wrapper = mount(BaseSelect, {
      props: {
        "onUpdate:modelValue": update,
        onChange: change,
        modelValue: "a",
        options,
        id: "test-select",
      },
      attachTo: document.body,
    });

    const trigger = wrapper.find(".select-trigger");
    expect(trigger.text()).toBe("选项A");
    expect(wrapper.find(".select-dropdown").exists()).toBe(false);

    // 点击展开
    await trigger.trigger("click");
    expect(wrapper.find(".select-dropdown").exists()).toBe(true);
    expect(wrapper.findAll(".select-option")).toHaveLength(3);

    // 点击选中第二项
    await wrapper.findAll(".select-option")[1].trigger("click");
    expect(update).toHaveBeenCalledWith("b");
    expect(change).toHaveBeenCalledWith("b");
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.find(".select-dropdown").exists()).toBe(false);

    wrapper.unmount();
  });

  it("支持键盘上下键切换选项及 Escape 关闭", async () => {
    const update = vi.fn();
    const wrapper = mount(BaseSelect, {
      props: {
        "onUpdate:modelValue": update,
        modelValue: "a",
        options,
      },
      attachTo: document.body,
    });

    const trigger = wrapper.find(".select-trigger");
    await trigger.trigger("keydown", { key: "ArrowDown" });
    expect(wrapper.find(".select-dropdown").exists()).toBe(true);

    await trigger.trigger("keydown", { key: "ArrowDown" });
    expect(update).toHaveBeenCalledWith("b");

    await trigger.trigger("keydown", { key: "Escape" });
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.find(".select-dropdown").exists()).toBe(false);

    wrapper.unmount();
  });

  it("点击外部区域自动收起下拉菜单", async () => {
    const wrapper = mount(BaseSelect, {
      props: {
        modelValue: "a",
        options,
      },
      attachTo: document.body,
    });

    await wrapper.find(".select-trigger").trigger("click");
    expect(wrapper.find(".select-dropdown").exists()).toBe(true);

    document.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    await wrapper.vm.$nextTick();
    await vi.advanceTimersByTimeAsync(1000);
    expect(wrapper.find(".select-dropdown").exists()).toBe(false);

    wrapper.unmount();
  });

  it("同步原生 select 的变更事件", async () => {
    const update = vi.fn();
    const change = vi.fn();
    const wrapper = mount(BaseSelect, {
      props: {
        "onUpdate:modelValue": update,
        onChange: change,
        modelValue: "a",
        options,
        id: "native-test",
      },
      attachTo: document.body,
    });

    const native = wrapper.find("select");
    await native.setValue("c");
    expect(update).toHaveBeenCalledWith("c");
    expect(change).toHaveBeenCalledWith("c");

    wrapper.unmount();
  });
});
