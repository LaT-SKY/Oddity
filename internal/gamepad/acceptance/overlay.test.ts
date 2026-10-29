import { afterEach, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import GamepadOverlay from "../../../launcher/ui/src/components/GamepadOverlay.vue";
import { gamepad, gamepadTarget } from "../../../launcher/ui/src/core/gamepad";
vi.mock("../../../launcher/ui/src/core/controller", () => ({
  form: { reduceMotion: true, theme: "light" },
  state: { page: "records" },
}));
vi.mock("../../../launcher/ui/src/composables/useSceneMotion", async () => {
  const { ref } = await import("vue");
  return { playerExpanded: ref(false) };
});
vi.mock("../../../launcher/ui/src/core/gamepad", async () => {
  const { reactive, shallowRef, computed } = await import("vue");
  return {
    gamepad: reactive({ active: true, pressing: false, navigation: false }),
    gamepadTarget: shallowRef<HTMLElement | null>(null),
    gamepadViewport: () => ({ top: 0, bottom: 760 }),
    gamepadFamily: computed(() => "dualsense"),
    gamepadGlyph: (button: string) => `ps-${button}`,
    confirmButton: computed(() => 0),
    backButton: computed(() => 1),
  };
});
afterEach(() => {
  delete (Element.prototype as unknown as Record<string, unknown>)
    .getAnimations;
});
it("静止时停止测量, 滚动、尺寸和过渡变化重新跟随, 浏览图表时收起确认图标", async () => {
  let resized!: ResizeObserverCallback;
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: ResizeObserverCallback) {
        resized = callback;
      }
      observe() {}
      disconnect() {}
    },
  );
  let moving = false;
  Object.defineProperty(Element.prototype, "getAnimations", {
    configurable: true,
    value: () => (moving ? [{ playState: "running" }] : []),
  });
  document.body.innerHTML =
    '<section class="page is-active"><div data-pad-region></div></section>';
  const target = document.querySelector<HTMLElement>("[data-pad-region]")!;
  const bounds = vi
    .spyOn(target, "getBoundingClientRect")
    .mockReturnValue(new DOMRect(40, 200, 500, 100));
  gamepadTarget.value = target;
  gamepad.active = true;
  const wrapper = mount(GamepadOverlay, { attachTo: document.body });
  await vi.advanceTimersByTimeAsync(80);
  const measured = bounds.mock.calls.length;
  await vi.advanceTimersByTimeAsync(1000);
  expect(bounds).toHaveBeenCalledTimes(measured);
  target.parentElement!.style.transform = "translateY(20px)";
  await nextTick();
  await vi.advanceTimersByTimeAsync(40);
  expect(bounds.mock.calls.length).toBeGreaterThan(measured);
  const previous = bounds.mock.calls.length;
  moving = true;
  resized([], {} as ResizeObserver);
  await vi.advanceTimersByTimeAsync(80);
  expect(bounds.mock.calls.length - previous).toBeGreaterThan(2);
  moving = false;
  await vi.advanceTimersByTimeAsync(40);
  const settled = bounds.mock.calls.length;
  await vi.advanceTimersByTimeAsync(1000);
  expect(bounds).toHaveBeenCalledTimes(settled);
  document.dispatchEvent(new Event("scroll"));
  await vi.advanceTimersByTimeAsync(40);
  expect(bounds).toHaveBeenCalledTimes(settled + 1);
  target.setAttribute("data-pad-browsing", "true");
  await nextTick();
  await vi.advanceTimersByTimeAsync(40);
  expect(document.querySelector(".gamepad-select__key")).toBeNull();
  wrapper.unmount();
  const unmounted = bounds.mock.calls.length;
  document.dispatchEvent(new Event("scroll"));
  await vi.advanceTimersByTimeAsync(100);
  expect(bounds).toHaveBeenCalledTimes(unmounted);
});
