import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { BookSummary, PaginatedResponse } from "@calibre/shared";
import { SearchPage } from "../features/search/SearchPage";
import { apiClient } from "../lib/api-client";

const listBooksMock = vi.spyOn(apiClient, "listBooks");

function makeBook(id: string, title: string): BookSummary {
  return {
    id,
    title,
    sort_title: title,
    authors: [{ id: `author-${id}`, name: "Frank Herbert", sort_name: "Herbert, Frank" }],
    series: null,
    series_index: null,
    cover_url: null,
    has_cover: false,
    language: "en",
    rating: 8,
    last_modified: "2026-04-19T00:00:00Z",
  };
}

function makeResponse(items: BookSummary[]): PaginatedResponse<BookSummary> {
  return {
    items,
    total: items.length,
    page: 1,
    page_size: 24,
  };
}

function renderPage(path = "/search?q=dune") {
  window.history.replaceState({}, "", path);

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <SearchPage />
    </QueryClientProvider>,
  );
}

describe("SearchPage", () => {
  beforeEach(() => {
    listBooksMock.mockReset();
    window.history.replaceState({}, "", "/search");
  });

  afterEach(() => {
    cleanup();
  });

  test("test_search_page_renders_books_for_query", async () => {
    listBooksMock.mockResolvedValue(makeResponse([makeBook("book-1", "Dune")]));

    renderPage();

    expect(await screen.findByText("Dune")).toBeTruthy();
    expect(screen.getByText("Frank Herbert")).toBeTruthy();
  });

  test("test_semantic_tab_disabled_when_unavailable", async () => {
    listBooksMock.mockResolvedValue(makeResponse([makeBook("book-1", "Dune")]));

    renderPage();

    const semanticTab = await screen.findByRole("button", { name: "Semantic" });
    expect(semanticTab.hasAttribute("disabled")).toBe(true);
    expect(semanticTab.getAttribute("title")).toBe("Semantic search is unavailable right now.");
  });

  test("test_filter_chip_updates_query", async () => {
    listBooksMock.mockResolvedValue(makeResponse([]));

    renderPage("/search");
    await screen.findByText("Search");

    fireEvent.click(screen.getByRole("button", { name: "Author" }));

    expect(window.location.search).toContain("author_id=author-default");
  });
});
