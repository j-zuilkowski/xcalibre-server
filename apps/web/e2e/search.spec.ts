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
});

test("FTS search returns results matching query", async ({ page }) => {
  await page.goto("/library");
  await page.getByPlaceholder("Search title, author, tag").fill("Yellow Wallpaper");
  await page.getByPlaceholder("Search title, author, tag").press("Enter");

  await page.waitForURL(/\/search\?q=Yellow(?:\+|%20)Wallpaper/);
  await expect(page.getByRole("link", { name: /The Yellow Wallpaper/i }).first()).toBeVisible();
});

test("empty search shows no results state", async ({ page }) => {
  await page.goto("/search?q=xyzzy_no_match_12345");

  await expect(page.getByText("No results.")).toBeVisible();
});

test("semantic tab is grayed when LLM is disabled", async ({ page }) => {
  await page.goto("/search");

  const semanticTab = page.getByRole("button", { name: "Semantic" });
  await expect(semanticTab).toBeDisabled();
  await expect(semanticTab).toHaveAttribute("title", "Semantic search is unavailable.");
  await expect(semanticTab).toHaveClass(/text-zinc-400/);
});

test("clicking a search result navigates to book detail", async ({ page }) => {
  await page.goto("/library");
  await page.getByPlaceholder("Search title, author, tag").fill("Yellow Wallpaper");
  await page.getByPlaceholder("Search title, author, tag").press("Enter");
  await page.waitForURL(/\/search\?q=Yellow(?:\+|%20)Wallpaper/);

  await page.getByRole("link", { name: /The Yellow Wallpaper/i }).first().click();

  await expect(page).toHaveURL(/\/books\/[^/]+$/);
});
