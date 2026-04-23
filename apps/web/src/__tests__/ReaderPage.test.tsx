import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Book, ReadingProgress, User } from "@autolibre/shared";
import { ReaderPage } from "../features/reader/ReaderPage";
import { apiClient } from "../lib/api-client";
import { useAuthStore } from "../lib/auth-store";

const epubCallbacks: Array<(payload: { start: { percentage: number; cfi: string } }) => void> = [];

vi.mock("epubjs", () => {
  return {
    default: vi.fn(() => ({
      renderTo: () => ({
        display: vi.fn(async () => undefined),
        next: vi.fn(async () => {
          epubCallbacks.at(-1)?.({
            start: {
              percentage: 0.25,
              cfi: "epubcfi(/6/2)",
            },
          });
        }),
        prev: vi.fn(async () => undefined),
        on: vi.fn((event: string, callback: (payload: { start: { percentage: number; cfi: string } }) => void) => {
          if (event === "relocated") {
            epubCallbacks.push(callback);
          }
        }),
        themes: {
          default: vi.fn(),
          fontSize: vi.fn(),
        },
        destroy: vi.fn(),
      }),
      destroy: vi.fn(),
      loaded: {
        navigation: Promise.resolve({
          toc: [{ id: "chapter-1", label: "Chapter 1", href: "chapter-1.xhtml" }],
        }),
      },
    })),
  };
});

vi.mock("pdfjs-dist", () => {
  return {
    default: {
      GlobalWorkerOptions: {},
      getDocument: vi.fn(() => ({
        promise: Promise.resolve({
          numPages: 10,
          getPage: vi.fn(async (page: number) => ({
            getViewport: ({ scale }: { scale: number }) => ({ width: 600 * scale, height: 800 * scale }),
            render: vi.fn(() => ({
              promise: Promise.resolve(),
            })),
          })),
        }),
      })),
    },
  };
});

vi.mock("../features/reader/ComicReader", () => {
  return {
    ComicReader: () => <div data-testid="comic-reader" />,
  };
});

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
    is_read: false,
    is_archived: false,
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
    default_library_id: "default",
    totp_enabled: false,
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

async function renderReaderAndFlush(pathname: string) {
  renderReader(pathname);

  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
}

describe("ReaderPage", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    getBookMock.mockReset();
    getReadingProgressMock.mockReset();
    patchReadingProgressMock.mockReset();
    epubCallbacks.length = 0;
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
    vi.runAllTimers();
    vi.clearAllTimers();
    vi.useRealTimers();
    cleanup();
  });

  test("test_epub_reader_renders_for_epub_format", async () => {
    await renderReaderAndFlush("/books/book-1/read/epub");

    expect(screen.getByTestId("epub-reader")).toBeTruthy();
    expect(screen.queryByTestId("pdf-reader")).toBeNull();
  });

  test("test_pdf_reader_renders_for_pdf_format", async () => {
    await renderReaderAndFlush("/books/book-1/read/pdf");

    expect(screen.getByTestId("pdf-reader")).toBeTruthy();
    expect(screen.queryByTestId("epub-reader")).toBeNull();
  });

  test("test_comic_reader_renders_for_cbz_format", async () => {
    await renderReaderAndFlush("/books/book-1/read/cbz");

    expect(screen.getByTestId("comic-reader")).toBeTruthy();
    expect(screen.queryByTestId("epub-reader")).toBeNull();
    expect(screen.queryByTestId("pdf-reader")).toBeNull();
  });

  test("test_reader_saves_progress_on_advance", async () => {
    await renderReaderAndFlush("/books/book-1/read/epub");

    fireEvent.keyDown(window, { key: "ArrowRight" });

    act(() => {
      vi.advanceTimersByTime(600);
    });

    expect(patchReadingProgressMock).toHaveBeenCalledWith(
      "book-1",
      expect.objectContaining({ format_id: "format-1", percentage: expect.any(Number) }),
    );
  });

  test("test_reader_restores_progress_on_load", async () => {
    getReadingProgressMock.mockResolvedValue(makeProgress({ percentage: 42, cfi: "epubcfi(/6/2)" }));

    await renderReaderAndFlush("/books/book-1/read/epub");

    const label = screen.getByTestId("reader-progress-label");
    expect(label.textContent).toContain("42%");
  });

  test("test_toolbar_fades_in_on_mouse_move", async () => {
    await renderReaderAndFlush("/books/book-1/read/epub");

    const reader = screen.getByTestId("epub-reader");
    const toolbar = screen.getByTestId("reader-toolbar");

    expect(toolbar.getAttribute("data-visible")).toBe("false");

    fireEvent.mouseMove(reader);

    expect(toolbar.getAttribute("data-visible")).toBe("true");
  });

  test("test_settings_panel_opens_on_settings_click", async () => {
    await renderReaderAndFlush("/books/book-1/read/epub");

    const reader = screen.getByTestId("epub-reader");
    fireEvent.mouseMove(reader);

    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(screen.getByText("Reader settings")).toBeTruthy();
  });
});
