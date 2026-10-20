import { expect, it, vi } from "vitest";

vi.mock("../../src/App.vue", () => ({
  default: { template: "<p>Oddity</p>" },
}));

it("将根组件挂载到页面入口", async () => {
  document.body.innerHTML = '<div id="root"></div>';
  await import("../../src/main");
  expect(document.querySelector("#root")?.textContent).toBe("Oddity");
  expect(document.querySelector("#root")?.hasAttribute("data-v-app")).toBe(
    true,
  );
});
