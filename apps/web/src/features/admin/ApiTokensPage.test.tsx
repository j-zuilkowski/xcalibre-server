import { beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClientProvider } from "@tanstack/react-query";
import { HttpResponse, http } from "msw";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { I18nextProvider } from "react-i18next";
import { ApiTokensPage } from "./ApiTokensPage";
import i18n from "../../i18n";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";
import { makeAdminUser, makeApiToken, makeUser } from "../../test/fixtures";
import { makeTestQueryClient } from "../../test/query-client";
import { server } from "../../test/setup";

const listTokensMock = vi.spyOn(apiClient, "listApiTokens");

function renderPage(user = makeAdminUser()) {
  useAuthStore.setState({
    access_token: "test-token",
    refresh_token: "test-refresh",
    user,
  });

  const queryClient = makeTestQueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: Infinity,
      },
      mutations: {
        retry: 0,
      },
    },
  });

  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={queryClient}>
        <ApiTokensPage />
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe("ApiTokensPage", () => {
  beforeEach(() => {
    listTokensMock.mockReset();
    server.resetHandlers();
  });

  afterEach(() => {
    cleanup();
    useAuthStore.getState().clearAuth();
  });

  test("scope radio group renders with Read, Read-Write, and Admin options", async () => {
    listTokensMock.mockResolvedValue([]);

    renderPage();

    expect(await screen.findByRole("heading", { name: /api tokens/i })).toBeTruthy();
    const group = screen.getByRole("radiogroup", { name: /token scope/i });
    expect(group).toBeTruthy();
    expect(within(group).getByRole("radio", { name: /^read$/i })).toBeTruthy();
    expect(within(group).getByRole("radio", { name: /^read-write$/i })).toBeTruthy();
    expect(within(group).getByRole("radio", { name: /^admin$/i })).toBeTruthy();
    expect(screen.getByText(/query books and metadata only/i)).toBeTruthy();
    expect(screen.getByText(/full access to user resources/i)).toBeTruthy();
    expect(screen.getByText(/admin panel access/i)).toBeTruthy();
  });

  test("Admin option is disabled when current user is not an admin", async () => {
    listTokensMock.mockResolvedValue([]);

    renderPage(makeUser());

    const adminOption = await screen.findByRole("radio", { name: /^admin$/i });
    expect(adminOption).toBeDisabled();
  });

  test('submitting the form with Read selected calls API with scope: "read"', async () => {
    listTokensMock.mockResolvedValue([]);
    let requestBody: Record<string, unknown> | null = null;

    server.use(
      http.post("/api/v1/admin/tokens", async ({ request }) => {
        requestBody = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(
          {
            id: "token-1",
            name: "Reader",
            token: "plain-token",
            created_at: "2026-04-19T00:00:00Z",
            scope: "read",
          },
          { status: 201 },
        );
      }),
    );

    const user = userEvent.setup();
    renderPage();

    await user.type(await screen.findByLabelText(/token name/i), "Reader");
    await user.click(screen.getByRole("radio", { name: /^read$/i }));
    await user.click(screen.getByRole("button", { name: /create token/i }));

    await waitFor(() => {
      expect(requestBody).toMatchObject({
        name: "Reader",
        scope: "read",
      });
    });
    expect(screen.getByText("plain-token")).toBeTruthy();
  });

  test("token list displays a scope badge for each token", async () => {
    listTokensMock.mockResolvedValue([
      makeApiToken({ id: "token-1", name: "Reader token", scope: "read" }),
      makeApiToken({ id: "token-2", name: "Admin token", scope: "admin" }),
    ]);

    renderPage();

    const readerRow = (await screen.findByText("Reader token")).closest("tr");
    const adminRow = screen.getByText("Admin token").closest("tr");
    expect(readerRow).toBeTruthy();
    expect(adminRow).toBeTruthy();

    expect(within(readerRow as HTMLElement).getByText(/^read$/i)).toBeTruthy();
    expect(within(adminRow as HTMLElement).getByText(/^admin$/i)).toBeTruthy();
  });
});
