import { expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

it("主要按钮折叠为带图标入口, ghost 固定在右侧且没有图标", async () => {
  const n = await import("../../src/core/notifications");
  const { default: Actions } =
    await import("../../src/components/NotificationActions.vue");
  const primary = vi.fn();
  const secondary = vi.fn();
  n.notify("公告获取失败", "error", 0, {
    primary: { label: "重试", icon: "i-arrows-clockwise", run: primary },
    secondary: { label: "查看公告", run: secondary },
  });
  const wrapper = mount(Actions, {
    props: { item: n.notifications[0], reduced: true },
  });
  const buttons = wrapper.findAll("button");
  expect(buttons).toHaveLength(2);
  expect(buttons[0].classes()).toContain("notification-action--primary");
  expect(buttons[0].attributes("aria-label")).toBe("重试");
  expect(buttons[0].find("svg use").attributes("href")).toBe(
    "#i-arrows-clockwise",
  );
  expect(buttons[1].classes()).toContain("notification-action--ghost");
  expect(buttons[1].text()).toBe("查看公告");
  expect(buttons[1].find("svg").exists()).toBe(false);
  expect(
    wrapper.get(".notification-action-label").attributes("style"),
  ).toContain("width: 0px");
  await buttons[1].trigger("click");
  await flushPromises();
  expect(secondary).toHaveBeenCalledOnce();
  expect(primary).not.toHaveBeenCalled();
  wrapper.unmount();
  n.resetNotifications();
});
