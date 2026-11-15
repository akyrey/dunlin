import { expect, test } from "@playwright/test";

import { addAndOpenConnection, runInEditor } from "./helpers";

test("manual commit: a statement opens a transaction until commit or rollback", async ({
  page,
}) => {
  await addAndOpenConnection(page, "People");
  const manual = page.getByLabel("Manual commit");
  await manual.check();

  await runInEditor(page, "UPDATE people SET name = 'x' WHERE id = 1");
  const open = page.getByRole("status", { name: "Transaction" });
  await expect(open).toHaveText("Transaction open");
  await expect(manual).toBeDisabled();

  await page.getByRole("button", { name: "Rollback" }).click();
  await expect(open).toBeHidden();
  await expect(manual).toBeEnabled();

  await runInEditor(page, "\nSELECT * FROM people");
  await expect(open).toBeVisible();
  // Results stay read-only while the tab manages its own transaction.
  await expect(page.getByRole("toolbar", { name: "Edit rows" })).toBeHidden();
  await page.getByRole("button", { name: "Commit" }).click();
  await expect(open).toBeHidden();
});

test("closing a tab with an open transaction asks first", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await page.getByLabel("Manual commit").check();
  await runInEditor(page, "SELECT 1");

  await page.getByRole("button", { name: "Close Query 1" }).click();
  const ask = page.getByRole("dialog", { name: "Roll back and close?" });
  await expect(ask).toBeVisible();
  await ask.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("status", { name: "Transaction" })).toBeVisible();
});
