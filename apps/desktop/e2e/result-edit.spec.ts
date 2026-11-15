import { expect, test } from "@playwright/test";

import { addAndOpenConnection, runInEditor } from "./helpers";

const cell = (page: import("@playwright/test").Page, text: string) =>
  page.getByRole("gridcell", { name: text, exact: true });

test("edit the rows of a single-table query, then the query runs again", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await runInEditor(page, "SELECT * FROM people");

  await cell(page, "Grace").dblclick();
  const editor = page.getByLabel("Edit name");
  await editor.fill("Grace Hopper");
  await editor.press("Enter");
  await expect(page.getByRole("status", { name: "Pending changes" })).toHaveText(/1 edited/);

  await page.getByRole("button", { name: "Apply changes" }).click();
  const preview = page.getByRole("dialog", { name: "Apply changes?" });
  await expect(preview.getByLabel("Statements to apply")).toContainText(
    "UPDATE people SET name = 'Grace Hopper' WHERE id = '2'",
  );
  await preview.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByText(/Applied 1 statement/)).toBeVisible();
  await expect(page.getByRole("status", { name: "Pending changes" })).toBeHidden();
});

test("running again asks before discarding pending edits", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await runInEditor(page, "SELECT * FROM people");
  await cell(page, "Ada").dblclick();
  await page.getByLabel("Edit name").fill("Ada L");
  await page.getByLabel("Edit name").press("Enter");

  await page.getByRole("button", { name: "Run", exact: true }).click();
  const ask = page.getByRole("dialog", { name: "Discard unapplied changes?" });
  await expect(ask).toBeVisible();
  await ask.getByRole("button", { name: "Cancel" }).click();
  await expect(cell(page, "Ada L")).toBeVisible();
});

test("a query that is not a plain single-table select stays read-only", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await runInEditor(page, "SELECT * FROM people WHERE id > 0");

  await expect(page.getByRole("grid")).toBeVisible();
  await expect(page.getByRole("toolbar", { name: "Edit rows" })).toBeHidden();
});
