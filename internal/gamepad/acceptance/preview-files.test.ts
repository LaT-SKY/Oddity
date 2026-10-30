import { expect, it } from "vitest";
import { invoke } from "../../../launcher/ui/src/core/api";
import { defaultProfile } from "../../../launcher/ui/src/types/gamepad";

it("预览模式区分浏览文件与反馈配置导入错误", async () => {
  await expect(invoke("browse_files", { path: null })).rejects.toThrow(
    "请在 Oddity 桌面应用中浏览文件.",
  );
  await expect(
    invoke("gamepad_profile_validate", { profile: defaultProfile() }),
  ).rejects.toThrow("请在 Oddity 桌面应用中导入反馈配置.");
});
