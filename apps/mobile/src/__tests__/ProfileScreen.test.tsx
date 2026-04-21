import React from "react";
import TestRenderer, { act } from "react-test-renderer";
import * as SecureStore from "expo-secure-store";
import ProfileScreen from "../app/(tabs)/profile";

const {
  mockGetMe,
  mockReplace,
  mockClearTokens,
  mockGetApiBaseUrl,
  mockQueryData,
} = vi.hoisted(() => ({
  mockGetMe: vi.fn(),
  mockReplace: vi.fn(),
  mockClearTokens: vi.fn(),
  mockGetApiBaseUrl: vi.fn(),
  mockQueryData: {
    data: null as null | { username: string; email: string },
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

function findByTestId(tree: TestRenderer.ReactTestRenderer, testID: string) {
  return tree.root.find((node) => node.props.testID === testID);
}

function allText(tree: TestRenderer.ReactTestRenderer): string[] {
  return tree.root
    .findAll(() => true)
    .flatMap((node) => node.children.filter((child): child is string => typeof child === "string"));
}

describe("ProfileScreen", () => {
  beforeEach(() => {
    mockGetMe.mockReset();
    mockReplace.mockReset();
    mockClearTokens.mockReset();
    mockGetApiBaseUrl.mockReset();
    mockGetApiBaseUrl.mockResolvedValue("http://localhost:8080");
    mockGetMe.mockResolvedValue({
      username: "reader",
      email: "reader@example.com",
    });
    mockClearTokens.mockResolvedValue(undefined);
    mockQueryData.data = {
      username: "reader",
      email: "reader@example.com",
    };
  });

  it("test_profile_shows_username", async () => {
    let tree!: TestRenderer.ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(<ProfileScreen />);
    });

    await flushPromises();

    expect(allText(tree)).toContain("reader");
  });

  it("test_signout_clears_tokens_and_navigates", async () => {
    let tree!: TestRenderer.ReactTestRenderer;

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
    let tree!: TestRenderer.ReactTestRenderer;

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
