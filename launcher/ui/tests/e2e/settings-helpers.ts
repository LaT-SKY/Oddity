import type { Page } from "@playwright/test";
import type { SettingsCategory } from "../../src/core/settingsCatalog";

export async function settingsCategory(page: Page, category: SettingsCategory) {
  await page.locator(`#page-settings [data-category="${category}"]`).click();
}
