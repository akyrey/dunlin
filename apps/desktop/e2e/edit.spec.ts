import { type Page, expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

async function openTable(page: Page, name: string, readOnly = false) {
  await addAndOpenConnection(page, "Data", readOnly);
  await page
    .getByRole("tree", { name: "Schema tree" })
    .getByRole("button", { name, exact: true })
    .click();
  await expect(page.getByRole("grid")).toBeVisible();
}

const cell = (page: Page, text: string) => page.getByRole("gridcell", { name: text, exact: true });

test("edit a cell, preview the SQL and apply", async ({ page }) => {
  await openTable(page, "people");

  await cell(page, "Grace").dblclick();
  const editor = page.getByLabel("Edit name");
  await editor.fill("Grace Hopper");
  await editor.press("Enter");

  await expect(page.getByRole("status", { name: "Pending changes" })).toHaveText(/1 edited/);
  await expect(cell(page, "Grace Hopper")).toHaveAttribute("data-edited", "true");
  await page.getByRole("button", { name: "Apply changes" }).click();
  const preview = page.getByRole("dialog", { name: "Apply changes?" });
  await expect(preview.getByLabel("Statements to apply")).toContainText(
    "UPDATE people SET name = 'Grace Hopper' WHERE id = '2'",
  );
  await expect(preview).toContainText("on Data");
  await preview.getByRole("button", { name: "Apply" }).click();

  await expect(page.getByText(/Applied 1 statement/)).toBeVisible();
  await expect(cell(page, "Grace Hopper")).not.toHaveAttribute("data-edited");
});

test("insert and delete rows in one batch", async ({ page }) => {
  await openTable(page, "people");

  await page.getByRole("button", { name: "Add row" }).click();
  const added = page.locator('[role="row"][data-state="inserted"]');
  await added.locator('[aria-colindex="2"]').dblclick();
  await page.getByLabel("Edit id").fill("4");
  await page.getByLabel("Edit id").press("Tab");
  await page.getByLabel("Edit name").fill("Margaret");
  await page.getByLabel("Edit name").press("Enter");
  await cell(page, "Ada").click();
  await page.getByRole("button", { name: "Delete rows" }).click();

  await expect(page.getByRole("status", { name: "Pending changes" })).toHaveText(
    /1 new · 1 deleted/,
  );
  await expect(page.locator('[role="row"][data-state="deleted"]')).toHaveCount(1);
  await page.getByRole("button", { name: "Apply changes" }).click();
  await page
    .getByRole("dialog", { name: "Apply changes?" })
    .getByRole("button", { name: "Apply" })
    .click();

  await expect(page.getByText(/Applied 2 statements\. 1 new row/)).toBeVisible();
  await expect(cell(page, "Margaret")).toBeVisible();
  await expect(cell(page, "Ada")).toHaveCount(0);
});

test("a failed apply keeps the pending changes and marks the row", async ({ page }) => {
  await openTable(page, "people");

  await page.getByRole("button", { name: "Add row" }).click();
  await page.locator('[role="row"][data-state="inserted"] [aria-colindex="2"]').dblclick();
  await page.getByLabel("Edit id").fill("1");
  await page.getByLabel("Edit id").press("Enter");
  await page.getByRole("button", { name: "Apply changes" }).click();
  await page
    .getByRole("dialog", { name: "Apply changes?" })
    .getByRole("button", { name: "Apply" })
    .click();

  await expect(page.getByRole("alert")).toContainText("Nothing was applied: duplicate key");
  await expect(page.getByRole("status", { name: "Pending changes" })).toHaveText(/1 new/);
  await expect(page.locator('[role="row"][data-state="failed"]')).toHaveCount(1);
});

test("views and read-only connections show no editors", async ({ page }) => {
  await openTable(page, "rich_people");
  await expect(page.getByRole("note")).toHaveText("Views cannot be edited.");
  await expect(page.getByRole("toolbar", { name: "Edit rows" })).toHaveCount(0);

  await openTable(page, "people", true);
  await expect(page.getByRole("note")).toHaveText(/Read-only connection/);
  await cell(page, "Grace").dblclick();
  await expect(page.getByLabel("Edit name")).toHaveCount(0);
});

test("changing the sort with pending changes asks first", async ({ page }) => {
  await openTable(page, "people");
  await cell(page, "Grace").dblclick();
  await page.getByLabel("Edit name").fill("G");
  await page.getByLabel("Edit name").press("Enter");

  await page.getByRole("button", { name: "Sort by name" }).click();
  const dialog = page.getByRole("dialog", { name: "Unapplied changes" });
  await expect(dialog).toContainText("to change the sort");
  await dialog.getByRole("button", { name: "Discard" }).click();

  await expect(page.getByRole("columnheader", { name: "name" })).toHaveAttribute(
    "aria-sort",
    "ascending",
  );
  await expect(cell(page, "Grace")).toBeVisible();
});
