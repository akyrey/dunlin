import { type Page, expect, test } from "@playwright/test";

async function addAndOpenConnection(page: Page, name: string) {
  await page.goto("/");
  await page.getByRole("button", { name: "New connection" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Name").fill(name);
  await dialog.getByRole("button", { name: "Browse…" }).click();
  await expect(dialog.getByLabel("Database file")).toHaveValue("/mock/people.db");
  await dialog.getByRole("button", { name: "Test connection" }).click();
  await expect(dialog.getByText("Connected: SQLite 3 (mock)")).toBeVisible();
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();
  await page.getByRole("button", { name: new RegExp(`^${name}`) }).click();
}

async function runInEditor(page: Page, sql: string) {
  const editor = page.getByLabel("SQL editor");
  await editor.click();
  await editor.pressSequentially(sql);
  await page.keyboard.press("ControlOrMeta+Enter");
}

test("add a connection, open it, run a query and see rows", async ({ page }) => {
  await addAndOpenConnection(page, "People");

  await runInEditor(page, "SELECT * FROM people;");

  const grid = page.getByRole("grid");
  await expect(grid.getByRole("gridcell", { name: "Ada" })).toBeVisible();
  await expect(grid.getByRole("gridcell", { name: "9007199254740993" })).toBeVisible();
  await expect(grid.getByRole("gridcell", { name: "NULL" })).toBeVisible();
  await expect(page.getByText("3 rows · 3 ms")).toBeVisible();
});

test("a result over the row limit is truncated and marked", async ({ page }) => {
  await addAndOpenConnection(page, "Big");

  await runInEditor(page, "SELECT * FROM big");

  await expect(page.getByText("Showing the first 1000 rows")).toBeVisible();
  await expect(page.getByText("1000+ rows")).toBeVisible();
});

test("a syntax error shows the server message and its location", async ({ page }) => {
  await addAndOpenConnection(page, "People");

  await runInEditor(page, "SELECT * FRM people");

  await expect(page.getByRole("alert")).toContainText('near "FRM": syntax error');
  await expect(page.getByRole("button", { name: "line 1, column 10" })).toBeVisible();
});
