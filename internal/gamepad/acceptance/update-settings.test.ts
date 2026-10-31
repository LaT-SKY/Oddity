import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, flushPromises, type VueWrapper } from "@vue/test-utils";
import { nextTick } from "vue";
vi.mock("../../../launcher/ui/src/core/api", () => ({
  invoke: vi.fn().mockResolvedValue(null),
  isDesktop: () => false,
}));
import * as c from "../../../launcher/ui/src/core/controller";
import { settingsNavigation } from "../../../launcher/ui/src/core/settingsCatalog";
import {
  createGamepadNavigation,
  gamepad,
  gamepadTarget,
} from "../../../launcher/ui/src/core/gamepad";
import SettingsPage from "../../../launcher/ui/src/pages/SettingsPage.vue";
import GamepadKeyboard from "../../../launcher/ui/src/components/GamepadKeyboard.vue";

let wrapper: VueWrapper;
const inertDescriptor = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "inert",
);
let navigation: ReturnType<typeof createGamepadNavigation>;
beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, "inert", {
    configurable: true,
    get() {
      return this.hasAttribute("inert");
    },
    set(value: boolean) {
      this.toggleAttribute("inert", value);
    },
  });
  c.dispose();
  c.state.page = "settings";
  c.form.reduceMotion = true;
  Object.assign(c.form.updates, {
    autoDownload: false,
    metered: "ask",
    windowEnabled: false,
    windowStart: "02:00",
    windowEnd: "07:00",
    limitBytesPerSecond: 0,
  });
  settingsNavigation.category = "game";
  settingsNavigation.query = "";
  Object.assign(gamepad, {
    active: true,
    navigation: false,
    windowControls: false,
    notification: false,
    editing: null,
    device: null,
    deviceError: "",
    feedbackError: "",
  });
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.scrollBy = vi.fn();
  const visible = (element: Element) => !element.closest("[inert],[hidden]");
  vi.spyOn(Element.prototype, "getClientRects").mockImplementation(function (
    this: Element,
  ) {
    return (visible(this)
      ? [new DOMRect(20, 20, 200, 24)]
      : []) as unknown as DOMRectList;
  });
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(
    function (this: Element) {
      const controls = [
        ...document.querySelectorAll("button,input,textarea"),
      ].filter(visible);
      return new DOMRect(20, Math.max(0, controls.indexOf(this)) * 24, 200, 24);
    },
  );
  wrapper = mount(
    {
      components: { SettingsPage, GamepadKeyboard },
      template: "<div><SettingsPage /><GamepadKeyboard /></div>",
    },
    { attachTo: document.body },
  );
  navigation = createGamepadNavigation();
});
afterEach(() => {
  if (inertDescriptor)
    Object.defineProperty(HTMLElement.prototype, "inert", inertDescriptor);
  else Reflect.deleteProperty(HTMLElement.prototype, "inert");
  navigation?.dispose();
  wrapper?.unmount();
  c.dispose();
  gamepad.editing = null;
});
async function reach(id: string) {
  for (let step = 0; step < 30 && gamepadTarget.value?.id !== id; step++)
    await navigation.press(12);
  expect(gamepadTarget.value?.id).toBe(id);
}
it("设备错误清除后仍显示独立的反馈失败原因", async () => {
  settingsNavigation.category = "gamepad";
  gamepad.deviceError = "设备打开失败";
  gamepad.feedbackError = "振动调用失败";
  await nextTick();
  expect(
    wrapper.findAll(".gamepad-settings-hint").map((item) => item.text()),
  ).toEqual(["设备打开失败", "振动调用失败"]);
  gamepad.deviceError = "";
  await nextTick();
  expect(
    wrapper.findAll(".gamepad-settings-hint").map((item) => item.text()),
  ).toEqual(["振动调用失败"]);
});
it("从分类进入更新设置, 展开项目可达, 返回恢复分类", async () => {
  navigation.refresh();
  expect(gamepadTarget.value?.dataset.category).toBe("game");
  await navigation.press(0);
  await reach("set-auto-download");
  await navigation.press(0);
  expect(c.form.updates.autoDownload).toBe(true);
  await reach("set-metered-trigger");
  await navigation.press(0);
  await navigation.press(12);
  await navigation.press(0);
  expect(c.form.updates.metered).toBe("pause");
  await reach("set-updates-window");
  await navigation.press(0);
  expect(c.form.updates.windowEnabled).toBe(true);
  await reach("set-updates-window-start");
  await reach("set-updates-window-end");
  await reach("set-download-limit");
  await navigation.press(0);
  expect(gamepad.editing?.id).toBe("set-download-limit");
  expect(wrapper.text()).not.toContain("小写");
  const finish = wrapper
    .findAll(".pad-keyboard button")
    .find((button) => button.text() === "完成")!;
  const one = wrapper
    .findAll(".pad-keyboard__row button")
    .find((button) => button.text() === "1")!;
  await one.trigger("click");
  await finish.trigger("click");
  expect(c.form.updates.limitBytesPerSecond).toBe(1024 * 1024);
  await navigation.press(1);
  expect(gamepad.navigation).toBe(true);
  expect(gamepadTarget.value?.dataset.category).toBe("game");
});
it("时间键盘支持数字与冒号, 拒绝非法时间并提交有效时段", async () => {
  c.form.updates.autoDownload = true;
  c.form.updates.windowEnabled = true;
  await nextTick();
  navigation.refresh();
  await navigation.press(0);
  await reach("set-updates-window-start");
  await navigation.press(0);
  expect(gamepad.editing?.type).toBe("time");
  const key = (text: string) =>
    wrapper
      .findAll(".pad-keyboard button")
      .find((button) => button.text() === text)!;
  expect(key(":")).toBeDefined();
  expect(key("字母")).toBeUndefined();
  await key("清空").trigger("click");
  for (const digit of "25:90") await key(digit).trigger("click");
  await key("完成").trigger("click");
  expect(wrapper.get('[role="alert"]').text()).toContain("HH:mm");
  expect(c.form.updates.windowStart).toBe("02:00");
  expect(gamepad.editing).not.toBeNull();
  await key("清空").trigger("click");
  for (const digit of "23:30") await key(digit).trigger("click");
  await key("完成").trigger("click");
  await flushPromises();
  expect(c.form.updates.windowStart).toBe("23:30");
  expect(gamepad.editing).toBeNull();
});
