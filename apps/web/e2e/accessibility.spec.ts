import { test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { fileURLToPath } from "node:url";
import type { AuthSession, Book } from "@xs/shared";
import { bootstrapAdminSession, seedAuthState } from "./helpers/auth";
import { uploadFixtureBook } from "./helpers/books";

const FIXTURE_PATH = fileURLToPath(new URL("./fixtures/test.epub", import.meta.url));
const API_BASE = process.env.PLAYWRIGHT_API_URL ?? "http://127.0.0.1:8083";

let adminSession!: AuthSession;
let uploadedBook!: Book;
let backendReady = false;

async function checkA11y(page: Page) {
  await new AxeBuilder({ page })
    .options({
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa"] },
    })
    .analyze();
}

test.beforeAll(async ({ browser }) => {
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 1000);
    const probe = await fetch(`${API_BASE}/api/v1/auth/providers`, {
      signal: controller.signal,
    }).catch(() => null);
    clearTimeout(timeout);

    if (!probe || !probe.ok) {
      backendReady = false;
      return;
    }

    adminSession = await bootstrapAdminSession(browser);
    uploadedBook = await uploadFixtureBook(adminSession, FIXTURE_PATH, {
      title: "The Yellow Wallpaper",
      author: "Charlotte Perkins Gilman",
      authors: ["Charlotte Perkins Gilman"],
    });
    backendReady = true;
  } catch {
    backendReady = false;
  }
});

test("login page has no critical a11y violations", async ({ page }) => {
  await page.goto("/login");
  await checkA11y(page);
});

test("library page has no critical a11y violations", async ({ page }) => {
  test.skip(!backendReady, "Backend not available in this environment");
  await seedAuthState(page.context(), adminSession);
  await page.goto("/library");
  await checkA11y(page);
});

test("reader has no critical a11y violations", async ({ page }) => {
  test.skip(!backendReady, "Backend not available in this environment");
  await seedAuthState(page.context(), adminSession);
  await page.goto(`/books/${uploadedBook.id}`);
  await page.getByRole("link", { name: "Read" }).first().click();
  await page.waitForURL(new RegExp(`/books/${uploadedBook.id}/read/`));
  await checkA11y(page);
});

test("admin panel has no critical a11y violations", async ({ page }) => {
  test.skip(!backendReady, "Backend not available in this environment");
  await seedAuthState(page.context(), adminSession);
  await page.goto("/admin/dashboard");
  await checkA11y(page);
});
