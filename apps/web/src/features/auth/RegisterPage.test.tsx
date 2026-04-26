import { beforeEach, describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { server } from "../../test/setup";
import { useAuthStore } from "../../lib/auth-store";
import { screen, waitFor } from "@testing-library/react";

function renderRegisterPage() {
  return renderWithProviders(<></>, {
    initialPath: "/register",
    authenticated: false,
  });
}

describe("RegisterPage", () => {
  beforeEach(() => {
    useAuthStore.getState().clearAuth();
  });

  test("renders the registration fields", async () => {
    renderRegisterPage();

    expect(await screen.findByLabelText(/username/i)).toBeTruthy();
    expect(screen.getByLabelText(/email/i)).toBeTruthy();
    expect(screen.getByLabelText(/password/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: /create account/i })).toBeTruthy();
  });

  test("successful registration navigates to /library", async () => {
    const user = userEvent.setup();
    const { router } = renderRegisterPage();

    await user.type(await screen.findByLabelText(/username/i), "admin");
    await user.type(screen.getByLabelText(/email/i), "admin@example.com");
    await user.type(screen.getByLabelText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /create account/i }));

    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/library");
    });
    expect(useAuthStore.getState().access_token).toBe("tok");
  });

  test("duplicate username shows the API error", async () => {
    server.use(
      http.post("/api/v1/auth/register", () =>
        HttpResponse.json({ message: "username exists" }, { status: 409 }),
      ),
    );

    const user = userEvent.setup();
    renderRegisterPage();

    await user.type(await screen.findByLabelText(/username/i), "admin");
    await user.type(screen.getByLabelText(/email/i), "admin@example.com");
    await user.type(screen.getByLabelText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /create account/i }));

    expect(await screen.findByText(/account already exists/i)).toBeTruthy();
  });
});
