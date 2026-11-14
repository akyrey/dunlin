import { type Page, expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

async function openTable(page: Page, name: string) {
  await addAndOpenConnection(page, "Data");
  await page
    .getByRole("tree", { name: "Schema tree" })
    .getByRole("button", { name, exact: true })
    .click();
}

const firstColumn = (page: Page) =>
  page.getByRole("grid").getByRole("row").locator('[role="gridcell"][aria-colindex="2"]');

test("open a table, sort, filter and page", async ({ page }) => {
  await openTable(page, "events");
  await expect(page.getByRole("status", { name: "Rows shown" })).toHaveText(
    /Rows 1–200 of ~450 rows/,
  );

  const idHeader = page.getByRole("button", { name: "Sort by id" });
  await idHeader.click();
  await idHeader.click();
  await expect(page.getByRole("columnheader", { name: "id" })).toHaveAttribute(
    "aria-sort",
    "descending",
  );
  await expect(firstColumn(page).first()).toHaveText("450");

  await page.getByRole("button", { name: "Next page" }).click();
  await expect(page.getByRole("status", { name: "Rows shown" })).toHaveText(/Rows 201–400/);

  await page.getByLabel("Filter", { exact: true }).fill("kind = 'login'");
  await page.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByRole("status", { name: "Rows shown" })).toHaveText("Rows 1–150");
  await page.getByRole("button", { name: "Count exactly" }).click();
  await expect(page.getByRole("status", { name: "Rows shown" })).toHaveText(/of 150 rows/);

  await page.getByLabel("Filter", { exact: true }).fill("kind FRM x");
  await page.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByRole("alert")).toContainText("syntax error");
  await expect(firstColumn(page).first()).toBeVisible();
});

test("open a JSON cell in the viewer and toggle the JSON view", async ({ page }) => {
  await openTable(page, "documents");
  await page.getByRole("gridcell", { name: /Quarterly report/ }).dblclick();

  const viewer = page.getByRole("complementary", { name: "Cell viewer" });
  await expect(viewer).toContainText("Showing the start of a long value");
  await viewer.getByRole("button", { name: "Load the whole value" }).click();
  await expect(viewer.getByLabel("Cell value")).toContainText('"title": "Quarterly report"');

  await page.getByRole("gridcell", { name: /broken/ }).click();
  await expect(viewer).toContainText("Not valid JSON");

  await page.getByRole("radio", { name: "JSON" }).click();
  await expect(page.getByLabel("Rows as JSON")).toContainText('"id": 1');
});

test("big integers keep every digit in the JSON view; NULL and empty text differ", async ({
  page,
}) => {
  await openTable(page, "people");

  const cells = page.getByRole("gridcell");
  await expect(cells.filter({ hasText: /^NULL$/ })).toHaveClass(/./);
  await page.getByRole("radio", { name: "JSON" }).click();
  await expect(page.getByLabel("Rows as JSON")).toContainText('"balance": "9007199254740993"');
  await expect(page.getByLabel("Rows as JSON")).toContainText('"balance": null');
});

test("a foreign key leads to the referenced row", async ({ page }) => {
  await openTable(page, "orders");

  await page.getByRole("button", { name: "Open the main.people row" }).last().click();

  await expect(page.getByText("id = 2")).toBeVisible();
  await expect(page.getByRole("gridcell", { name: "Grace" })).toBeVisible();
  await expect(page.getByRole("gridcell", { name: "Ada" })).toHaveCount(0);
});
