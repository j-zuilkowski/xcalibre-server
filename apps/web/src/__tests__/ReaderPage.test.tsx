import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Book, ReadingProgress, User } from "@calibre/shared";
import { ReaderPage } from "../features/reader/ReaderPage";
import { apiClient } from "../lib/api-client";
import { useAuthStore } from "../lib/auth-store";

const getBookMock = vi.spyOn(apiClient, "getBook");
const getReadingProgressMock = vi.spyOn(apiClient, "getReadingProgress");
const patchReadingProgressMock = vi.spyOn(apiClient, "patchReadingProgress");

function makeBook(): Book {
  return {
    id: "book-1",
    title: "Dune",
    sort_title: "Dune",
    description: "A desert planet novel.",
    pubdate: "1965-08-01T00:00:00Z",
    language: "en",
    rating: 8,
    series: { id: "series-1", name: "Dune" },
    series_index: 1,
    authors: [{ id: "author-1", name: "Frank Herbert", sort_name: "Herbert, Frank" }],
    tags: [],
    formats: [
      { id: "format-1", format: "epub", size_bytes: 1024 },
      { id: "format-2", format: "pdf", size_bytes: 4096 },
    ],
    cover_url: null,
    has_cover: false,
    identifiers: [],
    created_at: "2026-04-18T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
    indexed_at: null,
  };
}

function makeProgress(overrides: Partial<ReadingProgress> = {}): ReadingProgress {
  return {
    id: "progress-1",
    book_id: "book-1",
    format_id: "format-1",
    cfi: null,
    page: null,
    percentage: 0,
    updated_at: "2026-04-19T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
    ...overrides,
  };
}

function makeUser(): User {
  return {
    id: "user-1",
    username: "reader",
    email: "reader@example.com",
    role: {
      id: "role-1",
      name: "reader",
      can_edit: false,
      can_bulk: false,
      can_upload: false,
      can_download: true,
    },
    is_active: true,
    force_pw_reset: false,
    created_at: "2026-04-19T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
  };
}

function renderReader(pathname: string) {
  window.history.replaceState({}, "", pathname);

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <ReaderPage />
    </QueryClientProvider>,
  );
}

describe("ReaderPage", () => {
  beforeEach(() => {
    getBookMock.mockReset();
    getReadingProgressMock.mockReset();
    patchReadingProgressMock.mockReset();
    getBookMock.mockResolvedValue(makeBook());
    getReadingProgressMock.mockResolvedValue(null);
    patchReadingProgressMock.mockResolvedValue(makeProgress());

    useAuthStore.setState({
      access_token: null,
      refresh_token: null,
      user: makeUser(),
      setAuth: useAuthStore.getState().setAuth,
      clearAuth: useAuthStore.getState().clearAuth,
    });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  test("test_epub_reader_renders_for_epub_format", async () => {
    renderReader("/books/book-1/read/epub");

    expect(await screen.findByTestId("epub-reader")).toBeTruthy();
    expect(screen.queryByTestId("pdf-reader")).toBeNull();
  });

  test("test_pdf_reader_renders_for_pdf_format", async () => {
    renderReader("/books/book-1/read/pdf");

    expect(await screen.findByTestId("pdf-reader")).toBeTruthy();
    expect(screen.queryByTestId("epub-reader")).toBeNull();
  });

  test("test_reader_saves_progress_on_advance", async () => {
    renderReader("/books/book-1/read/epub");
    await screen.findByTestId("epub-reader");

    fireEvent.keyDown(window, { key: "ArrowRight" });

    await waitFor(() => {
      expect(patchReadingProgressMock).toHaveBeenCalledWith(
        "book-1",
        expect.objectContaining({ format: "epub", percentage: expect.any(Number) }),
      );
    }, { timeout: 2000 });
  });

  test("test_reader_restores_progress_on_load", async () => {
    getReadingProgressMock.mockResolvedValue(makeProgress({ percentage: 42, cfi: "epubcfi(/6/2)" }));

    renderReader("/books/book-1/read/epub");

    const label = await screen.findByTestId("reader-progress-label");
    expect(label.textContent).toContain("42%");
  });

  test("test_toolbar_fades_in_on_mouse_move", async () => {
    renderReader("/books/book-1/read/epub");

    const reader = await screen.findByTestId("epub-reader");
    const toolbar = screen.getByTestId("reader-toolbar");

    expect(toolbar.getAttribute("data-visible")).toBe("false");

    fireEvent.mouseMove(reader);

    expect(toolbar.getAttribute("data-visible")).toBe("true");
  });

  test("test_settings_panel_opens_on_settings_click", async () => {
    renderReader("/books/book-1/read/epub");

    const reader = await screen.findByTestId("epub-reader");
    fireEvent.mouseMove(reader);

    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));

    expect(await screen.findByText("Reader settings")).toBeTruthy();
  });
});
