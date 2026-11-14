import { expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

test("the command palette opens a table", async ({ page }) => {
  await addAndOpenConnection(page, "Palette");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByRole("combobox").fill("orders");
  await expect(palette.getByRole("option", { selected: true })).toContainText("orders");
  await page.keyboard.press("Enter");

  await expect(palette).toBeHidden();
  await expect(page.getByRole("status", { name: "Rows shown" })).toBeVisible();
});
