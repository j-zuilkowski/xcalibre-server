import { expect, test } from "@playwright/test";
import type { AuthSession } from "@xs/shared";
import { bootstrapAdminSession, seedAuthState } from "./helpers/auth";

let adminSession!: AuthSession;

test.beforeAll(async ({ browser }) => {
  adminSession = await bootstrapAdminSession(browser);
});

test.beforeEach(async ({ page }) => {
  await seedAuthState(page.context(), adminSession);
  await page.goto("/library");
});

test("admin panel is accessible from user avatar menu", async ({ page }) => {
  await page.getByLabel("User menu").click();
  await expect(page.getByRole("link", { name: "Admin Panel" })).toBeVisible();

  await page.getByRole("link", { name: "Admin Panel" }).click();
  await expect(page).toHaveURL(/\/admin\/dashboard$/);
});

test("users table lists at least the admin user", async ({ page }) => {
  await page.goto("/admin/users");

  await expect(page.getByRole("row").filter({ hasText: "admin@test.local" })).toBeVisible();
});

test("create user inline and verify it appears in table", async ({ page }) => {
  const username = `e2e-test-user-create-${Date.now()}`;

  await page.goto("/admin/users");
  const createSection = page.locator("section").filter({ hasText: "Create user" });

  await createSection.getByPlaceholder("Username").fill(username);
  await createSection.getByPlaceholder("Email").fill(`${username}@test.local`);
  await createSection.getByPlaceholder("Password").fill("Test1234!");
  await createSection.getByRole("combobox").selectOption({ label: "user" });
  await createSection.getByRole("button", { name: "Create" }).click();

  await expect(page.getByRole("row").filter({ hasText: username })).toBeVisible();
});

test("delete user removes them from the table", async ({ page }) => {
  const username = `e2e-test-user-delete-${Date.now()}`;

  await page.goto("/admin/users");
  const createSection = page.locator("section").filter({ hasText: "Create user" });
  await createSection.getByPlaceholder("Username").fill(username);
  await createSection.getByPlaceholder("Email").fill(`${username}@test.local`);
  await createSection.getByPlaceholder("Password").fill("Test1234!");
  await createSection.getByRole("combobox").selectOption({ label: "user" });
  await createSection.getByRole("button", { name: "Create" }).click();

  const row = page.getByRole("row").filter({ hasText: username });
  await expect(row).toBeVisible();

  page.once("dialog", (dialog) => dialog.accept());
  await row.getByRole("button", { name: "Delete" }).click();

  await expect(page.getByRole("row").filter({ hasText: username })).toHaveCount(0);
});

test("import page renders with drag-drop zone and dry run toggle", async ({ page }) => {
  await page.goto("/admin/import");

  await expect(page.getByTestId("import-dropzone")).toBeVisible();
  await expect(page.getByLabel("Dry run")).toBeVisible();
});
