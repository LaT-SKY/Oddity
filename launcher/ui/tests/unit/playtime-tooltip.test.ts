import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import FloatingTooltip from "../../src/components/playtime/FloatingTooltip.vue";

let wrapper: VueWrapper;
beforeEach(() => {
  vi.stubGlobal("innerWidth", 400);
  vi.stubGlobal("innerHeight", 300);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 100,
    height: 40,
  } as DOMRect);
});
afterEach(() => wrapper?.unmount());

async function render(x: number, y: number) {
  wrapper = mount(FloatingTooltip, {
    props: {
      open: false,
      x,
      y,
      reduceMotion: true,
      theme: "dark",
      name: "test",
    },
    slots: { default: "游玩时长" },
  });
  await wrapper.setProps({ open: true });
  await vi.advanceTimersByTimeAsync(20);
  await flushPromises();
}
const tip = () => document.querySelector<HTMLElement>('[role="tooltip"]')!;

it.each([
  [5, 10, "12px", "26px"],
  [395, 250, "288px", "200px"],
  [200, 100, "150px", "50px"],
])("在鼠标 %i,%i 处定位提示并限制在视口内", async (x, y, left, top) => {
  await render(x, y);
  expect(tip().style.left).toBe(left);
  expect(tip().style.top).toBe(top);
  expect(tip().style.visibility).toBe("visible");
  expect(tip().dataset.theme).toBe("dark");
  expect(tip().textContent).toBe("游玩时长");
});

it("鼠标移动后重新定位, 关闭后移除提示", async () => {
  await render(200, 100);
  await wrapper.setProps({ x: 395, y: 250 });
  await vi.advanceTimersByTimeAsync(20);
  expect(tip().style.left).toBe("288px");
  expect(tip().style.top).toBe("200px");
  await wrapper.setProps({ open: false });
  await vi.advanceTimersByTimeAsync(20);
  expect(tip()).toBeNull();
});

it("尚无可测量宽度时保持隐藏且有限次重试", async () => {
  const bounds = vi.mocked(HTMLElement.prototype.getBoundingClientRect);
  bounds.mockReturnValue({ width: 0, height: 0 } as DOMRect);
  await render(200, 100);
  expect(tip().style.visibility).toBe("hidden");
  await vi.advanceTimersByTimeAsync(200);
  const calls = bounds.mock.calls.length;
  await vi.advanceTimersByTimeAsync(200);
  expect(bounds).toHaveBeenCalledTimes(calls);
  bounds.mockReturnValue({ width: 100, height: 40 } as DOMRect);
  await wrapper.setProps({ x: 201 });
  await vi.advanceTimersByTimeAsync(20);
  expect(tip().style.visibility).toBe("visible");
});

it("卸载时取消待执行的定位帧", async () => {
  await render(200, 100);
  const cancel = vi.spyOn(window, "cancelAnimationFrame");
  await wrapper.setProps({ x: 210 });
  wrapper.unmount();
  expect(cancel).toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(20);
  expect(tip()).toBeNull();
});
