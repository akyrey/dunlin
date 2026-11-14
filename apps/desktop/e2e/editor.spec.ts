import { expect, test } from "@playwright/test";

import { addAndOpenConnection, runInEditor } from "./helpers";

test("autocomplete offers table names and a table's columns", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  const editor = page.getByLabel("SQL editor");
  await editor.click();

  await editor.pressSequentially("SELECT * FROM peo");
  const options = page.getByRole("listbox");
  await expect(options.getByRole("option", { name: /^people/ })).toBeVisible();

  await page.keyboard.press("Escape");
  await editor.pressSequentially(" WHERE people.");
  await expect(options.getByRole("option", { name: /^balance/ })).toBeVisible();
});

test("run executes only the statement under the cursor", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  const editor = page.getByLabel("SQL editor");
  await editor.click();
  await editor.pressSequentially("SELECT 1;\nSELECT * FROM people;\nSELECT 3;");
  await page.keyboard.press("ArrowUp");

  await page.keyboard.press("ControlOrMeta+Enter");

  await expect(page.getByRole("grid").getByRole("gridcell", { name: "Ada" })).toBeVisible();
  const results = page.getByRole("tablist", { name: "Results" });
  await expect(results.getByRole("tab")).toHaveText(["Result 1", "Messages"]);
});

test("a script stops at the failing statement", async ({ page }) => {
  await addAndOpenConnection(page, "People");

  await runInEditor(page, "SELECT 1; SELECT * FRM people; SELECT 3");
  await page.getByRole("button", { name: "Run script" }).click();

  await expect(page.getByRole("alert")).toContainText('near "FRM": syntax error');
  const results = page.getByRole("tablist", { name: "Results" });
  await expect(results.getByRole("tab")).toHaveText([
    "Result 1",
    "Result 2 ✕",
    "Result 3 –",
    "Messages",
  ]);
});

test("a dangerous statement asks first, naming the connection", async ({ page }) => {
  await addAndOpenConnection(page, "People");

  await runInEditor(page, "DELETE FROM people");

  const dialog = page.getByRole("dialog", { name: "Run a dangerous statement?" });
  await expect(dialog).toContainText("On People");
  await expect(dialog).toContainText("DELETE without WHERE affects every row");
  await dialog.getByRole("button", { name: "Run anyway" }).click();
  await expect(page.getByText(/^2 rows affected/)).toBeVisible();
});

test("tabs and history", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await runInEditor(page, "SELECT * FROM people");
  await expect(page.getByRole("gridcell", { name: "Ada" })).toBeVisible();

  await page.getByRole("button", { name: "History" }).click();
  const history = page.getByRole("complementary", { name: "History" });
  await history.getByRole("button", { name: /SELECT \* FROM people/ }).dblclick();

  const tabs = page.getByRole("tablist", { name: "Open tabs" });
  await expect(tabs.getByRole("tab")).toHaveText(["Query 1", "Query 2"]);
  await expect(page.getByLabel("SQL editor")).toHaveText("SELECT * FROM people");
});
