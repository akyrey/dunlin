import { type Page, expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

async function typeStatement(page: Page, sql: string) {
  const editor = page.getByLabel("SQL editor");
  await editor.click();
  await editor.pressSequentially(sql);
}

test("explain analyze shows measured numbers and hints, and stages a suggested index", async ({
  page,
}) => {
  await addAndOpenConnection(page, "People");
  await typeStatement(page, "SELECT * FROM people WHERE balance = 5");

  await page.getByRole("button", { name: "Explain analyze" }).click();

  const plan = page.getByRole("tree", { name: "Query plan" });
  await expect(plan).toBeVisible();
  await expect(page.getByText("12.5 ms in all")).toBeVisible();
  await expect(plan.getByText("rows 120")).toBeVisible();
  const scan = plan.getByRole("treeitem").nth(1);
  await expect(scan.getByText("slowest")).toBeVisible();
  await expect(scan.getByText("full scan")).toBeVisible();

  await scan.getByRole("button", { name: "Create index on balance…" }).click();

  await expect(page.getByRole("heading", { name: "main.people" })).toBeVisible();
  await expect(page.getByText("1 change not applied")).toBeVisible();
  await page.getByRole("tab", { name: "Indexes" }).click();
  await expect(page.getByRole("row", { name: "Index people_balance_idx" })).toHaveAttribute(
    "data-state",
    "added",
  );
  await page.getByRole("button", { name: "Preview and apply" }).click();
  const dialog = page.getByRole("dialog", { name: "Apply schema changes?" });
  await expect(dialog.getByLabel("Statements to apply")).toContainText(
    'CREATE INDEX "people_balance_idx"',
  );
  await dialog.getByRole("button", { name: "Apply" }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("row", { name: "Index people_balance_idx" })).toBeVisible();
});

test("explain analyze of a dangerous statement asks first", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await typeStatement(page, "DELETE FROM people");

  await page.getByRole("button", { name: "Explain analyze" }).click();

  const dialog = page.getByRole("dialog", { name: "Run this statement to measure it?" });
  await expect(dialog).toContainText("DELETE without WHERE affects every row");
  await dialog.getByRole("button", { name: "Run and measure" }).click();
  await expect(page.getByRole("tree", { name: "Query plan" })).toBeVisible();
  await expect(page.getByText(/EXPLAIN ANALYZE DELETE FROM people/)).toBeVisible();
});
