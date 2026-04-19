import { afterAll, afterEach, beforeAll, describe, expect, test, vi } from "vitest";
import { http, HttpResponse } from "msw";
import { setupServer } from "msw/node";
import { ApiClient } from "../client";

const server = setupServer();

beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe("ApiClient", () => {
  test("test_login_sends_correct_request", async () => {
    const payload = { username: "alice", password: "secret" };
    const requestSpy = vi.fn();

    server.use(
      http.post("http://example.test/api/v1/auth/login", async ({ request }) => {
        requestSpy(await request.json());
        return HttpResponse.json({
          access_token: "access",
          refresh_token: "refresh",
          user: {
            id: "user-1",
            username: "alice",
            email: "alice@example.com",
            role: {
              id: "role-1",
              name: "admin",
            },
            is_active: true,
            force_pw_reset: false,
            created_at: "2026-04-18T00:00:00Z",
            last_modified: "2026-04-18T00:00:00Z",
          },
        });
      }),
    );

    const client = new ApiClient("http://example.test", () => null, () => {});
    await client.login(payload);

    expect(requestSpy).toHaveBeenCalledWith(payload);
  });

  test("test_login_returns_tokens", async () => {
    server.use(
      http.post("http://example.test/api/v1/auth/login", () =>
        HttpResponse.json({
          access_token: "access",
          refresh_token: "refresh",
          user: {
            id: "user-1",
            username: "alice",
            email: "alice@example.com",
            role: {
              id: "role-1",
              name: "admin",
            },
            is_active: true,
            force_pw_reset: false,
            created_at: "2026-04-18T00:00:00Z",
            last_modified: "2026-04-18T00:00:00Z",
          },
        }),
      ),
    );

    const client = new ApiClient("http://example.test", () => null, () => {});
    await expect(client.login({ username: "alice", password: "secret" })).resolves.toMatchObject({
      access_token: "access",
      refresh_token: "refresh",
    });
  });

  test("test_refresh_on_401", async () => {
    let accessToken = "expired-access";
    const unauthorized = vi.fn();
    const retrySpy = vi.fn();

    server.use(
      http.get("http://example.test/api/v1/books", ({ request }) => {
        const header = request.headers.get("authorization");
        retrySpy(header);

        if (header === "Bearer fresh-access") {
          return HttpResponse.json({
            items: [],
            total: 0,
            page: 1,
            page_size: 20,
          });
        }

        return HttpResponse.json({}, { status: 401 });
      }),
      http.post("http://example.test/api/v1/auth/refresh", async ({ request }) => {
        expect(await request.json()).toEqual({ refresh_token: "refresh-token" });
        accessToken = "fresh-access";
        return HttpResponse.json({
          access_token: "fresh-access",
          refresh_token: "new-refresh-token",
        });
      }),
    );

    const client = new ApiClient(
      "http://example.test",
      () => accessToken,
      unauthorized,
      {
        getRefreshToken: () => "refresh-token",
      },
    );

    await client.listBooks({ page: 1 });

    expect(retrySpy).toHaveBeenCalledWith("Bearer expired-access");
    expect(retrySpy).toHaveBeenCalledWith("Bearer fresh-access");
    expect(unauthorized).not.toHaveBeenCalled();
  });

  test("test_list_books_builds_correct_url", async () => {
    const requestSpy = vi.fn();

    server.use(
      http.get("http://example.test/api/v1/books", ({ request }) => {
        requestSpy(request.url.toString());
        return HttpResponse.json({
          items: [],
          total: 0,
          page: 2,
          page_size: 20,
        });
      }),
    );

    const client = new ApiClient("http://example.test", () => null, () => {});
    await client.listBooks({
      q: "wind",
      tag: ["fiction", "classic"],
      page: 2,
      page_size: 20,
      sort: "title",
    });

    expect(requestSpy).toHaveBeenCalledWith(
      "http://example.test/api/v1/books?q=wind&tag=fiction&tag=classic&page=2&page_size=20&sort=title",
    );
  });

  test("test_get_book_returns_book", async () => {
    server.use(
      http.get("http://example.test/api/v1/books/abc", () =>
        HttpResponse.json({
          id: "abc",
          title: "Book Title",
          sort_title: "Book Title",
          description: null,
          pubdate: null,
          language: null,
          rating: null,
          series: null,
          series_index: null,
          authors: [],
          tags: [],
          formats: [],
          cover_url: null,
          has_cover: false,
          identifiers: [],
          created_at: "2026-04-18T00:00:00Z",
          last_modified: "2026-04-18T00:00:00Z",
          indexed_at: null,
        }),
      ),
    );

    const client = new ApiClient("http://example.test", () => null, () => {});
    await expect(client.getBook("abc")).resolves.toMatchObject({
      id: "abc",
      title: "Book Title",
    });
  });

  test("test_api_error_on_non_ok_response", async () => {
    server.use(
      http.get("http://example.test/api/v1/books/abc", () =>
        HttpResponse.json({ detail: "nope" }, { status: 418, statusText: "I'm a teapot" }),
      ),
    );

    const client = new ApiClient("http://example.test", () => null, () => {});
    await expect(client.getBook("abc")).rejects.toMatchObject({
      status: 418,
      message: "I'm a teapot",
      details: { detail: "nope" },
    });
  });
});
