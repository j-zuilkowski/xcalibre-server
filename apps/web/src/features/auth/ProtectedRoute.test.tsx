import { beforeEach, describe, expect, test } from "vitest";
import { renderWithProviders } from "../../test/render";
import { useAuthStore } from "../../lib/auth-store";
import { screen, waitFor } from "@testing-library/react";

describe("ProtectedRoute", () => {
  beforeEach(() => {
    useAuthStore.getState().clearAuth();
  });

  test("renders protected content when authenticated", async () => {
    renderWithProviders(<></>, {
      initialPath: "/library",
      authenticated: true,
    });

    expect(await screen.findByText(/no books in your library yet/i)).toBeTruthy();
  });

  test("redirects to /login when not authenticated", async () => {
    const { router } = renderWithProviders(<></>, {
      initialPath: "/library",
      authenticated: false,
    });

    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/login");
    });
    expect(await screen.findByRole("heading", { name: /welcome back/i })).toBeTruthy();
  });
});
