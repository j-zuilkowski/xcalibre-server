import React from "react";
import TestRenderer, { act } from "react-test-renderer";
import LibraryScreen, { LIBRARY_QUERY_KEY } from "../app/(tabs)/library";

const {
  mockInvalidateQueries,
  mockListBooks,
  mockClient,
  queryState,
} = vi.hoisted(() => ({
  mockListBooks: vi.fn(),
  mockInvalidateQueries: vi.fn(),
  mockClient: {
    listBooks: vi.fn(),
  },
  queryState: {
    isOffline: false,
    onlineQuery: {
      data: {
        pages: [],
        pageParams: [1],
      },
      isLoading: false,
      isRefetching: false,
      isFetchingNextPage: false,
      hasNextPage: false,
      fetchNextPage: vi.fn(),
    },
    localQuery: {
      data: [] as Array<Record<string, unknown>>,
      isLoading: false,
      isRefetching: false,
      refetch: vi.fn(),
    },
  },
}));

vi.mock("@tanstack/react-query", () => ({
  useInfiniteQuery: () => queryState.onlineQuery,
  useQuery: () => queryState.localQuery,
  useQueryClient: () => ({
    invalidateQueries: mockInvalidateQueries,
  }),
}));

vi.mock("@react-native-community/netinfo", () => ({
  useNetInfo: () => ({
    type: queryState.isOffline ? "none" : "wifi",
    isConnected: !queryState.isOffline,
    isInternetReachable: !queryState.isOffline,
  }),
}));

vi.mock("expo-router", () => ({
  Stack: {
    Screen: () => null,
  },
}));

vi.mock("../lib/api", () => ({
  useApi: () => mockClient,
}));

vi.mock("../lib/sync", () => ({
  syncLibrary: vi.fn().mockResolvedValue({
    synced: 0,
    total: 0,
  }),
}));

vi.mock("../components/BookCard", () => ({
  BookCard: ({ book }: { book: { title: string } }) => React.createElement("Text", null, book.title),
}));

vi.mock("@expo/vector-icons", () => ({
  Ionicons: () => React.createElement("Text", null, "icon"),
}));

function setOnlineBooks(page: {
  items: Array<Record<string, unknown>>;
  total: number;
  page: number;
  page_size: number;
}): void {
  queryState.isOffline = false;
  queryState.onlineQuery = {
    data: {
      pages: [page],
      pageParams: [1],
    },
    isLoading: false,
    isRefetching: false,
    isFetchingNextPage: false,
    hasNextPage: false,
    fetchNextPage: vi.fn(),
  };
}

function setOfflineBooks(items: Array<Record<string, unknown>>): void {
  queryState.isOffline = true;
  queryState.localQuery = {
    data: items,
    isLoading: false,
    isRefetching: false,
    refetch: vi.fn(),
  };
}

describe("LibraryScreen", () => {
  beforeEach(() => {
    mockListBooks.mockReset();
    mockInvalidateQueries.mockReset();
    queryState.isOffline = false;
    queryState.onlineQuery.fetchNextPage = vi.fn();
    queryState.localQuery.refetch = vi.fn();
  });

  it("test_library_renders_book_cards", async () => {
    setOnlineBooks({
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
      tree = TestRenderer.create(<LibraryScreen />);
    });

    const list = tree.root.find((node) => node.props.testID === "library-list");
    expect(list.props.data.map((book: { title: string }) => book.title)).toEqual([
      "Book One",
      "Book Two",
    ]);

    tree.unmount();
  });

  it("test_library_pull_to_refresh", async () => {
    setOnlineBooks({
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

    let tree!: TestRenderer.ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(<LibraryScreen />);
    });

    const list = tree.root.find((node) => node.props.testID === "library-list");

    await act(async () => {
      list.props.onRefresh();
    });

    expect(mockInvalidateQueries).toHaveBeenCalledWith({ queryKey: LIBRARY_QUERY_KEY });

    tree.unmount();
  });

  it("test_empty_library_shows_state", async () => {
    setOnlineBooks({
      items: [],
      total: 0,
      page: 1,
      page_size: 30,
    });

    let tree!: TestRenderer.ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(<LibraryScreen />);
    });

    const emptyState = tree.root.find((node) => node.props.testID === "library-empty-state");
    expect(emptyState).toBeTruthy();

    tree.unmount();
  });
});
