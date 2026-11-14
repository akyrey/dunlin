import { expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

test("settings switch the theme and the NULL text", async ({ page }) => {
  await addAndOpenConnection(page, "Themed");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Settings" });
  await dialog.getByLabel("Theme").selectOption("light");
  await dialog.getByLabel("NULL shown as").fill("∅");
  await dialog.getByRole("button", { name: "Save" }).click();

  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  const background = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  // bg-zinc-950 is near-white in the light theme.
  expect(background).toMatch(/oklch\(0\.98|rgb\(25[0-5]/);

  await page
    .getByRole("tree", { name: "Schema tree" })
    .getByRole("button", { name: "people", exact: true })
    .click();
  await expect(page.getByRole("gridcell", { name: "∅" }).first()).toBeVisible();
});
