import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { BookSummary, PaginatedResponse } from "@xs/shared";
import { LibraryPage } from "../features/library/LibraryPage";
import { apiClient } from "../lib/api-client";
import { makeTestQueryClient } from "../test/query-client";

const listBooksMock = vi.spyOn(apiClient, "listBooks");

function makeBook(id: string, title: string): BookSummary {
  return {
    id,
    title,
    sort_title: title,
    authors: [{ id: `author-${id}`, name: "Test Author", sort_name: "Author, Test" }],
    series: null,
    series_index: null,
    cover_url: null,
    has_cover: false,
    is_read: false,
    is_archived: false,
    language: "en",
    rating: 4,
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

function renderPage() {
  const queryClient = makeTestQueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: Infinity,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <LibraryPage />
    </QueryClientProvider>,
  );
}

describe("LibraryPage", () => {
  beforeEach(() => {
    listBooksMock.mockReset();
    window.history.replaceState({}, "", "/library");
  });

  afterEach(() => {
    cleanup();
  });

  test("test_renders_book_cards", async () => {
    listBooksMock.mockResolvedValue(makeResponse([makeBook("book-1", "Dune")]));

    renderPage();

    expect(await screen.findByText("Dune")).toBeTruthy();
    expect(screen.getByText("Test Author")).toBeTruthy();
  });

  test("test_empty_state_when_no_books", async () => {
    listBooksMock.mockResolvedValue(makeResponse([]));

    renderPage();

    expect(await screen.findByText("No books in your library yet")).toBeTruthy();
  });

  test("test_filter_updates_query", async () => {
    listBooksMock.mockResolvedValue(makeResponse([]));

    renderPage();
    await screen.findByText("No books in your library yet");

    fireEvent.click(screen.getByRole("button", { name: "Author" }));

    await waitFor(() => {
      expect(window.location.search).toContain("author_id=author-default");
    });
  });
});
