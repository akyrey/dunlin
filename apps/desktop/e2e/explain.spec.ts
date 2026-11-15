import { expect, test } from "@playwright/test";

import { addAndOpenConnection } from "./helpers";

test("explain shows the plan as a tree without running the statement", async ({ page }) => {
  await addAndOpenConnection(page, "People");
  const editor = page.getByLabel("SQL editor");
  await editor.click();
  await editor.pressSequentially("SELECT * FROM people p JOIN orders o ON o.person = p.id");

  await page.getByRole("button", { name: "Explain" }).click();

  const plan = page.getByRole("tree", { name: "Query plan" });
  await expect(plan).toBeVisible();
  await expect(plan.getByRole("treeitem").first()).toContainText("Hash Join");
  await expect(plan).toContainText("people p");
  await expect(plan).toContainText("Index Name: orders_person");
  await expect(page.getByRole("tab", { name: "Plan" })).toHaveAttribute("aria-selected", "true");
  // Nothing ran: there is no result sub-tab.
  await expect(page.getByRole("tab", { name: /Result 1/ })).toHaveCount(0);

  await editor.press("ControlOrMeta+Shift+E");
  await expect(plan).toBeVisible();
});
