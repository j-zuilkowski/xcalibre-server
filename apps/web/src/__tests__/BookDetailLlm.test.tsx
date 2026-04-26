import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Book, ClassifyResult, LlmHealth, User } from "@xs/shared";
import { BookDetailPage } from "../features/library/BookDetailPage";
import { apiClient } from "../lib/api-client";
import { useAuthStore } from "../lib/auth-store";

const getBookMock = vi.spyOn(apiClient, "getBook");
const getLlmHealthMock = vi.spyOn(apiClient, "getLlmHealth");
const classifyBookMock = vi.spyOn(apiClient, "classifyBook");
const confirmTagsMock = vi.spyOn(apiClient, "confirmTags");
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
    tags: [{ id: "tag-1", name: "Fiction", confirmed: true }],
    formats: [{ id: "format-1", format: "epub", size_bytes: 1024 }],
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

function makeLlmHealth(enabled: boolean): LlmHealth {
  return {
    enabled,
    librarian: {
      available: enabled,
      model_id: enabled ? "librarian-v1" : null,
      endpoint: "http://llm.local",
    },
  };
}

function makeClassifyResult(): ClassifyResult {
  return {
    book_id: "book-1",
    suggestions: [{ name: "Space Opera", confidence: 0.92 }],
    model_id: "librarian-v1",
    pending_count: 1,
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

function makeUser(): User {
  return {
    id: "user-1",
    username: "admin",
    email: "admin@example.com",
    role: {
      id: "role-1",
      name: "admin",
      can_edit: true,
      can_bulk: true,
      can_upload: true,
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

describe("BookDetailLlm", () => {
  beforeEach(() => {
    getBookMock.mockReset();
    getLlmHealthMock.mockReset();
    classifyBookMock.mockReset();
    confirmTagsMock.mockReset();
    getBookCustomValuesMock.mockReset();

    window.history.replaceState({}, "", "/books/book-1");
    setUser(makeUser());

    getBookMock.mockResolvedValue(makeBook());
    getLlmHealthMock.mockResolvedValue(makeLlmHealth(true));
    getBookCustomValuesMock.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    setUser(null);
  });

  test("test_classify_shows_suggestions", async () => {
    classifyBookMock.mockResolvedValue(makeClassifyResult());

    renderPage();

    await screen.findByRole("heading", { name: "Dune" });
    fireEvent.click(await screen.findByRole("button", { name: "AI" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Classify" })[1] as HTMLButtonElement);

    expect(await screen.findByText("Space Opera (92%)")).toBeTruthy();
  });

  test("test_confirm_tag_calls_api", async () => {
    const result = makeClassifyResult();
    classifyBookMock.mockResolvedValue(result);
    confirmTagsMock.mockResolvedValue(makeBook());

    renderPage();

    await screen.findByRole("heading", { name: "Dune" });
    fireEvent.click(await screen.findByRole("button", { name: "AI" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Classify" })[1] as HTMLButtonElement);
    await screen.findByText("Space Opera (92%)");

    fireEvent.click(screen.getByRole("button", { name: "Confirm Space Opera" }));

    await waitFor(() => {
      expect(confirmTagsMock).toHaveBeenCalledWith("book-1", ["Space Opera"], []);
    });
  });

  test("test_llm_panel_hidden_when_disabled", async () => {
    getLlmHealthMock.mockResolvedValue(makeLlmHealth(false));

    renderPage();

    await screen.findByRole("heading", { name: "Dune" });
    expect(screen.queryByRole("button", { name: "AI" })).toBeNull();
  });
});
