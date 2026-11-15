import { expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

test("export and import connections from Settings", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  await page.getByRole("button", { name: "Settings" }).click();

  await page.getByRole("button", { name: "Export connections…" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Exported 1 connection." }),
  ).toBeVisible();

  await page.getByRole("button", { name: "Import connections…" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Imported 1 connection." }),
  ).toBeVisible();
  await expect(
    page.getByRole("alert").filter({ hasText: "Skipped 1 connection: Broken" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: /^Imported/ })).toBeVisible();
});
