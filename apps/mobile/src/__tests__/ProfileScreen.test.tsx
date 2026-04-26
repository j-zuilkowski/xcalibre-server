import React from "react";
import TestRenderer, { act, type ReactTestRenderer } from "react-test-renderer";
import * as SecureStore from "expo-secure-store";
import ProfileScreen from "../app/(tabs)/profile";

const {
  mockGetMe,
  mockGetUserStats,
  mockReplace,
  mockClearTokens,
  mockGetApiBaseUrl,
  mockQueryData,
} = vi.hoisted(() => ({
  mockGetMe: vi.fn(),
  mockGetUserStats: vi.fn(),
  mockReplace: vi.fn(),
  mockClearTokens: vi.fn(),
  mockGetApiBaseUrl: vi.fn(),
  mockQueryData: {
    data: null as
      | null
      | {
          username: string;
          email: string;
          default_library_id: string;
          totp_enabled: boolean;
        },
  },
}));

vi.mock("@tanstack/react-query", () => ({
  useQuery: (options: { queryFn?: () => unknown }) => {
    void options.queryFn?.();
    return {
      data: mockQueryData.data,
      isLoading: false,
      isError: false,
    };
  },
}));

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    useApi: () => ({
      getMe: mockGetMe,
      getUserStats: mockGetUserStats,
    }),
    getApiBaseUrl: mockGetApiBaseUrl,
  };
});

vi.mock("../lib/auth", () => ({
  clearTokens: mockClearTokens,
}));

vi.mock("expo-router", () => ({
  useRouter: () => ({
    replace: mockReplace,
  }),
}));

vi.mock("expo-constants", () => ({
  default: {
    expoConfig: {
      version: "1.0.0",
    },
  },
}));

async function flushPromises(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

function findByTestId(tree: ReactTestRenderer, testID: string) {
  return tree.root.find((node) => node.props.testID === testID);
}

function allText(tree: ReactTestRenderer): string[] {
  return tree.root
    .findAll(() => true)
    .flatMap((node) => node.children.filter((child): child is string => typeof child === "string"));
}

describe("ProfileScreen", () => {
  beforeEach(() => {
    mockGetMe.mockReset();
    mockGetUserStats.mockReset();
    mockReplace.mockReset();
    mockClearTokens.mockReset();
    mockGetApiBaseUrl.mockReset();
    mockGetApiBaseUrl.mockResolvedValue("http://localhost:8080");
    mockGetMe.mockResolvedValue({
      username: "reader",
      email: "reader@example.com",
      default_library_id: "default",
      totp_enabled: false,
    });
    mockGetUserStats.mockResolvedValue({
      total_books_read: 12,
      books_read_this_year: 8,
      books_read_this_month: 3,
      books_in_progress: 2,
      total_reading_sessions: 44,
      reading_streak_days: 7,
      longest_streak_days: 19,
      average_progress_per_session: 0.42,
      formats_read: { EPUB: 10, PDF: 2 },
      top_tags: [],
      top_authors: [],
      monthly_books: [],
    });
    mockClearTokens.mockResolvedValue(undefined);
    mockQueryData.data = {
      username: "reader",
      email: "reader@example.com",
      default_library_id: "default",
      totp_enabled: false,
    };
  });

  it("test_profile_shows_username", async () => {
    let tree!: ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(<ProfileScreen />);
    });

    await flushPromises();

    expect(allText(tree)).toContain("reader");
  });

  it("test_signout_clears_tokens_and_navigates", async () => {
    let tree!: ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(<ProfileScreen />);
    });

    await flushPromises();

    await act(async () => {
      findByTestId(tree, "sign-out").props.onPress();
    });

    await flushPromises();

    expect(mockClearTokens).toHaveBeenCalled();
    expect(mockReplace).toHaveBeenCalledWith("/login");
  });

  it("test_server_url_saves_to_secure_store", async () => {
    let tree!: ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(<ProfileScreen />);
    });

    await flushPromises();

    await act(async () => {
      findByTestId(tree, "server-url-input").props.onChangeText("http://example.test");
    });

    await flushPromises();

    await act(async () => {
      findByTestId(tree, "server-url-save").props.onPress();
    });

    await flushPromises();

    expect(vi.mocked(SecureStore.setItemAsync)).toHaveBeenCalledWith(
      "server_url",
      "http://example.test",
    );
  });
});
