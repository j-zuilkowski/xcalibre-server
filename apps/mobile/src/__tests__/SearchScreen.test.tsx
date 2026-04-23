import React from "react";
import TestRenderer, { act, type ReactTestRenderer } from "react-test-renderer";
import { QueryClientProvider } from "@tanstack/react-query";
import { Alert } from "react-native";
import SearchScreen from "../app/(tabs)/search";
import { queryClient } from "../lib/query-client";

const {
  mockClient,
  mockPush,
} = vi.hoisted(() => ({
  mockClient: {
    search: vi.fn(),
    getLlmHealth: vi.fn(),
    coverUrl: vi.fn((id: string) => `http://example.test/books/${id}/cover`),
  },
  mockPush: vi.fn(),
}));

vi.mock("expo-router", () => ({
  useRouter: () => ({
    push: mockPush,
  }),
}));

vi.mock("../lib/api", () => ({
  useApi: () => mockClient,
}));

vi.mock("@expo/vector-icons", () => ({
  Ionicons: ({ name }: { name: string }) => React.createElement("Text", null, name),
}));

vi.mock("expo-image", () => ({
  Image: "Image",
}));

function renderScreen() {
  return TestRenderer.create(
    <QueryClientProvider client={queryClient}>
      <SearchScreen />
    </QueryClientProvider>,
  );
}

async function flushTimers() {
  await act(async () => {
    await Promise.resolve();
  });
}

async function flushSearchTimers() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1);
    await Promise.resolve();
  });
}

async function findEventually(
  tree: ReactTestRenderer,
  predicate: (node: { props: Record<string, any> }) => boolean,
): Promise<{ props: Record<string, any> }> {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const matches = tree.root.findAll((node) => predicate(node as { props: Record<string, any> }));
    if (matches.length > 0) {
      return matches[0] as { props: Record<string, any> };
    }

    await flushSearchTimers();
  }

  throw new Error("Timed out waiting for node");
}

describe("SearchScreen", () => {
  beforeEach(() => {
    queryClient.clear();
    mockClient.search.mockReset();
    mockClient.getLlmHealth.mockReset();
    mockPush.mockReset();
    vi.spyOn(Alert, "alert").mockImplementation(() => undefined);
    mockClient.getLlmHealth.mockResolvedValue({ enabled: true });
    mockClient.search.mockResolvedValue({
      items: [],
      total: 0,
      page: 1,
      page_size: 20,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  test("test_search_input_triggers_api_call_after_debounce", async () => {
    vi.useFakeTimers();
    const tree = renderScreen();

    await act(async () => {
      const input = tree.root.find((node) => node.props.testID === "search-input");
      input.props.onChangeText("dune");
    });

    expect(mockClient.search).not.toHaveBeenCalled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    await flushTimers();

    expect(mockClient.search).toHaveBeenCalledWith({
      q: "dune",
      language: undefined,
      format: undefined,
      sort: "title",
      order: "asc",
      page: 1,
      page_size: 20,
    });

    tree.unmount();
    vi.useRealTimers();
  });

  test("test_empty_query_shows_prompt_not_results", async () => {
    const tree = renderScreen();

    await flushTimers();

    expect(tree.root.findByProps({ children: "Enter a search term" })).toBeTruthy();
    expect(mockClient.search).not.toHaveBeenCalled();

    tree.unmount();
  });

  test("test_semantic_tab_grayed_when_llm_disabled", async () => {
    mockClient.getLlmHealth.mockResolvedValue({ enabled: false });

    const tree = renderScreen();
    await flushTimers();

    const semanticTab = tree.root.find((node) => node.props.testID === "search-tab-semantic");
    expect(semanticTab.props.accessibilityState).toEqual({ disabled: true });
    expect(String(semanticTab.props.className)).toContain("opacity-40");

    tree.unmount();
  });

  test("test_result_card_navigates_to_book_detail", async () => {
    vi.useFakeTimers();
    mockClient.search.mockResolvedValue({
      items: [
        {
          id: "book-1",
          title: "Dune",
          sort_title: "Dune",
          authors: [{ id: "author-1", name: "Frank Herbert", sort_name: "Herbert, Frank" }],
          series: null,
          series_index: null,
          cover_url: null,
          has_cover: false,
          is_read: false,
          is_archived: false,
          language: "en",
          rating: 9,
          document_type: "novel",
          last_modified: "2026-01-01T00:00:00Z",
        },
      ],
      total: 1,
      page: 1,
      page_size: 20,
    });

    const tree = renderScreen();

    await act(async () => {
      const input = tree.root.find((node) => node.props.testID === "search-input");
      input.props.onChangeText("dune");
      await vi.advanceTimersByTimeAsync(300);
    });
    await flushTimers();

    const card = await findEventually(tree, (node) => node.props.testID === "search-result-book-1");

    await act(async () => {
      card.props.onPress();
    });

    expect(mockPush).toHaveBeenCalledWith({
      pathname: "/book/[id]",
      params: { id: "book-1" },
    });

    tree.unmount();
    vi.useRealTimers();
  });

  test("test_pagination_next_increments_page", async () => {
    vi.useFakeTimers();
    mockClient.search.mockResolvedValue({
      items: [
        {
          id: "book-1",
          title: "Dune",
          sort_title: "Dune",
          authors: [{ id: "author-1", name: "Frank Herbert", sort_name: "Herbert, Frank" }],
          series: null,
          series_index: null,
          cover_url: null,
          has_cover: false,
          is_read: false,
          is_archived: false,
          language: "en",
          rating: 9,
          document_type: "novel",
          last_modified: "2026-01-01T00:00:00Z",
        },
      ],
      total: 40,
      page: 1,
      page_size: 20,
    });

    const tree = renderScreen();

    await act(async () => {
      const input = tree.root.find((node) => node.props.testID === "search-input");
      input.props.onChangeText("dune");
      await vi.advanceTimersByTimeAsync(300);
    });
    await flushTimers();

    const nextButton = await findEventually(tree, (node) => node.props.testID === "search-pagination-next");

    await act(async () => {
      nextButton.props.onPress();
    });

    await flushTimers();

    expect(mockClient.search).toHaveBeenLastCalledWith(
      expect.objectContaining({
        page: 2,
        page_size: 20,
      }),
    );

    tree.unmount();
    vi.useRealTimers();
  });
});
