import React from "react";
import TestRenderer, { act, type ReactTestRenderer } from "react-test-renderer";
import LoginScreen from "../app/login";

const { mockLogin, mockSetApiBaseUrl, mockGetApiBaseUrl, mockSaveTokens, mockReplace } = vi.hoisted(
  () => ({
    mockLogin: vi.fn(),
    mockSetApiBaseUrl: vi.fn(),
    mockGetApiBaseUrl: vi.fn(),
    mockSaveTokens: vi.fn(),
    mockReplace: vi.fn(),
  }),
);

vi.mock("../lib/api", () => ({
  useApi: () => ({
    login: mockLogin,
  }),
  getApiBaseUrl: mockGetApiBaseUrl,
  setApiBaseUrl: mockSetApiBaseUrl,
}));

vi.mock("../lib/auth", () => ({
  saveTokens: mockSaveTokens,
}));

vi.mock("expo-router", () => ({
  useRouter: () => ({
    replace: mockReplace,
  }),
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

describe("LoginScreen", () => {
  beforeEach(() => {
    mockLogin.mockReset();
    mockSetApiBaseUrl.mockReset();
    mockGetApiBaseUrl.mockReset();
    mockSaveTokens.mockReset();
    mockReplace.mockReset();
    mockGetApiBaseUrl.mockResolvedValue("http://localhost:8080");
    mockSetApiBaseUrl.mockResolvedValue(undefined);
  });

  it("test_login_success_navigates", async () => {
    mockLogin.mockResolvedValue({
      access_token: "access-token",
      refresh_token: "refresh-token",
      user: {
        id: "1",
        username: "test",
        email: "test@example.com",
        role: { id: "role-1", name: "User" },
        is_active: true,
        force_pw_reset: false,
        default_library_id: "default",
        totp_enabled: false,
        created_at: "2026-01-01T00:00:00Z",
        last_modified: "2026-01-01T00:00:00Z",
      },
    });

    let tree!: ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(<LoginScreen />);
    });

    await flushPromises();

    await act(async () => {
      findByTestId(tree, "login-email").props.onChangeText("test@example.com");
      findByTestId(tree, "login-password").props.onChangeText("password123");
    });

    await flushPromises();

    await act(async () => {
      findByTestId(tree, "login-submit").props.onPress();
    });

    await flushPromises();

    expect(mockLogin).toHaveBeenCalledWith({
      username: "test@example.com",
      password: "password123",
    });
    expect(mockSaveTokens).toHaveBeenCalledWith("access-token", "refresh-token");
    expect(mockReplace).toHaveBeenCalledWith("/(tabs)/library");
  });

  it("test_login_error_shows_message", async () => {
    mockLogin.mockRejectedValue({
      status: 401,
      message: "Unauthorized",
    });

    let tree!: ReactTestRenderer;

    await act(async () => {
      tree = TestRenderer.create(<LoginScreen />);
    });

    await flushPromises();

    await act(async () => {
      findByTestId(tree, "login-email").props.onChangeText("wrong@example.com");
      findByTestId(tree, "login-password").props.onChangeText("bad-password");
    });

    await flushPromises();

    await act(async () => {
      findByTestId(tree, "login-submit").props.onPress();
    });

    await flushPromises();

    expect(allText(tree)).toContain("Invalid credentials.");
  });
});
