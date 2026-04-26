import { describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import { renderWithProviders } from "../../test/render";
import { makeSearchResult } from "../../test/fixtures";
import { server } from "../../test/setup";
import { screen } from "@testing-library/react";

function renderSearchPage(path = "/search") {
  return renderWithProviders(<></>, {
    initialPath: path,
    authenticated: true,
  });
}

describe("SearchPage", () => {
  test("renders the search prompt when the query is empty", async () => {
    renderSearchPage();

    expect(await screen.findByText(/enter a search query/i)).toBeTruthy();
    expect(screen.queryByText("Dune")).toBeNull();
  });

  test("renders search results for a query", async () => {
    renderSearchPage("/search?q=dune");

    expect(await screen.findByText("Dune")).toBeTruthy();
  });

  test("shows a semantic score badge when a score is returned", async () => {
    server.use(
      http.get("/api/v1/search", () =>
        HttpResponse.json({
          items: [makeSearchResult({ id: "1", title: "Dune", score: 0.85 })],
          total: 1,
          page: 1,
          page_size: 24,
        }),
      ),
    );

    renderSearchPage("/search?q=dune&tab=semantic");

    expect(await screen.findByText(/match 85%/i)).toBeTruthy();
  });

  test("shows a no results state", async () => {
    server.use(
      http.get("/api/v1/search", ({ request }) => {
        const url = new URL(request.url);
        if (url.searchParams.get("q") !== "missing") {
          return HttpResponse.json({
            items: [makeSearchResult()],
            total: 1,
            page: 1,
            page_size: 24,
          });
        }

        return HttpResponse.json({
          items: [],
          total: 0,
          page: 1,
          page_size: 24,
        });
      }),
    );

    renderSearchPage("/search?q=missing");

    expect(await screen.findByRole("heading", { name: /no results\./i })).toBeTruthy();
  });

  test("shows an error state", async () => {
    server.use(
      http.get("/api/v1/search", () => HttpResponse.json({ message: "failed" }, { status: 500 })),
    );

    renderSearchPage("/search?q=error");

    expect(await screen.findByText(/unable to search/i)).toBeTruthy();
  });
});
