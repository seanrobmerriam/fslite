import type { Page } from "@playwright/test";

import { SEED_ENTRIES } from "../src/lib/server/seed";
import { performAndAssertNewestActivity } from "./activity";
import { expect, test } from "./fixtures";

const seedReadme = SEED_ENTRIES.find(
  (entry): entry is (typeof SEED_ENTRIES)[number] & { text: string } =>
    entry.path === "/README.md",
)?.text;

if (seedReadme === undefined) {
  throw new Error("README seed entry is required by the reset regression");
}

function operation(body: unknown): RequestInit {
  return {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  };
}

async function openSeed(page: Page, baseURL: string): Promise<void> {
  await page.goto(baseURL);
  await expect(
    page.getByRole("region", { name: "Workspace status" }),
  ).toContainText("Server ready");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("treeitem", { name: "README.md" }).click(),
    "GET",
    "content/README.md",
  );
  await expect(
    page.getByRole("textbox", { name: "File contents" }),
  ).toHaveValue(seedReadme);
}

test.describe("reset lifecycle", () => {
  test.use({ resetIntervalMs: 5_000, resetResponseDelayMs: 3_000 });

  test("reseeds visibly, blocks mutations, and retains an editable local draft", async ({
    page,
    browser,
    e2e,
  }) => {
    await openSeed(page, e2e.baseURL);
    const marker = `reset-marker-${Date.now()}.txt`;
    await page.getByRole("button", { name: "New file" }).click();
    await page.getByRole("textbox", { name: "Name" }).fill(marker);
    await performAndAssertNewestActivity(
      page,
      () => page.getByRole("button", { name: "Create file" }).click(),
      "PUT",
      `content/${marker}`,
    );
    await expect(page.getByRole("tree", { name: "Files" })).toContainText(
      marker,
    );

    await performAndAssertNewestActivity(
      page,
      () => page.getByRole("treeitem", { name: "README.md" }).click(),
      "GET",
      "content/README.md",
    );
    const editor = page.getByRole("textbox", { name: "File contents" });
    const temporaryServerText = "temporary server text before reset";
    await editor.fill(temporaryServerText);
    await performAndAssertNewestActivity(
      page,
      () => page.getByRole("button", { name: "Save file" }).click(),
      "PUT",
      "content/README.md",
    );
    const localDraft = "local draft retained while reset";
    await editor.fill(localDraft);
    await expect(page.getByText("Unsaved changes")).toBeVisible();

    const before = (await (await e2e.request("/api/status")).json()) as {
      generation: number;
    };
    await expect
      .poll(
        async () => {
          const status = (await (await e2e.request("/api/status")).json()) as {
            resetting: boolean;
          };
          return status.resetting;
        },
        { intervals: [100, 200], timeout: 15_000 },
      )
      .toBe(true);

    await expect(
      page.getByRole("status", { name: "Workspace reset in progress" }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "New file" })).toBeDisabled();
    await expect(
      page.getByRole("button", { name: "Save file" }),
    ).toBeDisabled();
    const activities = page.locator(".api-activity .activity-list > li");
    const activityCount = await activities.count();
    await editor.fill(`${localDraft} and still editable`);
    await editor.press("ControlOrMeta+s");
    await expect(activities).toHaveCount(activityCount);
    await expect(editor).toHaveValue(`${localDraft} and still editable`);

    await expect
      .poll(
        async () => {
          const status = (await (await e2e.request("/api/status")).json()) as {
            generation: number;
          };
          return status.generation;
        },
        { intervals: [100, 200], timeout: 15_000 },
      )
      .toBeGreaterThan(before.generation);
    await expect(
      page.getByRole("status", { name: "Workspace reset in progress" }),
    ).toBeHidden();
    await expect(
      page.getByRole("button", { name: "Refresh files" }),
    ).toBeEnabled();
    await page.getByRole("button", { name: "Refresh files" }).click();
    await expect(page.getByRole("tree", { name: "Files" })).not.toContainText(
      marker,
    );

    const fresh = await browser.newPage();
    try {
      await openSeed(fresh, e2e.baseURL);
      await expect(
        fresh.getByRole("textbox", { name: "File contents" }),
      ).toHaveValue(seedReadme);
    } finally {
      await fresh.close();
    }
  });
});

test("recovers two stale-revision conflicts without replacing the local draft", async ({
  page,
  context,
  e2e,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"], {
    origin: e2e.baseURL,
  });
  await openSeed(page, e2e.baseURL);
  const editor = page.getByRole("textbox", { name: "File contents" });
  const localDraft = "my local revision";
  await editor.fill(localDraft);

  const firstServerText = "written by the second actor";
  expect(
    (
      await e2e.request(
        "/api/operation",
        operation({
          kind: "write_file",
          path: "/README.md",
          text: firstServerText,
        }),
      )
    ).status,
  ).toBe(200);
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Save file" }).click(),
    "PUT",
    "content/README.md",
    409,
  );
  const conflict = page.getByRole("alert", { name: "Revision conflict" });
  await expect(conflict).toContainText("changed on the shared workspace");
  await page.getByRole("button", { name: "Copy my unsaved text" }).click();
  await expect
    .poll(() => page.evaluate(() => navigator.clipboard.readText()))
    .toBe(localDraft);
  await expect(editor).toHaveValue(localDraft);

  const secondServerText = "newer text written by the second actor";
  expect(
    (
      await e2e.request(
        "/api/operation",
        operation({
          kind: "write_file",
          path: "/README.md",
          text: secondServerText,
        }),
      )
    ).status,
  ).toBe(200);
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Save file" }).click(),
    "PUT",
    "content/README.md",
    409,
  );
  await page.getByRole("button", { name: "Reload server version" }).click();
  await expect(editor).toHaveValue(secondServerText);
  await expect(conflict).toBeHidden();
});
