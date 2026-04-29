import { expect, test } from "@playwright/test";
import type { Book } from "@xs/shared";
import { fileURLToPath } from "node:url";
import { bootstrapAdminSession, createUser, loginViaApi, seedAuthState, E2E_ADMIN_PASSWORD, E2E_ADMIN_USERNAME } from "./helpers/auth";

const API = process.env.PLAYWRIGHT_API_URL ?? "http://127.0.0.1:8083";
import { uploadFixtureBook } from "./helpers/books";

const FIXTURE_PATH = fileURLToPath(new URL("./fixtures/test.epub", import.meta.url));
const BOOK_TITLE = "Stage One Critical Path";
const SEARCH_TERM = BOOK_TITLE;


test("create user and login", async ({ page }) => {
  // /register is a first-admin-only setup page; subsequent users are created
  // via the admin API. We create a fresh user here and verify login works.
  const testUsername = `e2e-login-${Date.now()}`;
  const testPassword = "Test1234!";
  await createUser(testUsername, testPassword);

  await page.goto("/login");
  await page.getByLabel("Username").fill(testUsername);
  await page.getByLabel("Password").fill(testPassword);
  await page.getByRole("button", { name: "Sign in" }).click();

  await expect(page).toHaveURL(/\/(home|library)$/);
});

test.describe.serial("critical path content", () => {
  let adminSession: AuthSession;
  let uploadedBook: Book;

  test.beforeAll(async ({ browser }) => {
    adminSession = await bootstrapAdminSession(browser);
    uploadedBook = await uploadFixtureBook(adminSession, FIXTURE_PATH, {
      title: BOOK_TITLE,
      author: "Codex",
      authors: ["Codex"],
    });
  });

  test.beforeEach(async ({ page }) => {
    await seedAuthState(page.context(), adminSession);
  });

  test("upload a book and see it in the library", async ({ page }) => {
    await page.goto("/library");

    const card = page.getByRole("article").filter({ hasText: BOOK_TITLE }).first();
    await expect(card.getByRole("link", { name: BOOK_TITLE }).first()).toBeVisible();
  });

  test("search returns results", async ({ page }) => {
    await page.goto(`/search?q=${encodeURIComponent(SEARCH_TERM)}`);

    await expect(page.getByRole("link", { name: BOOK_TITLE }).first()).toBeVisible();
  });

  test("open reader and navigate chapters", async ({ page }) => {
    await page.goto(`/books/${uploadedBook.id}`);
    await page.getByRole("link", { name: "Read" }).first().click();

    await page.waitForURL(new RegExp(`/books/${uploadedBook.id}/read/epub$`, "i"));
    await expect(page.getByTestId("epub-reader")).toBeVisible();

    const reader = page.getByTestId("epub-reader");
    const box = await reader.boundingBox();
    if (box) {
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    }

    await expect(page.getByTestId("reader-toolbar")).toHaveAttribute("data-visible", "true");
    await expect(page.getByTestId("reader-progress-label")).toBeVisible();
  });

  test("admin creates and revokes an API token", async ({ request }) => {
    const session = await loginViaApi(E2E_ADMIN_USERNAME, E2E_ADMIN_PASSWORD);
    const headers = {
      Authorization: `Bearer ${session.access_token}`,
      "Content-Type": "application/json",
    };

    const createResponse = await request.post(`${API}/api/v1/admin/tokens`, {
      headers,
      data: {
        name: "E2E critical path token",
        scope: "read",
      },
    });
    expect(createResponse.status()).toBe(201);

    const created = (await createResponse.json()) as { id: string; name: string; token: string; scope: string };
    expect(created.name).toBe("E2E critical path token");
    expect(created.scope).toBe("read");
    expect(created.token).toMatch(/^[0-9a-f]{64}$/);

    const listedBeforeDelete = await request.get(`${API}/api/v1/admin/tokens`, { headers });
    expect(listedBeforeDelete.ok()).toBeTruthy();
    const beforeDelete = (await listedBeforeDelete.json()) as { items: Array<{ id: string }> };
    expect(beforeDelete.items.some((item) => item.id === created.id)).toBe(true);

    const deleteResponse = await request.delete(`${API}/api/v1/admin/tokens/${created.id}`, { headers });
    expect(deleteResponse.status()).toBe(204);

    const listedAfterDelete = await request.get(`${API}/api/v1/admin/tokens`, { headers });
    expect(listedAfterDelete.ok()).toBeTruthy();
    const afterDelete = (await listedAfterDelete.json()) as { items: Array<{ id: string }> };
    expect(afterDelete.items.some((item) => item.id === created.id)).toBe(false);
  });

  test("memory ingest via API", async ({ request }) => {
    const session = await loginViaApi(E2E_ADMIN_USERNAME, E2E_ADMIN_PASSWORD);
    const headers = {
      Authorization: `Bearer ${session.access_token}`,
      "Content-Type": "application/json",
    };

    const ingestResponse = await request.post(`${API}/api/v1/memory`, {
      headers,
      data: {
        text: "Test memory chunk from E2E",
        chunk_type: "episodic",
      },
    });
    expect(ingestResponse.status()).toBe(201);

    const ingest = (await ingestResponse.json()) as { id: string; created_at: number };
    expect(ingest.id).toMatch(/^[0-9a-f-]{36}$/i);

    const searchResponse = await request.get(
      `${API}/api/v1/search/chunks?q=${encodeURIComponent("Test memory")}&source=memory`,
      { headers },
    );
    expect(searchResponse.ok()).toBeTruthy();

    const search = (await searchResponse.json()) as {
      chunks: Array<{ source: string; text: string }>;
    };
    expect(
      search.chunks.some(
        (chunk) => chunk.source === "memory" && chunk.text.includes("Test memory chunk from E2E"),
      ),
    ).toBe(true);
  });
});
