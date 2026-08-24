import type { Page } from "@playwright/test";

import { performAndAssertNewestActivity } from "./activity";
import { expect, test } from "./fixtures";

function operation(body: unknown): RequestInit {
  return {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  };
}

async function open(page: Page, baseURL: string): Promise<void> {
  await page.goto(baseURL);
  await expect(
    page.getByRole("region", { name: "Workspace status" }),
  ).toContainText("Server ready");
}

test("keeps the upstream token and private hostname out of browser-visible data", async ({
  page,
  e2e,
}) => {
  const renderedResponses: Array<Promise<string>> = [];
  page.on("response", (response) => {
    if (response.url().startsWith(e2e.baseURL)) {
      renderedResponses.push(
        response
          .text()
          .then((body) => `${JSON.stringify(response.headers())}\n${body}`),
      );
    }
  });
  await open(page, e2e.baseURL);
  await page.getByRole("treeitem", { name: "README.md" }).click();
  await expect(
    page.getByRole("textbox", { name: "File contents" }),
  ).toBeVisible();
  await expect.poll(() => renderedResponses.length).toBeGreaterThan(1);
  // Snapshot before awaiting: status polling intentionally keeps the page
  // active, so networkidle would never be a valid completion condition.
  const firstSnapshot = [...renderedResponses];
  await Promise.all(firstSnapshot);
  const completeRendered = await Promise.all([...renderedResponses]);
  const exposed = `${await page.locator("body").innerText()}\n${[
    ...firstSnapshot,
    ...completeRendered,
  ].join("\n")}`;
  expect(exposed).not.toContain(e2e.token);
  expect(exposed).not.toContain("fslite-server:8080");
  const logs = e2e.logs();
  expect(logs.rust.length).toBeLessThanOrEqual(24 * 1024);
  expect(logs.showcase.length).toBeLessThanOrEqual(24 * 1024);
  expect(`${logs.rust}\n${logs.showcase}`).not.toContain(e2e.token);
  expect(`${logs.rust}\n${logs.showcase}`).not.toContain("fslite-server:8080");
  const diagnostics = e2e.diagnostics();
  expect(diagnostics).toContain("Rust process diagnostics");
  expect(diagnostics).not.toContain(e2e.token);
  expect(diagnostics).not.toContain("fslite-server:8080");
});

test("rejects unknown operations and oversized uploads at the public boundary", async ({
  e2e,
}) => {
  const unknown = await e2e.request(
    "/api/operation",
    operation({ kind: "shell" }),
  );
  expect(unknown.status).toBe(400);
  const upload = await e2e.request("/api/upload?path=/too-large.bin", {
    method: "POST",
    headers: { "content-type": "application/octet-stream" },
    body: new Uint8Array(1024 * 1024 + 1),
  });
  expect(upload.status).toBe(413);
});

test("enforces the real 121-request read limit", async ({ e2e }) => {
  let response: Response | undefined;
  for (let request = 1; request <= 121; request += 1) {
    response = await e2e.request(
      "/api/operation",
      operation({ kind: "tree", path: "/" }),
    );
    expect(response.status, `request ${request}`).toBe(
      request === 121 ? 429 : 200,
    );
  }
});

test("enforces the real 31-request mutation limit", async ({ e2e }) => {
  for (let request = 1; request <= 31; request += 1) {
    const response = await e2e.request(
      "/api/operation",
      operation({
        kind: "mkdir",
        path: `/rate-mutation-${request}`,
        parents: false,
      }),
    );
    expect(response.status, `request ${request}`).toBe(
      request === 31 ? 429 : 200,
    );
  }
});

test("enforces the real 11-request upload limit", async ({ e2e }) => {
  for (let request = 1; request <= 11; request += 1) {
    const response = await e2e.request(
      `/api/upload?path=/rate-upload-${request}.txt`,
      {
        method: "POST",
        headers: { "content-type": "application/octet-stream" },
        body: "x",
      },
    );
    expect(response.status, `request ${request}`).toBe(
      request === 11 ? 429 : 200,
    );
  }
});

test("keeps 375px navigation functional without horizontal overflow", async ({
  page,
  e2e,
}) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await open(page, e2e.baseURL);
  const tree = page.getByRole("tree", { name: "Files" });
  const editor = page.getByRole("region", { name: "File editor" });
  const activity = page.getByRole("region", { name: "API activity" });
  const [treeBox, editorBox, activityBox] = await Promise.all([
    tree.boundingBox(),
    editor.boundingBox(),
    activity.boundingBox(),
  ]);
  expect(treeBox?.y).toBeLessThan(editorBox?.y ?? 0);
  expect(editorBox?.y).toBeLessThan(activityBox?.y ?? 0);
  await page.getByRole("button", { name: "New folder" }).click();
  await page.getByRole("textbox", { name: "Name" }).fill("narrow-proof");
  await performAndAssertNewestActivity(
    page,
    () => page.getByRole("button", { name: "Create folder" }).click(),
    "PUT",
    "fs/narrow-proof\\?type=directory",
  );
  await expect(tree).toContainText("narrow-proof");
  await page.getByRole("tab", { name: "Search" }).click();
  await expect(page.getByRole("tabpanel", { name: "Search" })).toBeVisible();
  await page.getByRole("tab", { name: "Trash" }).click();
  await expect(page.getByRole("tabpanel", { name: "Trash" })).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollWidth <=
        document.documentElement.clientWidth,
    ),
  ).toBe(true);
});
