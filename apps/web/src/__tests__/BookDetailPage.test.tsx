import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Book, User } from "@autolibre/shared";
import { BookDetailPage } from "../features/library/BookDetailPage";
import { apiClient } from "../lib/api-client";
import { useAuthStore } from "../lib/auth-store";

const getBookMock = vi.spyOn(apiClient, "getBook");
const getLlmHealthMock = vi.spyOn(apiClient, "getLlmHealth");
const listShelvesMock = vi.spyOn(apiClient, "listShelves");
const getBookCustomValuesMock = vi.spyOn(apiClient, "getBookCustomValues");

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
    tags: [
      { id: "tag-1", name: "Fiction", confirmed: true },
      { id: "tag-2", name: "Draft", confirmed: false },
    ],
    formats: [
      { id: "format-1", format: "epub", size_bytes: 1024 },
      { id: "format-2", format: "pdf", size_bytes: 4096 },
    ],
    cover_url: null,
    has_cover: false,
    is_read: false,
    is_archived: false,
    identifiers: [{ id: "id-1", id_type: "isbn", value: "9780441172719" }],
    created_at: "2026-04-18T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
    indexed_at: null,
  };
}

function setUser(user: User | null) {
  useAuthStore.setState({
    access_token: null,
    refresh_token: null,
    user,
    setAuth: useAuthStore.getState().setAuth,
    clearAuth: useAuthStore.getState().clearAuth,
  });
}

function makeUser(role: Partial<User["role"]>): User {
  return {
    id: "user-1",
    username: "reader",
    email: "reader@example.com",
    role: {
      id: "role-1",
      name: role.name ?? "reader",
      can_edit: role.can_edit,
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

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <BookDetailPage bookId="book-1" />
    </QueryClientProvider>,
  );
}

describe("BookDetailPage", () => {
  beforeEach(() => {
    getBookMock.mockReset();
    getLlmHealthMock.mockReset();
    listShelvesMock.mockReset();
    getBookCustomValuesMock.mockReset();
    window.history.replaceState({}, "", "/books/book-1");
    setUser(makeUser({ name: "admin", can_edit: true }));
    getLlmHealthMock.mockResolvedValue({
      enabled: false,
      librarian: {
        available: false,
        model_id: null,
        endpoint: "http://llm.local",
      },
    });
    listShelvesMock.mockResolvedValue([]);
    getBookCustomValuesMock.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    setUser(null);
  });

  test("test_shows_book_title_and_author", async () => {
    getBookMock.mockResolvedValue(makeBook());

    renderPage();

    expect(await screen.findByRole("heading", { name: "Dune" })).toBeTruthy();
    expect(screen.getByText("Frank Herbert")).toBeTruthy();
  });

  test("test_download_dropdown_lists_formats", async () => {
    getBookMock.mockResolvedValue(makeBook());

    renderPage();
    await screen.findByRole("heading", { name: "Dune" });

    fireEvent.click(screen.getByRole("button", { name: "Download ▾" }));

    expect(screen.getByText("EPUB")).toBeTruthy();
    expect(screen.getByText("PDF")).toBeTruthy();
  });

  test("test_edit_menu_hidden_from_non_editors", async () => {
    setUser(makeUser({ name: "reader", can_edit: false }));
    getBookMock.mockResolvedValue(makeBook());

    renderPage();
    await screen.findByRole("heading", { name: "Dune" });

    expect(screen.queryByRole("button", { name: "More actions" })).toBeNull();
  });

  test("test_expandable_sections_toggle", async () => {
    getBookMock.mockResolvedValue(makeBook());

    renderPage();
    await screen.findByRole("heading", { name: "Dune" });

    expect(screen.queryByText("A desert planet novel.")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Description" }));
    expect(screen.getByText("A desert planet novel.")).toBeTruthy();
  });

  test("test_tag_click_navigates_to_filtered_library", async () => {
    getBookMock.mockResolvedValue(makeBook());

    renderPage();
    await screen.findByRole("heading", { name: "Dune" });

    fireEvent.click(screen.getByRole("button", { name: "Fiction" }));

    expect(window.location.pathname).toBe("/library");
    expect(window.location.search).toContain("tag=Fiction");
  });
});
