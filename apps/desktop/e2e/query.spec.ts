import { expect, test } from "@playwright/test";

import { addAndOpenConnection, runInEditor } from "./helpers";

test("add a connection, open it, run a query and see rows", async ({ page }) => {
  await addAndOpenConnection(page, "People");

  await runInEditor(page, "SELECT * FROM people;");

  const grid = page.getByRole("grid");
  await expect(grid.getByRole("gridcell", { name: "Ada" })).toBeVisible();
  await expect(grid.getByRole("gridcell", { name: "9007199254740993" })).toBeVisible();
  await expect(grid.getByRole("gridcell", { name: "NULL" })).toBeVisible();
  await expect(page.getByText("3 rows · 3 ms")).toBeVisible();
});

test("a result over the row limit is truncated and marked", async ({ page }) => {
  await addAndOpenConnection(page, "Big");

  await runInEditor(page, "SELECT * FROM big");

  await expect(page.getByText("Showing the first 1000 rows")).toBeVisible();
  await expect(page.getByText("1000+ rows")).toBeVisible();
});

test("a syntax error shows the server message and its location", async ({ page }) => {
  await addAndOpenConnection(page, "People");

  await runInEditor(page, "SELECT * FRM people");

  await expect(page.getByRole("alert")).toContainText('near "FRM": syntax error');
  await expect(page.getByRole("button", { name: "line 1, column 10" })).toBeVisible();
});

test("a long query can be cancelled and the tab runs the next one", async ({ page }) => {
  await addAndOpenConnection(page, "Slow");

  await runInEditor(page, "SELECT pg_sleep(30)");
  await expect(page.getByRole("timer")).toBeVisible();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("alert")).toContainText("canceling statement");

  await page.getByLabel("SQL editor").press("ControlOrMeta+a");
  await page.keyboard.type("SELECT * FROM people");
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(page.getByRole("gridcell", { name: "Ada" })).toBeVisible();
});

test("a network connection is configured with TLS and a stored password", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New connection" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Name", { exact: true }).fill("Staging");
  await dialog.getByLabel("Host").fill("db.example.com");
  await expect(dialog.getByLabel("Mode", { exact: true })).toHaveValue("verify-full");
  await dialog.getByLabel("Mode", { exact: true }).selectOption("require");
  await expect(dialog.getByRole("note")).toContainText("not checked");

  await dialog.getByRole("button", { name: "Test connection" }).click();
  await expect(dialog.getByText("password authentication failed")).toBeVisible();
  await dialog.getByLabel("Password").fill("s3cret");
  await dialog.getByRole("button", { name: "Test connection" }).click();
  await expect(dialog.getByText("Connected: PostgreSQL 18 (mock)")).toBeVisible();
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();

  // Row actions appear on hover, as for a user.
  await page.getByRole("button", { name: /^Staging/ }).hover();
  await page.getByRole("button", { name: "Edit Staging" }).click();
  await expect(page.getByRole("dialog").getByLabel("Password")).toHaveAttribute(
    "placeholder",
    "Stored in the keychain",
  );
});

test("an SSH tunnel asks to trust the server's key, then connects", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New connection" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Name", { exact: true }).fill("Behind bastion");
  await dialog.getByLabel("Password", { exact: true }).fill("db-pw");
  await dialog.getByLabel("Connect through an SSH server").check();
  await dialog.getByLabel("SSH server host").fill("bastion.example.com");
  await dialog.getByLabel("SSH server user").fill("deploy");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();

  await page.getByRole("button", { name: /^Behind bastion/ }).click();
  const prompt = page.getByRole("dialog", { name: "Trust this SSH server?" });
  await expect(prompt.getByTestId("fingerprint")).toContainText("SHA256:");
  await prompt.getByRole("button", { name: "Trust and connect" }).click();

  await runInEditor(page, "SELECT * FROM people");
  await expect(page.getByRole("gridcell", { name: "Ada" })).toBeVisible();
});
