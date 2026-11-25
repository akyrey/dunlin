import { type Page, expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

async function openPeopleDetails(page: Page) {
  await addAndOpenConnection(page, "People");
  const tree = page.getByRole("tree", { name: "Schema tree" });
  await tree.getByRole("button", { name: "Actions for people" }).click();
  await page.getByRole("menuitem", { name: "Open details" }).click();
  await expect(page.getByRole("heading", { name: "main.people" })).toBeVisible();
  await page.getByRole("button", { name: "Edit" }).click();
}

async function addColumn(page: Page, name: string) {
  await page.getByRole("button", { name: "Add column" }).click();
  const row = page.getByRole("row", { name: /New column/ });
  await row.getByLabel("Name").fill(name);
  await row.getByLabel("Type").fill("TEXT");
}

test("add a column and apply it", async ({ page }) => {
  await openPeopleDetails(page);
  await addColumn(page, "email");
  await expect(page.getByText("1 change not applied")).toBeVisible();

  await page.getByRole("button", { name: "Preview and apply" }).click();
  const dialog = page.getByRole("dialog", { name: "Apply schema changes?" });
  await expect(dialog.getByLabel("Statements to apply")).toContainText('ADD COLUMN "email" TEXT');
  await dialog.getByRole("button", { name: "Apply" }).click();

  await expect(dialog).toBeHidden();
  await expect(page.getByRole("toolbar", { name: "Schema changes" })).toBeHidden();
  await page.getByRole("button", { name: "Done editing" }).click();
  await expect(page.getByRole("cell", { name: "email" })).toBeVisible();
});

test("drop an index after confirming the drop", async ({ page }) => {
  await openPeopleDetails(page);
  await page.getByRole("tab", { name: "Indexes" }).click();
  const row = page.getByRole("row", { name: "Index people_name_idx" });
  await row.getByRole("button", { name: "Drop" }).click();
  await expect(row).toHaveAttribute("data-state", "dropped");

  await page.getByRole("button", { name: "Preview and apply" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText(/drop columns, keys, indexes/)).toBeVisible();
  await dialog.getByRole("button", { name: "Apply and drop" }).click();

  await expect(dialog).toBeHidden();
  await expect(page.getByRole("row", { name: "Index people_name_idx" })).toBeHidden();
});

test("a failed apply keeps the pending changes", async ({ page }) => {
  await openPeopleDetails(page);
  await addColumn(page, "broken");
  await page.getByRole("button", { name: "Preview and apply" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Apply" }).click();

  const alert = page.getByRole("alert");
  await expect(alert).toContainText("syntax error");
  await expect(alert).toContainText('ADD COLUMN "broken"');
  await expect(page.getByText("1 change not applied")).toBeVisible();
  await expect(page.getByRole("row", { name: /New column broken/ })).toBeVisible();
});

test("a read-only connection offers no editing", async ({ page }) => {
  await addAndOpenConnection(page, "People", true);
  const tree = page.getByRole("tree", { name: "Schema tree" });
  await tree.getByRole("button", { name: "Actions for people" }).click();
  await page.getByRole("menuitem", { name: "Open details" }).click();
  await expect(page.getByRole("heading", { name: "main.people" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Edit" })).toBeHidden();
});
