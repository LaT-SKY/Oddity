import { test, expect } from "@playwright/test";
import { settingsCategory } from "./settings-helpers";

declare global {
  interface Window {
    logExportCalls: string[];
  }
}

for (const width of [1180, 640]) {
  test(`日志检查预览, 取消与保存 (${width}px)`, async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.setViewportSize({ width, height: width === 640 ? 520 : 760 });
    await page.goto("/#settings");
    await settingsCategory(page, "maintenance");
    await expect(page.locator("#settings-export-logs")).toBeVisible();
    // 只替换日志相关 IPC; 页面初始化仍使用项目已有的浏览器预览数据.
    await page.evaluate(() => {
      const calls: string[] = [];
      let selections = 0;
      Object.assign(window, {
        isTauri: true,
        logExportCalls: calls,
        __TAURI_INTERNALS__: {
          invoke: async (command: string, args?: { id?: string }) => {
            if (command === "game_status")
              return {
                running: false,
                closing: false,
                pid: 0,
                startedAt: 0,
                exe: "",
                log: "",
              };
            calls.push(command);
            if (command === "prepare_log_export")
              return {
                id: "fixture-snapshot",
                fileCount: 8,
                redactions: { home: 4, phone: 2, credentials: 3, jwt: 1 },
                normalizedFiles: 1,
                samples: Array.from({ length: 12 }, (_, index) => ({
                  file: "environments/env-1/nte-app-launch.log",
                  line: index + 1,
                  text:
                    "prefix=$HOME/Games/异环 token=[凭据已移除] phone=[手机号已移除] " +
                    "诊断信息 ".repeat(24),
                })),
              };
            if (command === "discard_log_export") return;
            if (command === "plugin:dialog|open")
              return ++selections === 1 ? null : "/tmp/log-export";
            if (command === "save_log_export") {
              if (args?.id !== "fixture-snapshot")
                throw new Error("wrong snapshot");
              return "/tmp/log-export/oddity-logs.zip";
            }
            if (command === "open_directory") return;
            throw new Error(`Unexpected IPC: ${command}`);
          },
        },
      });
    });
    await page.locator("#settings-export-logs").click();
    const modal = page.getByRole("dialog", { name: "日志隐私检查" });
    await expect(modal).toBeVisible();
    await expect(modal).toContainText("8 个日志文件");
    await expect(modal.locator("pre")).toHaveCount(0);
    const toggle = modal.getByRole("button", { name: "脱敏预览" });
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(
      modal.getByRole("button", { name: "保存脱敏日志" }),
    ).toBeInViewport();
    await toggle.click();
    await expect(modal.locator("pre")).toHaveCount(12);
    const notice = page.getByRole("region", { name: "通知" });
    await expect(notice).toBeVisible();
    await expect
      .poll(async () =>
        page.evaluate(() => {
          const notification = document.querySelector(".notification-stack")!;
          const bounds = notification.getBoundingClientRect();
          const panel = document
            .querySelector(".modal__panel")!
            .getBoundingClientRect();
          const x = Math.max(bounds.left, panel.left) + 8;
          const y = Math.max(bounds.top, panel.top) + 8;
          if (
            x >= Math.min(bounds.right, panel.right) ||
            y >= Math.min(bounds.bottom, panel.bottom)
          )
            return false;
          return Boolean(
            document.elementFromPoint(x, y)?.closest(".notification-stack"),
          );
        }),
      )
      .toBe(true);
    expect(
      await modal.evaluate((el) => el.scrollWidth - el.clientWidth),
    ).toBeLessThanOrEqual(1);
    expect(await page.evaluate(() => window.logExportCalls)).toEqual([
      "prepare_log_export",
    ]);
    await page.keyboard.press("Escape");
    await expect(modal).not.toBeVisible();
    await expect(page.locator("#settings-export-logs")).toBeFocused();
    await page.locator("#settings-export-logs").click();
    await expect(modal).toBeVisible();
    await expect(
      modal.getByRole("button", { name: "脱敏预览" }),
    ).toHaveAttribute("aria-expanded", "false");
    await modal.getByRole("button", { name: "保存脱敏日志" }).click();
    await expect(modal).toBeVisible();
    await modal.locator(".modal__body").evaluate((el) => (el.scrollTop = 0));
    await page.screenshot({ path: `test-results/log-export-${width}.png` });
    await modal.getByRole("button", { name: "保存脱敏日志" }).click();
    await expect(modal).not.toBeVisible();
    await expect(
      page.locator(".log-export__result[role=status]"),
    ).toContainText("/tmp/log-export/oddity-logs.zip");
    const calls = await page.evaluate(() => window.logExportCalls);
    expect(
      calls.filter((command) => command === "save_log_export"),
    ).toHaveLength(1);
    expect(calls).toContain("discard_log_export");
    expect(calls.at(-1)).toBe("open_directory");
    expect(errors).toEqual([]);
  });
}
