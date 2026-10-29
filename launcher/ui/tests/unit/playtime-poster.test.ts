import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { buildPlaytimeStats } from "../../src/core/playtime";
import PlaytimePoster from "../../src/components/playtime/PlaytimePoster.vue";

const backend = vi.hoisted(() => ({
  save: vi.fn(),
  invoke: vi.fn(),
  isTauri: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: backend.save }));
vi.mock("../../src/core/api", () => ({
  invoke: backend.invoke,
  isTauri: backend.isTauri,
}));
vi.mock("../../src/core/notifications", () => ({ notify: backend.notify }));

const now = new Date(2026, 9, 28, 12).getTime();
const stats = buildPlaytimeStats(
  {
    sessions: [
      {
        startedAt: now - 3_600_000,
        endedAt: now,
        mode: "official",
        distribution: "cn",
      },
    ],
    active: null,
  },
  now,
);
const dataUrl = "data:image/png;base64,cG9zdGVy";
let wrapper: VueWrapper;
let images: {
  src: string;
  crossOrigin: string;
  onload: (() => void) | null;
  onerror: (() => void) | null;
}[];
let context: CanvasRenderingContext2D;

beforeEach(() => {
  vi.setSystemTime(now);
  backend.save.mockReset().mockResolvedValue("/saved/poster.png");
  backend.invoke
    .mockReset()
    .mockImplementation(async (command) =>
      command === "launcher_background"
        ? { image: "/official.jpg", video: null }
        : "/saved/poster.png",
    );
  backend.isTauri.mockReset().mockReturnValue(true);
  backend.notify.mockReset();
  images = [];
  vi.stubGlobal(
    "Image",
    class {
      src = "";
      crossOrigin = "";
      naturalWidth = 1920;
      naturalHeight = 1080;
      onload = null;
      onerror = null;
      constructor() {
        images.push(this);
      }
    },
  );
  context = {
    fillRect: vi.fn(),
    drawImage: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
    fillText: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arcTo: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    arc: vi.fn(),
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
  } as unknown as CanvasRenderingContext2D;
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context);
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue(dataUrl);
});
afterEach(() => wrapper?.unmount());

async function render() {
  wrapper = mount(PlaytimePoster, {
    props: { open: true, stats, reduceMotion: true },
    global: {
      stubs: {
        BaseModal: {
          name: "BaseModal",
          props: ["open"],
          emits: ["update:open"],
          template: '<section v-if="open" role="dialog"><slot /></section>',
        },
      },
    },
  });
  await flushPromises();
}
async function generate() {
  await render();
  images[0].onload?.();
  await flushPromises();
}
const saveButton = () =>
  wrapper.get(".playtime-poster__actions button:last-child");

it("等待壁纸加载时禁用保存, 生成后显示 PNG 预览", async () => {
  await render();
  expect(wrapper.get('[role="status"]').text()).toContain("正在生成");
  expect(saveButton().attributes()).toHaveProperty("disabled");
  expect(images[0].src).toBe("/official.jpg");
  expect(images[0].crossOrigin).toBe("anonymous");
  images[0].onload?.();
  await flushPromises();
  expect(wrapper.get("img").attributes("src")).toBe(dataUrl);
  expect(saveButton().attributes("disabled")).toBeUndefined();
  expect(HTMLCanvasElement.prototype.toDataURL).toHaveBeenCalledExactlyOnceWith(
    "image/png",
  );
});

it("官方壁纸失败时使用内置壁纸, 全部失败时仍生成海报", async () => {
  await render();
  images[0].onerror?.();
  await flushPromises();
  expect(images[1].src).toBe(
    new URL("assets/hero-keyart.jpg", window.location.href).href,
  );
  images[1].onerror?.();
  await flushPromises();
  expect(wrapper.find("img").exists()).toBe(true);
  expect(context.drawImage).not.toHaveBeenCalled();
  expect(wrapper.find('[role="alert"]').exists()).toBe(false);
});

it("背景查询失败时继续使用内置壁纸", async () => {
  backend.invoke.mockRejectedValueOnce(new Error("background unavailable"));
  await render();
  expect(images[0].src).toContain("assets/hero-keyart.jpg");
  images[0].onload?.();
  await flushPromises();
  expect(wrapper.find("img").exists()).toBe(true);
});

it("保存时只发送 PNG 的 base64 数据并显示实际保存路径", async () => {
  await generate();
  await saveButton().trigger("click");
  await flushPromises();
  expect(backend.save).toHaveBeenCalledExactlyOnceWith({
    title: "保存游玩记录海报",
    defaultPath: "异环游玩记录-2026-10-28.png",
    filters: [{ name: "PNG 图片", extensions: ["png"] }],
  });
  expect(backend.invoke).toHaveBeenCalledWith("playtime_export_poster", {
    destination: "/saved/poster.png",
    data: "cG9zdGVy",
  });
  expect(wrapper.get(".playtime-poster__saved").text()).toContain(
    "/saved/poster.png",
  );
  expect(backend.notify).toHaveBeenCalledWith("海报已保存.", "ok", 3400, {
    key: "playtime-poster",
  });
});

it("取消保存对话框不导出数据或提示成功, 可再次保存", async () => {
  backend.save.mockResolvedValueOnce(null);
  await generate();
  await saveButton().trigger("click");
  await flushPromises();
  expect(
    backend.invoke.mock.calls.filter(
      ([command]) => command === "playtime_export_poster",
    ),
  ).toHaveLength(0);
  expect(backend.notify).not.toHaveBeenCalled();
  expect(wrapper.find(".playtime-poster__saved").exists()).toBe(false);
  await saveButton().trigger("click");
  await flushPromises();
  expect(wrapper.find(".playtime-poster__saved").exists()).toBe(true);
});

it.each(["dialog", "export"])(
  "%s 失败显示错误, 重试后清除错误",
  async (stage) => {
    await generate();
    if (stage === "dialog")
      backend.save.mockRejectedValueOnce(new Error("disk unavailable"));
    else backend.invoke.mockRejectedValueOnce(new Error("disk unavailable"));
    await saveButton().trigger("click");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "保存海报失败: Error: disk unavailable",
    );
    expect(backend.notify).toHaveBeenCalledWith(
      expect.stringContaining("disk unavailable"),
      "error",
      6500,
      { key: "playtime-poster" },
    );
    expect(saveButton().attributes("disabled")).toBeUndefined();
    await saveButton().trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.find(".playtime-poster__saved").exists()).toBe(true);
  },
);

it("保存进行中阻止重复保存和关闭", async () => {
  await generate();
  let finish!: (path: string) => void;
  backend.save.mockReturnValueOnce(
    new Promise<string>((resolve) => {
      finish = resolve;
    }),
  );
  await saveButton().trigger("click");
  await saveButton().trigger("click");
  const modal = wrapper.findComponent({ name: "BaseModal" });
  modal.vm.$emit("update:open", false);
  expect(wrapper.emitted("update:open")).toBeUndefined();
  expect(backend.save).toHaveBeenCalledOnce();
  finish("/saved/poster.png");
  await flushPromises();
  await wrapper
    .get(".playtime-poster__actions button:first-child")
    .trigger("click");
  expect(wrapper.emitted("update:open")).toEqual([[false]]);
});

it("浏览器预览通过下载链接保存并清理链接", async () => {
  backend.isTauri.mockReturnValue(false);
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(function (this: HTMLAnchorElement) {
      expect(this.download).toBe("异环游玩记录-2026-10-28.png");
      expect(this.href).toBe(dataUrl);
      expect(this.isConnected).toBe(true);
    });
  await generate();
  await saveButton().trigger("click");
  await flushPromises();
  expect(click).toHaveBeenCalledOnce();
  expect(document.querySelector("a[download]")).toBeNull();
  expect(backend.save).not.toHaveBeenCalled();
  expect(backend.invoke).not.toHaveBeenCalled();
  expect(backend.notify).toHaveBeenCalledWith(
    "预览模式: 已下载海报.",
    "ok",
    3400,
    { key: "playtime-poster" },
  );
});

it("无法创建画布时显示生成错误且保持保存禁用", async () => {
  vi.mocked(HTMLCanvasElement.prototype.getContext).mockReturnValue(null);
  await generate();
  expect(wrapper.get('[role="alert"]').text()).toContain("无法创建画布");
  expect(saveButton().attributes()).toHaveProperty("disabled");
  expect(wrapper.find("img").exists()).toBe(false);
});

it("生成期间关闭后不发布预览, 重新打开可再次生成", async () => {
  await render();
  await wrapper.setProps({ open: false });
  images[0].onload?.();
  await flushPromises();
  expect(HTMLCanvasElement.prototype.toDataURL).not.toHaveBeenCalled();
  await wrapper.setProps({ open: true });
  await flushPromises();
  expect(images).toHaveLength(1);
  expect(wrapper.find("img").exists()).toBe(true);
  expect(wrapper.find(".playtime-poster__saved").exists()).toBe(false);
});
