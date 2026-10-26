import { test, expect } from "@playwright/test";

test("创建、切换、保留和删除实验环境", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await page.locator('[data-page="settings"]').click();
  const section = page
    .locator("section", { has: page.locator("#runtime-environments-title") })
    .last();
  await section.getByRole("button", { name: "从所选环境创建副本" }).click();
  await page.locator("#runtime-environment-name").fill("GE 新版测试");
  await page.locator("#runtime-environment-proton").fill("/opt/GE-Proton-New");
  await section
    .getByRole("button", { name: "创建实验环境", exact: true })
    .click();
  const select = page.locator("#runtime-environment-trigger");
  await expect(select).toHaveText(/原始环境/);
  await select.click();
  await page.getByRole("option", { name: "GE 新版测试", exact: true }).click();
  await section.getByRole("button", { name: "切换到此环境" }).click();
  await expect(select).toHaveText(/GE 新版测试 · 当前/);
  await expect(
    section.getByRole("button", { name: "删除实验环境" }),
  ).toBeDisabled();
  const currentPrefix = await section.getByText(/^当前 prefix:/).innerText();
  const originalProton = await page.locator("#set-proton").inputValue();
  await select.click();
  await page
    .getByRole("option", { name: "原始环境 · 稳定", exact: true })
    .click();
  await section.getByRole("button", { name: "从所选环境创建副本" }).click();
  await expect(section).toContainText("源环境: 原始环境");
  await expect(page.locator("#runtime-environment-proton")).toHaveValue(
    originalProton,
  );
  await page.locator("#runtime-environment-name").fill("稳定环境副本");
  await section
    .getByRole("button", { name: "创建实验环境", exact: true })
    .click();
  await expect(page.locator("#runtime-environment-name")).toHaveCount(0);
  await expect(section.getByText(/^当前 prefix:/)).toHaveText(currentPrefix);
  await select.click();
  await page
    .getByRole("option", { name: "GE 新版测试 · 当前", exact: true })
    .click();
  await section.getByRole("button", { name: "保留为稳定环境" }).click();
  await expect(select).toHaveText(/当前 · 稳定/);
  await select.click();
  await page.getByRole("option", { name: "原始环境", exact: true }).click();
  await section.getByRole("button", { name: "切换到此环境" }).click();
  await section.getByRole("button", { name: "保留为稳定环境" }).click();
  await select.click();
  await page.getByRole("option", { name: "GE 新版测试", exact: true }).click();
  await section.getByRole("button", { name: "删除实验环境" }).click();
  await expect(section).toContainText("确定删除“GE 新版测试”的 prefix?");
  await section.getByRole("button", { name: "确定删除", exact: true }).click();
  await select.click();
  await expect(
    page.getByRole("option", { name: "GE 新版测试", exact: true }),
  ).toHaveCount(0);
  expect(errors).toEqual([]);
});
