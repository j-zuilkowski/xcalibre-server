import { expect, test } from "@playwright/test";
import { fileURLToPath } from "node:url";
import type { AuthSession, Book } from "@xs/shared";
import { bootstrapAdminSession, seedAuthState } from "./helpers/auth";
import { uploadFixtureBook } from "./helpers/books";

const FIXTURE_PATH = fileURLToPath(new URL("./fixtures/test.epub", import.meta.url));

let adminSession!: AuthSession;
let uploadedBook!: Book;

test.beforeAll(async ({ browser }) => {
  adminSession = await bootstrapAdminSession(browser);
  uploadedBook = await uploadFixtureBook(adminSession, FIXTURE_PATH, {
    title: "The Yellow Wallpaper",
    author: "Charlotte Perkins Gilman",
    authors: ["Charlotte Perkins Gilman"],
  });
});

test.beforeEach(async ({ page }) => {
  await seedAuthState(page.context(), adminSession);
  await page.goto(`/books/${uploadedBook.id}`);
  await page.getByRole("link", { name: "Read" }).first().click();
  await page.waitForURL(new RegExp(`/books/${uploadedBook.id}/read/`));
  await expect(page.getByTestId("epub-reader")).toBeVisible();
});

test("EPUB reader opens and displays content", async ({ page }) => {
  await expect(page.getByTestId("epub-reader")).toBeVisible();
  await expect(page.getByTestId("reader-progress-label")).toHaveText("0%");
  await expect(page.getByText("The Yellow Wallpaper · Charlotte Perkins Gilman")).toBeVisible();
  await expect(page).toHaveURL(new RegExp(`/books/${uploadedBook.id}/read/`));
  await expect(page.getByText(/unable to load/i)).toHaveCount(0);
});

test("reader toolbar fades in on mouse move", async ({ page }) => {
  const reader = page.getByTestId("epub-reader");
  const toolbar = page.getByTestId("reader-toolbar");

  await expect(toolbar).toHaveAttribute("data-visible", "false");

  const box = await reader.boundingBox();
  if (box) {
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  }

  await expect(toolbar).toHaveAttribute("data-visible", "true");

  await page.waitForTimeout(4_000);
  await expect(toolbar).toHaveAttribute("data-visible", "false");
});

test("reading progress is saved and shown on return to library", async ({ page }) => {
  await page.evaluate(
    async ({ accessToken, bookId, formatId }) => {
      await fetch(`/api/v1/reading-progress/${bookId}`, {
        method: "PATCH",
        headers: {
          Authorization: `Bearer ${accessToken}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          format_id: formatId,
          percentage: 42,
          cfi: null,
          page: null,
        }),
      });
    },
    {
      accessToken: adminSession.access_token,
      bookId: uploadedBook.id,
      formatId: uploadedBook.formats[0].id,
    },
  );

  await page.goto("/library");
  const bookCard = page.locator(`article:has(a[href="/books/${uploadedBook.id}"])`).first();
  await bookCard.hover();
  await expect(bookCard.getByTestId("progress-bar")).toBeVisible();
});

test("reader settings panel opens on gear icon click", async ({ page }) => {
  const reader = page.getByTestId("epub-reader");
  const box = await reader.boundingBox();
  if (box) {
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  }

  await expect(page.getByTestId("reader-toolbar")).toHaveAttribute("data-visible", "true");
  await page.getByLabel("Open settings").click();

  await expect(page.getByText("Reader settings")).toBeVisible();
  await expect(page.getByText(/Font size/i)).toBeVisible();
});

test("TOC panel opens on menu icon click", async ({ page }) => {
  const reader = page.getByTestId("epub-reader");
  const box = await reader.boundingBox();
  if (box) {
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  }

  await expect(page.getByTestId("reader-toolbar")).toHaveAttribute("data-visible", "true");
  await page.getByLabel("Open table of contents").click();

  await expect(page.getByRole("heading", { name: "Table of contents" })).toBeVisible();
  await expect(page.getByText("No table of contents available.")).toBeVisible();
});
