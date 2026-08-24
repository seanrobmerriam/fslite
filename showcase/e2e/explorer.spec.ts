import type { Page } from "@playwright/test";

import { performAndAssertNewestActivity } from "./activity";
import { expect, test } from "./fixtures";

async function openWorkspace(page: Page, baseURL: string): Promise<void> {
  await page.goto(baseURL);
  await expect(
    page.getByRole("region", { name: "Workspace status" }),
  ).toContainText("Server ready");
  await expect(page.getByRole("tree", { name: "Files" })).toContainText(
    "README.md",
  );
}

async function chooseAction(
  page: Page,
  name: string,
  action: string,
): Promise<void> {
  await page.getByRole("button", { name: `Actions for ${name}` }).click();
  await page.getByRole("menuitem", { name: action, exact: true }).click();
}

test("loads the seeded workspace through the standalone gateway", async ({
  page,
  e2e,
}) => {
  const status = await e2e.request("/api/status");
  expect(status.status, await status.text()).toBe(200);
  await openWorkspace(page, e2e.baseURL);
});

test("records one visitor mutation while reconciliation adds no activity", async ({
  page,
  e2e,
}) => {
  await openWorkspace(page, e2e.baseURL);
  await page.getByRole("button", { name: "New folder" }).click();
  await page.getByRole("textbox", { name: "Name" }).fill("one-record");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Create folder" }).click(),
    "PUT",
    "fs/one-record\\?type=directory",
  );
});

test("rejects a stale permanent removal with a visible 409 conflict", async ({
  page,
  e2e,
}) => {
  await openWorkspace(page, e2e.baseURL);
  const secondActor = await e2e.request("/api/operation", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      kind: "write_file",
      path: "/README.md",
      text: "updated by another visitor",
    }),
  });
  expect(secondActor.status, await secondActor.text()).toBe(200);

  await chooseAction(page, "README.md", "Delete permanently");
  await page.getByRole("radio", { name: "Delete permanently" }).check();
  await page
    .getByRole("textbox", { name: "Confirm full path" })
    .fill("/README.md");
  await performAndAssertNewestActivity(
    page,
    () =>
      page.getByRole("button", { name: "Delete permanently" }).last().click(),
    "DELETE",
    "fs/README.md",
    409,
  );
  await expect(page.getByRole("dialog", { name: "Delete item" })).toContainText(
    /revision|changed/i,
  );
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("tree", { name: "Files" })).toContainText(
    "README.md",
  );
});

test("completes the visible filesystem journey with gateway activity", async ({
  page,
  e2e,
}) => {
  await openWorkspace(page, e2e.baseURL);

  await page.getByRole("button", { name: "New folder" }).click();
  await page.getByRole("textbox", { name: "Name" }).fill("journey");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Create folder" }).click(),
    "PUT",
    "fs/journey\\?type=directory",
  );
  await expect(page.getByRole("tree")).toContainText("journey");

  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("treeitem", { name: "README.md" }).click(),
    "GET",
    "content/README.md",
  );
  await page.getByRole("button", { name: "New file" }).click();
  await page.getByRole("textbox", { name: "Name" }).fill("draft.txt");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Create file" }).click(),
    "PUT",
    "content/draft.txt",
  );
  await expect(page.getByRole("tree")).toContainText("draft.txt");

  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("treeitem", { name: "draft.txt" }).click(),
    "GET",
    "content/draft.txt",
  );
  await page
    .getByRole("textbox", { name: "File contents" })
    .fill("visible journey text");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Save file" }).click(),
    "PUT",
    "content/draft.txt",
  );

  await performAndAssertNewestActivity(
    page,
    async () => {
      const download = page.waitForEvent("download");
      await page.getByRole("button", { name: "Download file" }).click();
      expect((await download).suggestedFilename()).toBe("draft.txt");
    },
    "GET",
    "content/draft.txt",
  );

  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("treeitem", { name: "README.md" }).click(),
    "GET",
    "content/README.md",
  );
  await page.getByRole("button", { name: "Upload" }).click();
  await page.locator('input[type="file"]').setInputFiles({
    name: "upload.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("uploaded"),
  });
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Upload file" }).click(),
    "PUT",
    "content/upload.txt",
  );
  await expect(page.getByRole("tree")).toContainText("upload.txt");

  await chooseAction(page, "draft.txt", "Rename");
  await page.getByRole("textbox", { name: "Name" }).fill("renamed.txt");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Rename" }).click(),
    "POST",
    "fs/draft.txt\\?action=move",
  );
  await expect(page.getByRole("tree")).toContainText("renamed.txt");

  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("treeitem", { name: "README.md" }).click(),
    "GET",
    "content/README.md",
  );
  await page.getByRole("button", { name: "New folder" }).click();
  await page.getByRole("textbox", { name: "Name" }).fill("archive");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Create folder" }).click(),
    "PUT",
    "fs/archive\\?type=directory",
  );
  await chooseAction(page, "renamed.txt", "Move");
  await page
    .getByRole("textbox", { name: "Destination" })
    .fill("/archive/renamed.txt");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Move" }).click(),
    "POST",
    "fs/renamed.txt\\?action=move",
  );

  await page.getByRole("treeitem", { name: "archive" }).press("ArrowRight");
  await chooseAction(page, "renamed.txt", "Copy");
  await page
    .getByRole("textbox", { name: "Destination" })
    .fill("/archive/copy.txt");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Copy" }).click(),
    "POST",
    "fs/archive/renamed.txt\\?action=copy",
  );
  await expect(page.getByRole("tree")).toContainText("copy.txt");

  await page.getByRole("tab", { name: "Search" }).click();
  await page.getByRole("textbox", { name: "Search text" }).fill("copy");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Search" }).click(),
    "POST",
    "search/find",
  );
  await expect(
    page.getByRole("region", { name: "Search files" }),
  ).toContainText("copy.txt");
  await page.getByRole("radio", { name: "Contents" }).check();
  await page
    .getByRole("textbox", { name: "Search text" })
    .fill("visible journey text");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Search" }).click(),
    "POST",
    "search/content",
  );
  await expect(
    page.getByRole("region", { name: "Search files" }),
  ).toContainText("renamed.txt");

  await page.getByRole("tab", { name: "Explorer" }).click();
  await page.getByRole("treeitem", { name: "archive" }).press("ArrowRight");
  await chooseAction(page, "copy.txt", "Move to trash");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Move to trash" }).click(),
    "POST",
    "fs/archive/copy.txt\\?action=trash",
  );
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("tab", { name: "Trash" }).click(),
    "GET",
    "trash",
  );
  await expect(page.getByRole("region", { name: "Trash" })).toContainText(
    "copy.txt",
  );
  await page.getByRole("button", { name: "Restore copy.txt" }).click();
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Confirm restore" }).click(),
    "POST",
    "trash/.*/restore",
  );

  await page.getByRole("tab", { name: "Explorer" }).click();
  await page.getByRole("treeitem", { name: "archive" }).press("ArrowRight");
  await chooseAction(page, "copy.txt", "Delete permanently");
  await page.getByRole("radio", { name: "Delete permanently" }).check();
  await page
    .getByRole("textbox", { name: "Confirm full path" })
    .fill("/archive/copy.txt");
  await performAndAssertNewestActivity(
    page,
    () =>
      page.getByRole("button", { name: "Delete permanently" }).last().click(),
    "DELETE",
    "fs/archive/copy.txt",
  );

  await page.getByRole("treeitem", { name: "archive" }).press("ArrowRight");
  await chooseAction(page, "renamed.txt", "Move to trash");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Move to trash" }).click(),
    "POST",
    "fs/archive/renamed.txt\\?action=trash",
  );
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("tab", { name: "Trash" }).click(),
    "GET",
    "trash",
  );
  await page.getByRole("button", { name: "Purge renamed.txt" }).click();
  await page.getByRole("textbox", { name: "Confirm name" }).fill("renamed.txt");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Purge permanently" }).click(),
    "DELETE",
    "trash/",
  );

  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("tab", { name: "Changes" }).click(),
    "GET",
    "changes",
  );
  await expect(page.getByRole("region", { name: "Changes" })).toContainText(
    "created",
  );
});
