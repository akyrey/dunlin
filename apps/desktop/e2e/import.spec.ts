import { expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

test("import a CSV file into a table after mapping its columns", async ({ page }) => {
  await addAndOpenConnection(page, "Data");
  await page
    .getByRole("tree", { name: "Schema tree" })
    .getByRole("button", { name: "people", exact: true })
    .click();
  await expect(page.getByRole("status", { name: "Rows shown" })).toHaveText(/Rows 1–/);

  await page.getByRole("button", { name: "Import CSV…" }).click();
  const dialog = page.getByRole("dialog", { name: "Import CSV into people" });
  await expect(dialog.getByText("new-people.csv")).toBeVisible();
  // Headers match the columns by name, whatever their case.
  await expect(dialog.getByLabel("Source for id")).toHaveValue("1");
  await expect(dialog.getByLabel("Source for balance")).toHaveValue("2");
  await expect(dialog.getByRole("table", { name: "Column mapping" })).toContainText("5, NULL");

  await dialog.getByRole("button", { name: "Import", exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByText("Imported 2 rows into people")).toBeVisible();
  await expect(page.getByRole("gridcell", { name: "Yan" })).toBeVisible();
});
