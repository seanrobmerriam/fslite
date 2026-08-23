import type { Locator, Page } from "@playwright/test";

import { expect } from "./fixtures";

const requestId =
  /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i;

/**
 * Attributes precisely one visible visitor action to precisely one new activity
 * item. Reconciliation tree reads must not add a second record.
 */
export async function performAndAssertNewestActivity(
  page: Page,
  perform: () => Promise<unknown>,
  method: string,
  path: string | RegExp,
  status = 200,
): Promise<Locator> {
  const records = page.locator(".api-activity .activity-list > li");
  const before = await records.count();
  await perform();
  await expect(records).toHaveCount(before + 1);
  const newest = records.nth(before);
  const pathSource = typeof path === "string" ? path : path.source;
  const pathFlags = typeof path === "string" ? "" : path.flags;
  await expect(newest).toContainText(
    new RegExp(`${method} .*${pathSource}.*${status}`, pathFlags),
  );
  await expect(newest).toContainText(requestId);
  return newest;
}
