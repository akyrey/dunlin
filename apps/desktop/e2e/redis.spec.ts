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

test("edit, create and delete Redis keys after previewing the commands", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New connection" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Database type").selectOption("redis");
  await dialog.getByLabel("Name").fill("Cache");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();
  await page.getByRole("button", { name: /^Cache/ }).click();
  const keys = page.getByRole("list", { name: "Keys" });

  // Selecting a row fills the editor; the change runs only after the preview.
  await keys.getByRole("button", { name: /user:1/ }).click();
  await page.getByRole("gridcell", { name: "Ada" }).click();
  const editor = page.getByRole("form", { name: "Edit value" });
  await expect(editor.getByLabel("Field")).toHaveValue("name");
  await editor.getByLabel("Value").fill("Grace");
  await editor.getByRole("button", { name: "Set field" }).click();
  const preview = page.getByRole("dialog", { name: "Run these commands?" });
  await expect(preview.getByLabel("Commands")).toHaveText("HSET user:1 name Grace");
  await preview.getByRole("button", { name: "Apply" }).click();
  await expect(preview).toBeHidden();
  await expect(page.getByRole("gridcell", { name: "Grace" })).toBeVisible();

  // A new key opens in its own tab.
  await page.getByRole("button", { name: "New key" }).click();
  const create = page.getByRole("dialog", { name: "New key" });
  await create.getByLabel("Key name").fill("greeting");
  await create.getByLabel("Value").fill("hello");
  await create.getByRole("button", { name: "Review" }).click();
  await expect(preview.getByLabel("Commands")).toHaveText("SET greeting hello KEEPTTL");
  await preview.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByRole("tab", { name: /greeting/ })).toBeVisible();
  await expect(page.getByRole("gridcell", { name: "hello" })).toBeVisible();

  // An existing name is refused.
  await page.getByRole("button", { name: "New key" }).click();
  await create.getByLabel("Key name").fill("user:1");
  await create.getByRole("button", { name: "Review" }).click();
  await expect(create.getByRole("alert")).toHaveText(/already exists/);
  await create.getByRole("button", { name: "Cancel" }).click();

  // Deleting closes the tab and drops the key from the list.
  await page.getByRole("button", { name: "Delete key" }).click();
  await expect(preview.getByLabel("Commands")).toHaveText("DEL greeting");
  await preview.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByRole("tab", { name: /greeting/ })).toHaveCount(0);
  await expect(keys.getByRole("button", { name: /greeting/ })).toHaveCount(0);
});
