import { type Page, expect, test } from "@playwright/test";

import { addAndOpenConnection, runInEditor } from "./helpers";

/** Screenshots for the README, against the mocked backend. Run with
 * `SCREENSHOTS=1 pnpm exec playwright test readme-screenshots`; skipped otherwise. */
test.skip(!process.env.SCREENSHOTS, "set SCREENSHOTS=1 to refresh the README screenshots");
test.use({ viewport: { width: 1280, height: 800 }, colorScheme: "dark" });

const shot = (page: Page, name: string) =>
  page.screenshot({ path: `../../docs/screenshots/${name}.png` });

test("editor and results", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await runInEditor(page, "SELECT * FROM people");
  await expect(page.getByRole("grid")).toBeVisible();
  await shot(page, "editor");
});

test("table data with a pending edit", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await page
    .getByRole("tree", { name: "Schema tree" })
    .getByRole("button", { name: "people", exact: true })
    .click();
  await page.getByRole("gridcell", { name: "Grace", exact: true }).dblclick();
  const editor = page.getByLabel("Edit name");
  await editor.fill("Grace Hopper");
  await editor.press("Enter");
  await page.getByRole("button", { name: "Apply changes" }).click();
  await expect(page.getByRole("dialog", { name: "Apply changes?" })).toBeVisible();
  await shot(page, "edit-preview");
});

test("new connection", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New connection" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Database type").selectOption("postgres");
  await dialog.getByLabel("Name").fill("Staging");
  await shot(page, "new-connection");
});
