import { describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick, type Ref } from "vue";
import { gamepadFamily } from "../../../launcher/ui/src/core/gamepad";
vi.mock("animejs", () => ({
  createTimeline: () => {
    const timers: ReturnType<typeof setTimeout>[] = [];
    return {
      call(callback: () => void, delay: number) {
        timers.push(setTimeout(callback, delay));
      },
      cancel() {
        timers.forEach(clearTimeout);
      },
    };
  },
}));
import GamepadHints from "../../../launcher/ui/src/components/GamepadHints.vue";
vi.mock("../../../launcher/ui/src/core/gamepad", async () => {
  const { computed, ref } = await import("vue");
  const family = ref("dualsense");
  return {
    gamepadFamily: family,
    gamepadGlyph: (button: string) => `${family.value}-${button}`,
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
  it("首次出现先挂载为隐藏的模糊状态, 再渐入图标和文字", async () => {
    const wrapper = mount(GamepadHints, {
      props: {
        items: [{ id: "confirm", buttons: ["confirm"], label: "操作" }],
        reduced: false,
      },
      attachTo: document.body,
    });
    await nextTick();
    const item = wrapper.get<HTMLElement>('[data-hint="confirm"]');
    expect(item.element.style.opacity).toBe("0");
    expect(
      wrapper.findComponent({ name: "motion.span" }).props("initial").filter,
    ).toBe("blur(6px)");
    expect(item.find(".gamepad-glyph").exists()).toBe(true);
    await vi.advanceTimersByTimeAsync(400);
    const animation = wrapper
      .findComponent({ name: "motion.span" })
      .props("animate");
    expect(animation.opacity).toBe(1);
    expect(animation.filter).toBe("blur(0px)");
    wrapper.unmount();
  });
  it("图标与文字在退场后一起交接, 不更新未变化的返回提示", async () => {
    (gamepadFamily as Ref<string>).value = "dualsense";
    const initial = [
      { id: "confirm", buttons: ["confirm"] as const, label: "操作" },
      { id: "back", buttons: ["back"] as const, label: "返回" },
      { id: "primary", buttons: ["r3"] as const, label: "快速检查" },
    ];
    const wrapper = mount(GamepadHints, {
      props: {
        items: initial.map((item) => ({ ...item, buttons: [...item.buttons] })),
        reduced: false,
      },
      attachTo: document.body,
    });
    await vi.advanceTimersByTimeAsync(400);
    const back = wrapper.get('[data-hint="back"]').element;
    const changes: MutationRecord[] = [];
    const observer = new MutationObserver((records) =>
      changes.push(...records),
    );
    observer.observe(back, {
      attributes: true,
      childList: true,
      subtree: true,
    });
    await wrapper.setProps({
      items: [
        { id: "confirm", buttons: ["confirm"], label: "进入" },
        { id: "back", buttons: ["back"], label: "返回" },
        { id: "primary", buttons: ["r3"], label: "重新检测" },
      ],
    });
    await vi.advanceTimersByTimeAsync(60);
    expect(wrapper.get('[data-hint="confirm"]').text()).toContain("操作");
    // 页面主操作稍晚更新, 不应重置其他提示的交接时刻。
    await wrapper.setProps({
      items: [
        { id: "confirm", buttons: ["confirm"], label: "进入" },
        { id: "back", buttons: ["back"], label: "返回" },
        { id: "primary", buttons: ["r3"], label: "刷新" },
      ],
    });
    await vi.advanceTimersByTimeAsync(100);
    expect(wrapper.get('[data-hint="confirm"]').text()).toContain("进入");
    await vi.advanceTimersByTimeAsync(300);
    expect(changes).toHaveLength(0);
    expect(wrapper.get('[data-hint="back"]').element).toBe(back);
    observer.disconnect();
    const previous = wrapper
      .get('[data-hint="confirm"] use')
      .attributes("href");
    (gamepadFamily as Ref<string>).value = "switch";
    await nextTick();
    await vi.advanceTimersByTimeAsync(80);
    expect(wrapper.get('[data-hint="confirm"] use').attributes("href")).toBe(
      previous,
    );
    await vi.advanceTimersByTimeAsync(100);
    expect(
      wrapper.get('[data-hint="confirm"] use').attributes("href"),
    ).toContain("switch-confirm");
    wrapper.unmount();
    (gamepadFamily as Ref<string>).value = "dualsense";
  });
});
