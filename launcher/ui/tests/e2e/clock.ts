import type { Page } from "@playwright/test";

/**
 * 安装假时钟并暂停。
 *
 * install 与 pauseAt 之间假时钟仍会前进, 直接以 new Date() 为目标可能落到
 * 当前时钟之前; 留出余量以避免 "Cannot fast-forward to the past"。
 */
export async function pauseClock(page: Page): Promise<void> {
  const now = Date.now();
  await page.clock.install({ time: now });
  await page.clock.pauseAt(now + 250);
}
