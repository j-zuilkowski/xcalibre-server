import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { PaginatedResponse, SearchResultItem, SearchStatusResponse } from "@xs/shared";
import { SearchPage } from "../features/search/SearchPage";
import { apiClient } from "../lib/api-client";

const searchMock = vi.spyOn(apiClient, "search");
const searchStatusMock = vi.spyOn(apiClient, "getSearchStatus");
const listCollectionsMock = vi.spyOn(apiClient, "listCollections");

function makeBook(id: string, title: string, score?: number): SearchResultItem {
  return {
    id,
    title,
    sort_title: title,
    authors: [{ id: `author-${id}`, name: "Frank Herbert", sort_name: "Herbert, Frank" }],
    series: null,
    series_index: null,
    cover_url: null,
    has_cover: false,
    is_read: false,
    is_archived: false,
    language: "en",
    rating: 8,
    last_modified: "2026-04-19T00:00:00Z",
    score,
  };
}

function makeResponse(items: SearchResultItem[]): PaginatedResponse<SearchResultItem> {
  return {
    items,
    total: items.length,
    page: 1,
    page_size: 24,
  };
}

function makeSearchStatus(semantic: boolean): SearchStatusResponse {
  return {
    fts: true,
    meilisearch: false,
    semantic,
    backend: "fts5",
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
    searchMock.mockReset();
    searchStatusMock.mockReset();
    listCollectionsMock.mockReset();
    searchStatusMock.mockResolvedValue(makeSearchStatus(false));
    listCollectionsMock.mockResolvedValue([]);
    window.history.replaceState({}, "", "/search");
  });

  afterEach(() => {
    cleanup();
  });

  test("test_search_page_renders_books_for_query", async () => {
    searchMock.mockResolvedValue(makeResponse([makeBook("book-1", "Dune", 0.84)]));

    renderPage();

    expect(await screen.findByText("Dune")).toBeTruthy();
    expect(screen.getByText("Frank Herbert")).toBeTruthy();
  });

  test("test_semantic_tab_disabled_when_unavailable", async () => {
    searchMock.mockResolvedValue(makeResponse([makeBook("book-1", "Dune")]));

    renderPage();

    const semanticTab = await screen.findByRole("button", { name: "Semantic" });
    expect(semanticTab.hasAttribute("disabled")).toBe(true);
    expect(semanticTab.getAttribute("title")).toBe("Semantic search is unavailable.");
  });

  test("test_semantic_tab_enabled_when_available", async () => {
    searchStatusMock.mockResolvedValue(makeSearchStatus(true));
    searchMock.mockResolvedValue(makeResponse([makeBook("book-1", "Dune")]));

    renderPage();

    const semanticTab = await screen.findByRole("button", { name: "Semantic" });
    await waitFor(() => expect(semanticTab.hasAttribute("disabled")).toBe(false));

    fireEvent.click(semanticTab);
    expect(window.location.search).toContain("tab=semantic");
  });

  test("test_score_badge_shown_when_score_present", async () => {
    searchMock.mockResolvedValue(makeResponse([makeBook("book-1", "Dune", 0.84)]));

    renderPage();

    expect(await screen.findByText("Match 84%")).toBeTruthy();
  });

  test("test_filter_chip_updates_query", async () => {
    searchMock.mockResolvedValue(makeResponse([]));

    renderPage("/search");
    await screen.findByText("Search");

    fireEvent.click(screen.getByRole("button", { name: "Author" }));

    await waitFor(() => {
      expect(window.location.search).toContain("author_id=author-default");
    });
  });

  test("test_collection_filter_updates_query", async () => {
    searchMock.mockResolvedValue(makeResponse([]));
    listCollectionsMock.mockResolvedValue([
      {
        id: "collection-1",
        name: "Oracle 19c",
        description: null,
        domain: "technical",
        is_public: false,
        book_count: 3,
        total_chunks: 12,
        created_at: "2026-04-19T00:00:00Z",
        updated_at: "2026-04-19T00:00:00Z",
      },
    ]);

    renderPage("/search");
    await screen.findByText("Search");
    await screen.findByRole("option", { name: "Oracle 19c" });

    const user = userEvent.setup();
    await user.selectOptions(screen.getByLabelText("Collection"), "collection-1");

    await waitFor(() => {
      expect(window.location.search).toContain("collection_id=collection-1");
    });
  });
});
