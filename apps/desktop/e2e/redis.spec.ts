import { expect, test } from "@playwright/test";

test("browse Redis keys and read their values", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New connection" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Database type").selectOption("redis");
  await dialog.getByLabel("Name").fill("Cache");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();
  await page.getByRole("button", { name: /^Cache/ }).click();

  const keys = page.getByRole("list", { name: "Keys" });
  await expect(keys.getByRole("button")).toHaveCount(2);
  await page.getByRole("button", { name: /Load more/ }).click();
  await expect(keys.getByRole("button")).toHaveCount(3);
  await expect(page.getByRole("button", { name: /Load more/ })).toBeHidden();

  await page.getByLabel("Key pattern").fill("session:*");
  await page.getByRole("button", { name: "Scan" }).click();
  await expect(keys.getByRole("button")).toHaveCount(1);
  await keys.getByRole("button", { name: /session:abc/ }).click();
  await expect(page.getByText("TTL 90 s")).toBeVisible();
  await expect(page.getByRole("gridcell", { name: "token" })).toBeVisible();

  await page.getByLabel("Key pattern").fill("user:*");
  await page.getByRole("button", { name: "Scan" }).click();
  await keys.getByRole("button", { name: /user:1/ }).click();
  await expect(page.getByRole("gridcell", { name: "Ada" })).toBeVisible();
  await expect(page.getByRole("tab", { name: /user:1/ })).toBeVisible();

  // No schema tree, Explain or manual transactions for a key-value store.
  await expect(page.getByRole("tree", { name: "Schema tree" })).toHaveCount(0);
  await page.getByRole("tab", { name: /Query 1/ }).click();
  await expect(page.getByRole("button", { name: "Explain" })).toHaveCount(0);
  await expect(page.getByLabel("Manual commit")).toHaveCount(0);
});
