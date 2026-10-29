import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { reactive, ref, nextTick } from "vue";
import { mount } from "@vue/test-utils";
vi.mock("../../../launcher/ui/src/core/controller", async () => {
  const { reactive } = await import("vue");
  const { defaultGamepadOptions } =
    await import("../../../launcher/ui/src/types/gamepad");
  const state = reactive({ page: "settings" });
  return {
    state,
    form: reactive({ gamepad: defaultGamepadOptions(), reduceMotion: true }),
    showPage: vi.fn((page: string) => {
      state.page = page;
    }),
    handleLaunchAction: vi.fn(),
    openSettings: vi.fn(),
  };
});
const bridge = vi.hoisted(() => ({
  native: false,
  invoke: vi.fn().mockResolvedValue(null),
}));
vi.mock("../../../launcher/ui/src/core/api", () => ({
  invoke: bridge.invoke,
  isTauri: () => bridge.native,
}));
import {
  createGamepadNavigation,
  startGamepad,
  gamepadFeedback,
  gamepad,
  gamepadTarget,
  confirmButton,
  backButton,
} from "../../../launcher/ui/src/core/gamepad";
import {
  state,
  showPage,
  handleLaunchAction,
} from "../../../launcher/ui/src/core/controller";
import { playerExpanded } from "../../../launcher/ui/src/composables/useSceneMotion";
import { settingsNavigation } from "../../../launcher/ui/src/core/settingsCatalog";
import BaseSelect from "../../../launcher/ui/src/components/BaseSelect.vue";
import GamepadKeyboard from "../../../launcher/ui/src/components/GamepadKeyboard.vue";
let navigation: ReturnType<typeof createGamepadNavigation>;
let rects: Map<Element, DOMRect>;
function region(html: string) {
  document.body.innerHTML = html;
  document
    .querySelectorAll<HTMLElement>("button,input,textarea,[data-pad-region]")
    .forEach((element, index) => {
      rects.set(element, new DOMRect(20, index * 60, 180, 40));
    });
}
beforeEach(() => {
  vi.clearAllMocks();
  bridge.native = false;
  bridge.invoke.mockResolvedValue(null);
  rects = new Map();
  vi.spyOn(Element.prototype, "getClientRects").mockImplementation(function (
    this: Element,
  ) {
    return rects.has(this)
      ? ([rects.get(this)!] as unknown as DOMRectList)
      : ([] as unknown as DOMRectList);
  });
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(
    function (this: Element) {
      return rects.get(this) ?? new DOMRect(0, 0, 180, 40);
    },
  );
  Element.prototype.scrollIntoView = vi.fn();
  gamepad.active = true;
  gamepad.notification = false;
  gamepad.navigation = false;
  gamepad.editing = null;
  gamepad.device = {
    id: 1,
    name: "DS",
    family: "dualsense",
    rumble: true,
    triggerRumble: false,
    adaptive: true,
    audio: true,
  };
  gamepadTarget.value = null;
  state.page = "settings";
  playerExpanded.value = false;
  settingsNavigation.category = "appearance";
  navigation = createGamepadNavigation();
});
afterEach(() => navigation.dispose());
describe("手柄导航", () => {
  it("按平台确认返回, 快捷键按位置", () => {
    expect(confirmButton.value).toBe(0);
    gamepad.device!.family = "switch";
    expect(confirmButton.value).toBe(1);
    expect(backButton.value).toBe(0);
  });
  it("方向移动跳过隐藏和禁用控件, 并记忆页面位置", async () => {
    region(
      '<section class="page is-active"><button id="a">A</button><button disabled>X</button><div inert><button>X</button></div><button id="b">B</button></section>',
    );
    navigation.refresh();
    expect(gamepadTarget.value?.id).toBe("a");
    await navigation.press(12);
    expect(gamepadTarget.value?.id).toBe("b");
    await navigation.press(11);
    expect(gamepadTarget.value?.id).toBe("a");
  });
  it("同一行左右切换, 上下保持列位置", async () => {
    region(
      '<section class="page is-active"><button id="a">A</button><button id="b">B</button><button id="c">C</button><button id="d">D</button></section>',
    );
    const list = [...document.querySelectorAll("button")];
    list.forEach((e, i) =>
      rects.set(
        e,
        new DOMRect(20 + (i % 2) * 220, Math.floor(i / 2) * 80, 180, 40),
      ),
    );
    navigation.refresh();
    await navigation.press(14);
    expect(gamepadTarget.value?.id).toBe("b");
    await navigation.press(12);
    expect(gamepadTarget.value?.id).toBe("d");
    await navigation.press(13);
    expect(gamepadTarget.value?.id).toBe("c");
  });
  it("启动页快捷启动不依赖当前选择, Y/B 关闭玩家信息", async () => {
    state.page = "home";
    region(
      '<section class="page is-active"><button id="other">Other</button><div class="game-records"><button>Player</button></div></section>',
    );
    navigation.refresh();
    await navigation.press(2);
    expect(handleLaunchAction).toHaveBeenCalledTimes(1);
    await navigation.press(3);
    expect(playerExpanded.value).toBe(true);
    await navigation.press(3);
    expect(playerExpanded.value).toBe(false);
    await navigation.press(3);
    await navigation.press(1);
    expect(playerExpanded.value).toBe(false);
  });
  it("肩键直接切页", async () => {
    region(
      '<nav class="rail__nav"><button data-page="home"></button><button data-page="settings"></button></nav><section class="page is-active"><button>A</button></section>',
    );
    const click = vi.fn();
    document
      .querySelector('[data-page="home"]')!
      .addEventListener("click", click);
    await navigation.press(10);
    expect(click).toHaveBeenCalledTimes(1);
  });
  it("进入分类栏, 确认后限于当前分类, 返回先回分类栏", async () => {
    region(
      '<div class="app"><header><button id="minimize">最小化</button></header><section class="page is-active"><nav class="settings-categories"><button data-category="appearance" aria-current="true" id="appearance">外观</button><button data-category="gamepad" id="gamepad">手柄</button></nav><div class="settings-panel is-current"><button id="first">First</button><button id="second">Second</button></div></section></div>',
    );
    navigation.refresh();
    expect(gamepadTarget.value?.id).toBe("appearance");
    expect(gamepad.navigation).toBe(true);
    await navigation.press(0);
    expect(gamepad.navigation).toBe(false);
    expect(gamepadTarget.value?.id).toBe("first");
    await navigation.press(11);
    expect(gamepadTarget.value?.id).toBe("first");
    await navigation.press(12);
    expect(gamepadTarget.value?.id).toBe("second");
    await navigation.press(1);
    expect(gamepad.navigation).toBe(true);
    expect(gamepadTarget.value?.id).toBe("appearance");
    expect(showPage).not.toHaveBeenCalled();
    await navigation.press(1);
    expect(showPage).toHaveBeenCalledWith("home");
  });
  it("在分类栏移动只改变 Select, 确认后进入对应分类", async () => {
    region(
      '<section class="page is-active"><nav class="settings-categories"><button data-category="appearance" aria-current="true" id="appearance">外观</button><button data-category="gamepad" id="gamepad">手柄</button></nav><div class="settings-panel is-current"><button id="field">Field</button></div></section>',
    );
    const choose = vi.fn();
    document.getElementById("gamepad")!.addEventListener("click", choose);
    navigation.refresh();
    await navigation.press(12);
    expect(gamepadTarget.value?.id).toBe("gamepad");
    expect(choose).not.toHaveBeenCalled();
    await navigation.press(0);
    expect(choose).toHaveBeenCalledTimes(1);
    expect(gamepadTarget.value?.id).toBe("field");
  });
  it("切换页面始终重选本页入口, 不落到最小化或刷新按钮", async () => {
    region(
      '<div class="app"><button id="minimize">Minimize</button><section class="page is-active"><header class="page-head"><button data-pad-primary id="refresh">刷新</button></header><button id="first">First</button><button id="second">Second</button></section></div>',
    );
    state.page = "update";
    navigation.refresh();
    expect(gamepadTarget.value?.id).toBe("first");
    await navigation.press(12);
    expect(gamepadTarget.value?.id).toBe("second");
    state.page = "records";
    await nextTick();
    navigation.refresh();
    expect(gamepadTarget.value?.id).toBe("first");
  });
  it("R3 在任意当前位置执行本页主操作, 不改变 Select, 弹窗内不穿透", async () => {
    region(
      '<section class="page is-active"><header class="page-head"><button data-pad-primary id="refresh">刷新</button></header><button id="field">Field</button></section>',
    );
    const action = vi.fn();
    document.getElementById("refresh")!.addEventListener("click", action);
    navigation.refresh();
    await navigation.press(8);
    expect(action).toHaveBeenCalledTimes(1);
    expect(gamepadTarget.value?.id).toBe("field");
    const dialog = document.createElement("div");
    dialog.className = "modal";
    dialog.innerHTML = '<button id="inside">Inside</button>';
    document.body.append(dialog);
    rects.set(dialog, new DOMRect(0, 0, 200, 200));
    rects.set(dialog.querySelector("button")!, new DOMRect(0, 0, 100, 40));
    await navigation.press(8);
    expect(action).toHaveBeenCalledTimes(1);
  });
  it("滑块左右调值, 上下离开控件", async () => {
    region(
      '<section class="page is-active"><input id="slider" type="range" min="0" max="100" value="50" step="5"><button id="next">Next</button></section>',
    );
    navigation.refresh();
    await navigation.press(14);
    expect((document.getElementById("slider") as HTMLInputElement).value).toBe(
      "55",
    );
    await navigation.press(12);
    expect(gamepadTarget.value?.id).toBe("next");
  });
  it("嵌套弹窗返回原控件", async () => {
    region(
      '<section class="page is-active"><button id="entry">Open</button></section>',
    );
    navigation.refresh();
    const dialog = document.createElement("div");
    dialog.className = "modal";
    dialog.innerHTML =
      '<div class="modal__panel"><button id="close" aria-label="关闭弹窗"></button><button id="inside">Inside</button></div>';
    document.body.append(dialog);
    dialog
      .querySelectorAll("button")
      .forEach((e, i) => rects.set(e, new DOMRect(20, i * 60, 180, 40)));
    rects.set(dialog, new DOMRect(0, 0, 300, 300));
    dialog
      .querySelector("#close")!
      .addEventListener("click", () => dialog.remove());
    navigation.refresh();
    expect(gamepadTarget.value?.id).toBe("inside");
    await navigation.press(1);
    expect(gamepadTarget.value?.id).toBe("entry");
  });
  it("图表取消先退出浏览, 再返回主页", async () => {
    region(
      '<section class="page is-active"><div tabindex="0" data-pad-region></div></section>',
    );
    let browsing = true;
    document
      .querySelector("[data-pad-region]")!
      .addEventListener("keydown", (event) => {
        if ((event as KeyboardEvent).key === "Escape" && browsing) {
          event.preventDefault();
          browsing = false;
        }
      });
    navigation.refresh();
    await navigation.press(1);
    expect(showPage).not.toHaveBeenCalled();
    await navigation.press(1);
    expect(showPage).toHaveBeenCalledWith("home");
  });
  it("右摇杆只滚动当前内容, 选择跟随可见控件且不滚动外层窗口", () => {
    region(
      '<div class="app" style="overflow:hidden"><section class="page is-active"><div class="settings-panel is-current" style="overflow-y:auto"><button id="a">A</button><button id="b">B</button><button id="c">C</button></div></section></div>',
    );
    const panel = document.querySelector<HTMLElement>(".settings-panel")!;
    const app = document.querySelector<HTMLElement>(".app")!;
    Object.defineProperty(panel, "clientHeight", { value: 180 });
    Object.defineProperty(panel, "scrollHeight", { value: 600 });
    rects.set(app, new DOMRect(0, 0, 400, 768));
    rects.set(panel, new DOMRect(0, 100, 400, 180));
    ["a", "b", "c"].forEach((id, index) =>
      rects.set(
        document.getElementById(id)!,
        new DOMRect(20, 110 + 90 * index, 180, 40),
      ),
    );
    const outer = vi.fn();
    app.scrollBy = outer;
    panel.scrollBy = vi.fn(({ top }: ScrollToOptions) => {
      for (const id of ["a", "b", "c"]) {
        const item = document.getElementById(id)!;
        const rect = rects.get(item)!;
        rects.set(
          item,
          new DOMRect(rect.x, rect.y - Number(top), rect.width, rect.height),
        );
      }
    });
    navigation.refresh();
    expect(gamepadTarget.value?.id).toBe("a");
    navigation.scroll(100);
    expect(gamepadTarget.value?.id).toBe("c");
    expect(outer).not.toHaveBeenCalled();
  });
  it("分类栏中的右摇杆不会滚动未进入的内容页面", () => {
    region(
      '<section class="page is-active"><nav class="settings-categories"><button data-category="appearance" aria-current="true">外观</button></nav><div class="settings-panel is-current" style="overflow-y:auto"><button>Field</button></div></section>',
    );
    const panel = document.querySelector<HTMLElement>(".settings-panel")!;
    Object.defineProperty(panel, "clientHeight", { value: 100 });
    Object.defineProperty(panel, "scrollHeight", { value: 800 });
    panel.scrollBy = vi.fn();
    navigation.refresh();
    navigation.scroll(100);
    expect(panel.scrollBy).not.toHaveBeenCalled();
    expect(gamepad.navigation).toBe(true);
  });
  it("下拉框展开时右摇杆由列表接管, 不滚动底下的设置", async () => {
    document.body.innerHTML =
      '<section class="page is-active"><div class="settings-panel is-current" style="overflow-y:auto"></div></section>';
    const panel = document.querySelector<HTMLElement>(".settings-panel")!;
    const wrapper = mount(BaseSelect, {
      props: {
        id: "scroll-select",
        modelValue: "a",
        options: [
          { value: "a", label: "A" },
          { value: "b", label: "B" },
        ],
      },
      attachTo: panel,
    });
    const trigger = wrapper.get('[role="combobox"]').element;
    rects.set(trigger, new DOMRect(20, 20, 180, 40));
    Object.defineProperty(panel, "clientHeight", { value: 200 });
    Object.defineProperty(panel, "scrollHeight", { value: 600 });
    rects.set(panel, new DOMRect(0, 0, 300, 200));
    panel.scrollBy = vi.fn();
    navigation.refresh();
    await navigation.press(0);
    const original = trigger.getAttribute("aria-activedescendant");
    navigation.scroll(90);
    await nextTick();
    expect(trigger.getAttribute("aria-activedescendant")).not.toBe(original);
    expect(panel.scrollBy).not.toHaveBeenCalled();
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    wrapper.unmount();
  });
  it("方向键经过收起的下拉框时不会展开或拦截移动", async () => {
    document.body.innerHTML = '<section class="page is-active"></section>';
    const section = document.querySelector("section")!;
    const wrapper = mount(BaseSelect, {
      props: {
        id: "closed",
        modelValue: "a",
        options: [
          { value: "a", label: "A" },
          { value: "b", label: "B" },
        ],
      },
      attachTo: section,
    });
    const next = document.createElement("button");
    next.id = "next-field";
    section.append(next);
    const trigger = wrapper.get('[role="combobox"]').element;
    rects.set(trigger, new DOMRect(20, 20, 180, 40));
    rects.set(next, new DOMRect(20, 100, 180, 40));
    navigation.refresh();
    await navigation.press(12);
    expect(gamepadTarget.value).toBe(next);
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    await navigation.press(11);
    expect(gamepadTarget.value).toBe(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    await navigation.press(0);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    await navigation.press(12);
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    await navigation.press(1);
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    await navigation.press(12);
    expect(gamepadTarget.value).toBe(next);
    wrapper.unmount();
  });
  it("只滚动声明可滚动的内容纵轴, 不调用跨容器 scrollIntoView", () => {
    region(
      '<div class="app" style="overflow:hidden"><section class="page is-active" style="overflow-y:auto"><button id="field">Field</button></section></div>',
    );
    const scroller = document.querySelector<HTMLElement>("section")!;
    Object.defineProperty(scroller, "clientHeight", { value: 100 });
    Object.defineProperty(scroller, "scrollHeight", { value: 600 });
    rects.set(scroller, new DOMRect(0, 0, 200, 100));
    rects.set(document.getElementById("field")!, new DOMRect(10, 400, 180, 40));
    const scroll = vi.fn();
    scroller.scrollBy = scroll;
    navigation.refresh();
    expect(scroll).toHaveBeenCalledWith({ top: 348, behavior: "instant" });
    expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled();
  });
  it("真实下拉控件可选择、确认、取消", async () => {
    document.body.innerHTML = '<section class="page is-active"></section>';
    const selected = ref("a");
    const wrapper = mount(BaseSelect, {
      props: {
        id: "choices",
        modelValue: "a",
        options: [
          { value: "a", label: "A" },
          { value: "b", label: "B" },
        ],
        "onUpdate:modelValue": (v: string) => {
          selected.value = v;
          wrapper.setProps({ modelValue: v });
        },
      },
      attachTo: document.querySelector("section")!,
    });
    const trigger = wrapper.get('[role="combobox"]').element;
    rects.set(trigger, new DOMRect(20, 20, 180, 40));
    navigation.refresh();
    await navigation.press(0);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    await navigation.press(12);
    await navigation.press(0);
    expect(selected.value).toBe("b");
    await navigation.press(0);
    await navigation.press(1);
    expect(selected.value).toBe("b");
    wrapper.unmount();
  });
  it("屏幕键盘确认提交, 返回取消", async () => {
    region(
      '<section class="page is-active"><input id="field" value="abc"></section>',
    );
    const wrapper = mount(GamepadKeyboard, { attachTo: document.body });
    navigation.refresh();
    await navigation.press(0);
    expect(gamepad.editing?.id).toBe("field");
    await nextTick();
    await wrapper.findAll(".pad-keyboard__row button")[0].trigger("click");
    await wrapper.findAll("button").at(-1)!.trigger("click");
    expect((document.getElementById("field") as HTMLInputElement).value).toBe(
      "abc1",
    );
    expect(gamepad.editing).toBe(null);
    wrapper.unmount();
  });
});

it("轮询处理重获焦点、按键边缘、重复、摇杆、切换输入与停止", async () => {
  bridge.native = true;
  region(
    '<section class="page is-active"><button id="a">A</button><button id="b">B</button><button id="c">C</button></section>',
  );
  let snapshot = {
    devices: [{ ...gamepad.device! }],
    device: { ...gamepad.device! },
    idle: { requested: false, error: null },
    buttons: [] as number[],
    pressed: [] as number[],
    axes: [0, 0, 0, 0, 0, 0],
    focused: true,
    error: null as string | null,
  };
  bridge.invoke.mockImplementation(async (command: string) =>
    command === "gamepad_poll" ? structuredClone(snapshot) : null,
  );
  const stop = startGamepad();
  await vi.advanceTimersByTimeAsync(20);
  snapshot.pressed = [12];
  snapshot.buttons = [12];
  await vi.advanceTimersByTimeAsync(20);
  snapshot.pressed = [];
  await vi.advanceTimersByTimeAsync(450);
  expect(gamepadTarget.value?.id).toBe("c");
  snapshot.buttons = [];
  snapshot.axes = [-0.8, 0, 0, 0.8, 0, 0];
  await vi.advanceTimersByTimeAsync(40);
  snapshot.axes = [0, -0.8, 0, 0, 0, 0];
  await vi.advanceTimersByTimeAsync(40);
  snapshot.axes = [0.8, 0, 0, -0.8, 0, 0];
  await vi.advanceTimersByTimeAsync(40);
  snapshot.axes = [0, 0.8, 0, 0, 0, 0];
  await vi.advanceTimersByTimeAsync(40);
  snapshot.axes = [0, 0, 0, 0, 0, 0];
  snapshot.focused = false;
  await vi.advanceTimersByTimeAsync(200);
  expect(gamepad.active).toBe(false);
  snapshot.focused = true;
  snapshot.buttons = [0];
  await vi.advanceTimersByTimeAsync(200);
  snapshot.buttons = [];
  await vi.advanceTimersByTimeAsync(20);
  snapshot.pressed = [0];
  await vi.advanceTimersByTimeAsync(20);
  snapshot.pressed = [];
  document.dispatchEvent(new MouseEvent("pointermove", { movementX: 0 }));
  document.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
  await vi.advanceTimersByTimeAsync(20);
  expect(gamepad.active).toBe(false);
  window.dispatchEvent(new Event("blur"));
  snapshot.error = "device issue";
  await vi.advanceTimersByTimeAsync(20);
  expect(gamepad.error).toBe("device issue");
  bridge.invoke.mockRejectedValueOnce(new Error("poll failed"));
  await vi.advanceTimersByTimeAsync(20);
  stop();
  await vi.advanceTimersByTimeAsync(250);
  gamepad.device = null;
  await gamepadFeedback("confirm");
  gamepad.device = snapshot.device;
  gamepad.active = true;
  bridge.invoke.mockRejectedValueOnce(new Error("feedback failed"));
  await gamepadFeedback("confirm");
  expect(gamepad.error).toContain("feedback failed");
});
