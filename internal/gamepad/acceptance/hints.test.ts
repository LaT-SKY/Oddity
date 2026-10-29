import { describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import GamepadHints from "../../../launcher/ui/src/components/GamepadHints.vue";
vi.mock("../../../launcher/ui/src/core/gamepad", async () => {
  const { computed } = await import("vue");
  return {
    gamepadFamily: computed(() => "dualsense"),
    confirmButton: computed(() => 0),
    backButton: computed(() => 1),
  };
});
describe("手柄提示更新", () => {
  it("更新文字时复用提示节点, 移动项目时保留未变化的位置", async () => {
    const items = [
      { id: "confirm", buttons: ["confirm"] as const, label: "操作" },
      { id: "back", buttons: ["back"] as const, label: "返回" },
      { id: "primary", buttons: ["r3"] as const, label: "刷新" },
    ];
    const wrapper = mount(GamepadHints, {
      props: {
        items: items.map((item) => ({ ...item, buttons: [...item.buttons] })),
        reduced: true,
      },
    });
    const original = wrapper.get('[data-hint="confirm"]').element;
    const back = wrapper.get('[data-hint="back"]').element;
    const primary = wrapper.get('[data-hint="primary"]').element;
    await wrapper.setProps({
      items: [
        { ...items[0], buttons: ["confirm"], label: "进入" },
        { ...items[1], buttons: ["back"] },
        { ...items[2], buttons: ["r3"] },
      ],
    });
    expect(wrapper.get('[data-hint="confirm"]').element).toBe(original);
    expect(wrapper.get('[data-hint="confirm"]').text()).toContain("进入");
    expect(wrapper.get('[data-hint="back"]').element).toBe(back);
    expect(wrapper.get('[data-hint="primary"]').element).toBe(primary);
    const confirm = wrapper.get('[data-hint="confirm"]').element;
    await wrapper.setProps({
      items: [
        { ...items[2], buttons: ["r3"] },
        { ...items[1], buttons: ["back"] },
        { ...items[0], buttons: ["confirm"], label: "进入" },
      ],
    });
    expect(wrapper.get('[data-hint="back"]').element).toBe(back);
    expect(wrapper.get('[data-hint="confirm"]').element).not.toBe(confirm);
    expect(wrapper.get('[data-hint="primary"]').element).not.toBe(primary);
    wrapper.unmount();
  });
});
