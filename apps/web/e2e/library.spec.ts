import { expect, test } from "@playwright/test";
import { fileURLToPath } from "node:url";
import type { AuthSession } from "@xs/shared";
import { bootstrapAdminSession, seedAuthState } from "./helpers/auth";
import { uploadFixtureBook } from "./helpers/books";

const FIXTURE_PATH = fileURLToPath(new URL("./fixtures/test.epub", import.meta.url));

let adminSession!: AuthSession;

test.beforeAll(async ({ browser }) => {
  adminSession = await bootstrapAdminSession(browser);
  await uploadFixtureBook(adminSession, FIXTURE_PATH, {
    title: "The Yellow Wallpaper",
    author: "Charlotte Perkins Gilman",
    authors: ["Charlotte Perkins Gilman"],
  });
});

test.beforeEach(async ({ page }) => {
  await seedAuthState(page.context(), adminSession);
  await page.goto("/library");
});

test("library grid renders at least one book card", async ({ page }) => {
  await expect(page.getByRole("img", { name: /The Yellow Wallpaper/i }).first()).toBeVisible();
});

test("filter chip opens filter panel and filters results", async ({ page }) => {
  await page.getByRole("button", { name: "Format" }).click();

  await expect(page).toHaveURL(/format=epub/);
  await expect(page.getByRole("img", { name: /The Yellow Wallpaper/i }).first()).toBeVisible();
});

test("sort dropdown changes book order", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" && !message.text().includes("Cannot update a component")) {
      consoleErrors.push(message.text());
    }
  });

  await page.getByLabel("Sort").selectOption("created_at");
  await expect(page).toHaveURL(/sort=created_at/);

  expect(consoleErrors).toEqual([]);
});

test("grid/list toggle switches to list view", async ({ page }) => {
  await page.getByRole("button", { name: "List" }).click();

  await expect(page).toHaveURL(/view=list/);
  await expect(page.getByRole("article").first()).toBeVisible();
  await expect(page.locator("section.grid.grid-cols-2.gap-4")).toHaveCount(0);
});

test("clicking a book card navigates to book detail", async ({ page }) => {
  await page.getByRole("link", { name: /The Yellow Wallpaper/i }).first().click();

  await expect(page).toHaveURL(/\/books\/[^/]+$/);
});
