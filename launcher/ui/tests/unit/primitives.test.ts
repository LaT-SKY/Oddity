import { describe, expect, it } from "vitest";
import { h } from "vue";
import { mount } from "@vue/test-utils";
import BaseButton from "../../src/components/BaseButton.vue";
import BaseInput from "../../src/components/BaseInput.vue";
import BaseSwitch from "../../src/components/BaseSwitch.vue";
import BaseTabs from "../../src/components/BaseTabs.vue";
import SettingRow from "../../src/components/settings/SettingRow.vue";

describe("基础控件", () => {
  it("按钮默认不提交表单, 等待时阻止再次操作并保留名称", async () => {
    const wrapper = mount(BaseButton, {
      props: { icon: "i-download-simple" },
      slots: { default: "导出日志" },
    });
    expect(wrapper.get("button").attributes("type")).toBe("button");
    await wrapper.setProps({ busy: true });
    expect(wrapper.get("button").attributes("disabled")).toBeDefined();
    expect(wrapper.get("button").attributes("aria-busy")).toBe("true");
    expect(wrapper.text()).toBe("导出日志");
    wrapper.unmount();
  });
  it("输入框保留字符串与原生约束, 中文组词完成后再更新模型", async () => {
    const wrapper = mount(BaseInput, {
      props: { modelValue: "", invalid: true },
      attrs: { id: "name", required: true, maxlength: "64" },
    });
    const input = wrapper.get("input");
    expect(input.attributes("required")).toBeDefined();
    expect(input.attributes("aria-invalid")).toBe("true");
    await input.trigger("compositionstart");
    input.element.value = "huan";
    await input.trigger("input");
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    input.element.value = "环";
    await input.trigger("compositionend");
    expect(wrapper.emitted("update:modelValue")).toEqual([["环"]]);
    wrapper.unmount();
  });
  it("开关使用原生复选框, 关联条目说明并受禁用状态控制", async () => {
    const wrapper = mount(SettingRow, {
      props: { label: "自动签到", description: "每天执行", for: "enabled" },
      slots: {
        default: ({ describedby }: { describedby: string | undefined }) => {
          return h(BaseSwitch, {
            id: "enabled",
            modelValue: false,
            describedby,
          });
        },
      },
    });
    expect(wrapper.get("label").attributes("for")).toBe("enabled");
    const input = wrapper.get("input");
    expect(input.attributes("aria-describedby")).toBe(
      wrapper.get("p").attributes("id"),
    );
    await input.setValue(true);
    expect(
      wrapper.findComponent(BaseSwitch).emitted("update:modelValue"),
    ).toEqual([[true]]);
    wrapper.unmount();
  });
  it("分段控件方向键跳过禁用项并发出明确的选择", async () => {
    const wrapper = mount(BaseTabs, {
      props: {
        modelValue: "a",
        label: "方式",
        options: [
          { value: "a", label: "A" },
          { value: "b", label: "B", disabled: true },
          { value: "c", label: "C" },
        ],
      },
      attachTo: document.body,
    });
    await wrapper
      .get('[role="tablist"]')
      .trigger("keydown", { key: "ArrowRight" });
    expect(wrapper.emitted("change")).toEqual([["c"]]);
    expect(document.activeElement?.textContent).toBe("C");
    wrapper.unmount();
  });
});
