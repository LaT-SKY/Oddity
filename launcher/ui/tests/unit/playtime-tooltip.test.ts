import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { nextTick, reactive } from "vue";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import FloatingTooltip from "../../src/components/playtime/FloatingTooltip.vue";

let wrapper: VueWrapper;
let pointer: { x: number; y: number };
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
  pointer = reactive({ x, y });
  wrapper = mount(FloatingTooltip, {
    props: {
      open: false,
      pointer,
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

function offset(element: HTMLElement) {
  const match = /translate3d\(([-\d.]+)px, ([-\d.]+)px, 0\)/.exec(
    element.style.transform,
  );
  if (!match) return null;
  return {
    left: `${Math.round(Number(match[1]))}px`,
    top: `${Math.round(Number(match[2]))}px`,
  };
}

it.each([
  [5, 10, "12px", "26px"],
  [395, 250, "288px", "200px"],
  [200, 100, "150px", "50px"],
])("在鼠标 %i,%i 处定位提示并限制在视口内", async (x, y, left, top) => {
  await render(x, y);
  expect(offset(tip())).toEqual({ left, top });
  expect(tip().style.visibility).toBe("visible");
  expect(tip().dataset.theme).toBe("dark");
  expect(tip().textContent).toBe("游玩时长");
});

it("鼠标移动后重新定位, 关闭后移除提示", async () => {
  await render(200, 100);
  pointer.x = 395;
  pointer.y = 250;
  await vi.advanceTimersByTimeAsync(20);
  expect(offset(tip())).toEqual({ left: "288px", top: "200px" });
  await wrapper.setProps({ open: false });
  await vi.advanceTimersByTimeAsync(20);
  expect(tip()).toBeNull();
});

it("移动时复用已测量的尺寸, 不重复触发布局", async () => {
  await render(200, 100);
  const bounds = vi.mocked(HTMLElement.prototype.getBoundingClientRect);
  const measured = bounds.mock.calls.length;
  pointer.x = 210;
  pointer.y = 120;
  await vi.advanceTimersByTimeAsync(20);
  expect(offset(tip())).toEqual({ left: "160px", top: "70px" });
  expect(bounds).toHaveBeenCalledTimes(measured);
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
  pointer.x = 201;
  await vi.advanceTimersByTimeAsync(20);
  expect(tip().style.visibility).toBe("visible");
});

it("重新打开后观察新的提示元素", async () => {
  const observed: Element[] = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(_callback: () => void) {}
      observe(element: Element) {
        observed.push(element);
      }
      disconnect() {}
    },
  );
  await render(200, 100);
  const first = tip();
  await wrapper.setProps({ open: false });
  await vi.advanceTimersByTimeAsync(200);
  await wrapper.setProps({ open: true });
  await vi.advanceTimersByTimeAsync(20);
  expect(observed).toHaveLength(2);
  expect(observed[1]).toBe(tip());
  expect(observed[1]).not.toBe(first);
});

it("卸载时取消待执行的定位帧", async () => {
  await render(200, 100);
  const cancel = vi.spyOn(window, "cancelAnimationFrame");
  pointer.x = 210;
  await nextTick();
  wrapper.unmount();
  expect(cancel).toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(20);
  expect(tip()).toBeNull();
});
