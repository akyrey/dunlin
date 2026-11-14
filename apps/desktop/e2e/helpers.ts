import { type Page, expect } from "@playwright/test";

export async function addAndOpenConnection(page: Page, name: string) {
  await page.goto("/");
  await page.getByRole("button", { name: "New connection" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Database type").selectOption("sqlite");
  await dialog.getByLabel("Name").fill(name);
  await dialog.getByRole("button", { name: "Browse…" }).click();
  await expect(dialog.getByLabel("Database file")).toHaveValue("/mock/people.db");
  await dialog.getByRole("button", { name: "Test connection" }).click();
  await expect(dialog.getByText("Connected: SQLite 3 (mock)")).toBeVisible();
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();
  await page.getByRole("button", { name: new RegExp(`^${name}`) }).click();
}

export async function runInEditor(page: Page, sql: string) {
  const editor = page.getByLabel("SQL editor");
  await editor.click();
  await editor.pressSequentially(sql);
  await page.keyboard.press("ControlOrMeta+Enter");
}
