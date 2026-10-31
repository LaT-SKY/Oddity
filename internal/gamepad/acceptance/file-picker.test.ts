import { beforeEach, describe, expect, it, vi } from "vitest";
const native = vi.hoisted(() => ({
  open: vi.fn().mockResolvedValue("/native"),
  save: vi.fn().mockResolvedValue("/save"),
}));
vi.mock("../../../launcher/ui/src/core/desktop", () => native);
vi.mock("../../../launcher/ui/src/core/gamepad", async () => {
  const { reactive } = await import("vue");
  return { gamepad: reactive({ active: false }) };
});
import { gamepad } from "../../../launcher/ui/src/core/gamepad";
import {
  open,
  save,
  fileRequest,
} from "../../../launcher/ui/src/core/filePicker";
beforeEach(() => {
  gamepad.active = false;
  fileRequest.value = null;
});
describe("文件选择输入方式", () => {
  it("鼠标使用系统对话框", async () => {
    expect(await open({ directory: true })).toBe("/native");
    expect(await save({ defaultPath: "test.json" })).toBe("/save");
  });
  it("手柄选择目录、过滤文件并返回选择结果", async () => {
    gamepad.active = true;
    const result = open({
      title: "目录",
      directory: true,
      defaultPath: "/home/test",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    expect(fileRequest.value).toMatchObject({
      title: "目录",
      directory: true,
      save: false,
      extensions: ["json"],
    });
    fileRequest.value!.resolve("/chosen");
    expect(await result).toBe("/chosen");
  });
  it("默认标题和取消", async () => {
    gamepad.active = true;
    const result = open({});
    expect(fileRequest.value?.title).toBe("选择文件");
    expect(fileRequest.value?.extensions).toEqual([]);
    fileRequest.value!.resolve(null);
    expect(await result).toBeNull();
  });
  it("保存走应用内文件浏览", async () => {
    gamepad.active = true;
    const result = save({});
    expect(fileRequest.value).toMatchObject({
      title: "保存文件",
      directory: false,
      save: true,
    });
    fileRequest.value!.resolve("/new.json");
    expect(await result).toBe("/new.json");
  });
});
