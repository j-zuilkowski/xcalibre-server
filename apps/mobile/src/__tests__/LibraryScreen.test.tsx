import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import TestRenderer, { act } from "react-test-renderer";
import LibraryScreen, { LIBRARY_QUERY_KEY } from "../app/(tabs)/library";

const { mockListBooks } = vi.hoisted(() => ({
  mockListBooks: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  useApi: () => ({
    listBooks: mockListBooks,
  }),
}));

vi.mock("../components/BookCard", () => ({
  BookCard: ({ book }: { book: { title: string } }) => React.createElement("Text", null, book.title),
}));

vi.mock("@expo/vector-icons", () => ({
  Ionicons: () => React.createElement("Text", null, "icon"),
}));

function createTestClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
      mutations: {
        retry: false,
      },
    },
  });
}

function seedLibrary(
  queryClient: QueryClient,
  page: {
    items: Array<Record<string, unknown>>;
    total: number;
    page: number;
    page_size: number;
  },
): void {
  queryClient.setQueryData(LIBRARY_QUERY_KEY, {
    pages: [page],
    pageParams: [1],
  });
}

describe("LibraryScreen", () => {
  beforeEach(() => {
    mockListBooks.mockReset();
  });

  it("test_library_renders_book_cards", async () => {
    mockListBooks.mockResolvedValue({
      items: [
        {
          id: "1",
          title: "Book One",
          sort_title: "Book One",
          authors: [{ id: "a1", name: "Author 1", sort_name: "Author 1" }],
          series: null,
          series_index: null,
          cover_url: null,
          has_cover: false,
          language: "en",
          rating: 8,
          last_modified: "2026-01-01T00:00:00Z",
        },
        {
          id: "2",
          title: "Book Two",
          sort_title: "Book Two",
          authors: [{ id: "a2", name: "Author 2", sort_name: "Author 2" }],
          series: null,
          series_index: null,
          cover_url: null,
          has_cover: false,
          language: "en",
          rating: 9,
          last_modified: "2026-01-01T00:00:00Z",
        },
      ],
      total: 2,
      page: 1,
      page_size: 30,
    });

    const queryClient = createTestClient();
    seedLibrary(queryClient, {
      items: [
        {
          id: "1",
          title: "Book One",
          sort_title: "Book One",
          authors: [{ id: "a1", name: "Author 1", sort_name: "Author 1" }],
          series: null,
          series_index: null,
          cover_url: null,
          has_cover: false,
          language: "en",
          rating: 8,
          last_modified: "2026-01-01T00:00:00Z",
        },
        {
          id: "2",
          title: "Book Two",
          sort_title: "Book Two",
          authors: [{ id: "a2", name: "Author 2", sort_name: "Author 2" }],
          series: null,
          series_index: null,
          cover_url: null,
          has_cover: false,
          language: "en",
          rating: 9,
          last_modified: "2026-01-01T00:00:00Z",
        },
      ],
      total: 2,
      page: 1,
      page_size: 30,
    });
    let tree!: TestRenderer.ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(
        <QueryClientProvider client={queryClient}>
          <LibraryScreen />
        </QueryClientProvider>,
      );
    });

    const list = tree.root.find((node) => node.props.testID === "library-list");
    expect(list.props.data.map((book: { title: string }) => book.title)).toEqual([
      "Book One",
      "Book Two",
    ]);
  });

  it("test_library_pull_to_refresh", async () => {
    mockListBooks.mockResolvedValue({
      items: [
        {
          id: "1",
          title: "Book One",
          sort_title: "Book One",
          authors: [{ id: "a1", name: "Author 1", sort_name: "Author 1" }],
          series: null,
          series_index: null,
          cover_url: null,
          has_cover: false,
          language: "en",
          rating: 8,
          last_modified: "2026-01-01T00:00:00Z",
        },
      ],
      total: 1,
      page: 1,
      page_size: 30,
    });

    const queryClient = createTestClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    let tree!: TestRenderer.ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(
        <QueryClientProvider client={queryClient}>
          <LibraryScreen />
        </QueryClientProvider>,
      );
    });

    const list = tree.root.find((node) => node.props.testID === "library-list");

    await act(async () => {
      list.props.onRefresh();
    });

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: LIBRARY_QUERY_KEY });
  });

  it("test_empty_library_shows_state", async () => {
    mockListBooks.mockResolvedValue({
      items: [],
      total: 0,
      page: 1,
      page_size: 30,
    });

    const queryClient = createTestClient();
    seedLibrary(queryClient, {
      items: [],
      total: 0,
      page: 1,
      page_size: 30,
    });
    let tree!: TestRenderer.ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(
        <QueryClientProvider client={queryClient}>
          <LibraryScreen />
        </QueryClientProvider>,
      );
    });

    const emptyState = tree.root.find((node) => node.props.testID === "library-empty-state");
    expect(emptyState).toBeTruthy();
  });
});
