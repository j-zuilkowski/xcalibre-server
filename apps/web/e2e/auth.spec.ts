import { expect, test } from "@playwright/test";
import { bootstrapAdminSession, createUser, login, loginAsAdmin } from "./helpers/auth";

test.beforeAll(async ({ browser }) => {
  await bootstrapAdminSession(browser);
});

test("login with valid credentials navigates to library", async ({ page }) => {
  const username = `e2e-auth-${crypto.randomUUID()}`;
  const password = "CorrectHorseBatteryStaple1!";

  await createUser(username, password);
  await login(page, username, password);

  await expect(page).toHaveURL(/\/library$/);
});

test("login with wrong password shows error", async ({ page }) => {
  const username = `e2e-auth-${crypto.randomUUID()}`;
  const password = "CorrectHorseBatteryStaple1!";

  await createUser(username, password);

  await page.goto("/login");
  await page.getByLabel("Username").fill(username);
  await page.getByLabel("Password").fill("wrong-password");
  await page.getByRole("button", { name: "Sign in" }).click();

  await expect(page.getByText("Invalid username or password.")).toBeVisible();
});

test("logout clears session and redirects to login", async ({ page }) => {
  await loginAsAdmin(page);

  await page.getByLabel("User menu").click();
  await page.getByRole("button", { name: "Sign out" }).click();

  await expect(page).toHaveURL(/\/login$/);

  await page.reload();
  await expect(page).toHaveURL(/\/login$/);
});

test("unauthenticated access to library redirects to login", async ({ page }) => {
  await page.addInitScript(() => localStorage.clear());
  await page.goto("/library");
  await expect(page).toHaveURL(/\/login$/);
});
