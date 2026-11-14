import { expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

test("expand the schema tree, open a table, see its columns and DDL", async ({ page }) => {
  await addAndOpenConnection(page, "People");

  const tree = page.getByRole("tree", { name: "Schema tree" });
  await expect(tree.getByRole("button", { name: /Tables/ })).toBeVisible();
  await tree.getByRole("button", { name: "people", exact: true }).click();

  await expect(page.getByRole("heading", { name: "main.people" })).toBeVisible();
  await expect(page.getByRole("cell", { name: "balance" })).toBeVisible();
  await expect(page.getByRole("cell", { name: "BIGINT" })).toBeVisible();
  await page.getByRole("tab", { name: "DDL" }).click();
  await expect(page.getByLabel("Table DDL")).toContainText("CREATE TABLE people");
});

test("generate a SELECT from the tree into the editor", async ({ page }) => {
  await addAndOpenConnection(page, "People");

  const tree = page.getByRole("tree", { name: "Schema tree" });
  await tree.getByRole("button", { name: "people", exact: true }).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Generate SELECT" }).click();

  await expect(page.getByLabel("SQL editor")).toContainText('FROM "main"."people";');
});
