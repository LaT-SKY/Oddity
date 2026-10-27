import { test, expect } from "@playwright/test";

test("先创建环境再启动, A/B 副本、稳定标记及删除弹窗", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await page.locator("#launch-btn").click();
  await expect(page.locator("#page-settings")).toBeVisible();
  await expect(page.locator("#set-proton")).toHaveCount(0);
  await expect(page.locator("#set-prefix")).toHaveCount(0);
  const section = page
    .locator("section", { has: page.locator("#runtime-environments-title") })
    .last();
  await expect(section).toContainText("请先创建 Proton 运行环境");
  await section
    .getByRole("button", { name: "创建运行环境", exact: true })
    .click();
  const createDialog = page.getByRole("dialog", {
    name: "创建运行环境",
    exact: true,
  });
  await expect(createDialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(createDialog).toHaveCount(0);
  await expect(
    section.getByRole("button", { name: "创建运行环境", exact: true }),
  ).toBeFocused();
  await section
    .getByRole("button", { name: "创建运行环境", exact: true })
    .click();
  await page.locator("#runtime-environment-name").fill("稳定 A");
  await page.locator("#runtime-environment-proton").fill("/opt/Proton-A");
  // 创建表单中的目录选择按钮仍可访问.
  await expect(
    page.getByRole("button", { name: "浏览Proton 目录" }),
  ).toBeEnabled();
  const createButton = createDialog.getByRole("button", {
    name: "创建环境",
    exact: true,
  });
  await expect(createButton.locator("svg use")).toHaveAttribute(
    "href",
    "#i-plus",
  );
  expect((await createButton.boundingBox())!.x).toBeLessThan(
    (await createDialog
      .getByRole("button", { name: "取消创建", exact: true })
      .boundingBox())!.x,
  );
  await expect(createDialog).toHaveCSS("opacity", "1");
  await expect(createDialog).toHaveCSS("transform", "matrix(1, 0, 0, 1, 0, 0)");
  await page.screenshot({ path: "test-results/environment-create-dialog.png" });
  await createDialog
    .getByRole("button", { name: "创建环境", exact: true })
    .click();
  const select = page.locator("#runtime-environment-trigger");
  await expect(select).toHaveText("稳定 A · 当前 · 稳定");
  await expect(
    section
      .locator(".runtime-environments__actions")
      .first()
      .getByRole("button")
      .first(),
  ).toHaveText("创建环境或副本");
  await expect(section.locator(".field__hint").first()).toContainText(
    "绑定的游戏目录: ~/Games/Neverness To Everness",
  );
  await expect(
    section.getByRole("button", { name: "删除环境", exact: true }),
  ).toBeDisabled();
  await section
    .getByRole("button", { name: "创建环境或副本", exact: true })
    .click();
  await page.locator("#runtime-environment-name").fill("实验 B");
  await page.locator("#runtime-environment-proton").fill("/opt/Proton-B");
  await createDialog
    .getByRole("button", { name: "创建副本", exact: true })
    .click();
  await select.click();
  await page.getByRole("option", { name: "实验 B", exact: true }).click();
  await section.getByRole("button", { name: "切换到此环境" }).click();
  await expect(select).toHaveText("实验 B · 当前");
  const currentPrefix = await section.getByText(/^当前 prefix:/).innerText();
  await select.click();
  await page
    .getByRole("option", { name: "稳定 A · 稳定", exact: true })
    .click();
  await section
    .getByRole("button", { name: "创建环境或副本", exact: true })
    .click();
  await expect(createDialog).toContainText("源环境: 稳定 A");
  await expect(
    createDialog
      .getByRole("button", { name: "创建副本", exact: true })
      .locator("svg use"),
  ).toHaveAttribute("href", "#i-copy");
  await expect(page.locator("#runtime-environment-proton")).toHaveValue(
    "/opt/Proton-A",
  );
  await page.locator("#runtime-environment-name").fill("A 的副本");
  await createDialog
    .getByRole("button", { name: "创建副本", exact: true })
    .click();
  await expect(page.locator("#runtime-environment-name")).toHaveCount(0);
  await expect(section.getByText(/^当前 prefix:/)).toHaveText(currentPrefix);
  await select.click();
  await page.getByRole("option", { name: "A 的副本", exact: true }).click();
  const remove = section.getByRole("button", { name: "删除环境", exact: true });
  await expect(remove).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await remove.click();
  const dialog = page.getByRole("dialog", { name: "删除运行环境" });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("确定删除“A 的副本”的 prefix?");
  const dangerColor = await remove.evaluate(
    (element) => getComputedStyle(element).color,
  );
  await expect(
    dialog.getByRole("button", { name: "确定删除", exact: true }),
  ).toHaveCSS("color", dangerColor);
  await expect(dialog.getByRole("button", { name: "关闭弹窗" })).toBeFocused();
  const confirmButton = dialog.getByRole("button", {
    name: "确定删除",
    exact: true,
  });
  await expect(confirmButton.locator("svg use")).toHaveAttribute(
    "href",
    "#i-trash",
  );
  expect((await confirmButton.boundingBox())!.x).toBeLessThan(
    (await dialog
      .getByRole("button", { name: "取消", exact: true })
      .boundingBox())!.x,
  );
  await expect(dialog).toHaveCSS("opacity", "1");
  await expect(dialog).toHaveCSS("transform", "matrix(1, 0, 0, 1, 0, 0)");
  await page.screenshot({ path: "test-results/environment-delete-dialog.png" });
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(remove).toBeFocused();
  await remove.click();
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await remove.click();
  await dialog.getByRole("button", { name: "确定删除", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(select).toHaveText("实验 B · 当前");
  await select.click();
  await expect(
    page.getByRole("option", { name: "A 的副本", exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press("Escape");
  await section.getByRole("button", { name: "保留为稳定环境" }).click();
  await expect(select).toHaveText("实验 B · 当前 · 稳定");
  await page.screenshot({ path: "test-results/runtime-environments.png" });
  expect(errors).toEqual([]);
});
