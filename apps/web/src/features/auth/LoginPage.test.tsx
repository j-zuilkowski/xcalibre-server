import { beforeEach, describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { server } from "../../test/setup";
import { useAuthStore } from "../../lib/auth-store";
import { makeAuthSession } from "../../test/fixtures";
import { screen, waitFor } from "@testing-library/react";

function renderLoginPage() {
  return renderWithProviders(<></>, {
    initialPath: "/login",
    authenticated: false,
  });
}

describe("LoginPage", () => {
  beforeEach(() => {
    useAuthStore.getState().clearAuth();
  });

  test("renders username and password fields", async () => {
    renderLoginPage();

    expect(await screen.findByLabelText(/username/i)).toBeTruthy();
    expect(screen.getByLabelText(/password/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: /sign in/i })).toBeTruthy();
  });

  test("renders Google oauth button when provider is enabled", async () => {
    server.use(http.get("/api/v1/auth/providers", () => HttpResponse.json({ google: true, github: false })));

    renderLoginPage();

    expect(await screen.findByRole("link", { name: /sign in with google/i })).toBeTruthy();
  });

  test("hides oauth buttons when none are enabled", async () => {
    renderLoginPage();

    await waitFor(() => {
      expect(screen.queryByRole("link", { name: /sign in with google/i })).toBeNull();
      expect(screen.queryByRole("link", { name: /sign in with github/i })).toBeNull();
    });
  });

  test("successful login stores token and navigates to /library", async () => {
    const user = userEvent.setup();
    const { router } = renderLoginPage();

    await user.type(await screen.findByLabelText(/username/i), "reader");
    await user.type(screen.getByLabelText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /sign in/i }));

    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/library");
    });
    expect(useAuthStore.getState().access_token).toBe("tok");
    expect(useAuthStore.getState().refresh_token).toBe("rtok");
  });

  test("invalid credentials show an error", async () => {
    server.use(
      http.post("/api/v1/auth/login", () =>
        HttpResponse.json({ message: "invalid" }, { status: 401 }),
      ),
    );

    const user = userEvent.setup();
    renderLoginPage();

    await user.type(await screen.findByLabelText(/username/i), "reader");
    await user.type(screen.getByLabelText(/password/i), "wrong");
    await user.click(screen.getByRole("button", { name: /sign in/i }));

    expect(await screen.findByText(/invalid username or password/i)).toBeTruthy();
  });

  test("totp required response transitions to the verification step", async () => {
    server.use(
      http.post("/api/v1/auth/login", () =>
        HttpResponse.json({ totp_required: true, totp_token: "totp-token" }),
      ),
    );

    const user = userEvent.setup();
    renderLoginPage();

    await user.type(await screen.findByLabelText(/username/i), "totp");
    await user.type(screen.getByLabelText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /sign in/i }));

    const codeInput = await screen.findByLabelText(/^code$/i);
    expect(codeInput).toHaveFocus();
  });

  test("successful totp verification navigates to /library", async () => {
    server.use(
      http.post("/api/v1/auth/login", () =>
        HttpResponse.json({ totp_required: true, totp_token: "totp-token" }),
      ),
      http.post("/api/v1/auth/totp/verify", () => HttpResponse.json(makeAuthSession())),
    );

    const user = userEvent.setup();
    const { router } = renderLoginPage();

    await user.type(await screen.findByLabelText(/username/i), "totp");
    await user.type(screen.getByLabelText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /sign in/i }));

    const codeInput = await screen.findByLabelText(/^code$/i);
    await user.type(codeInput, "123456");
    await user.click(screen.getByRole("button", { name: /^verify$/i }));

    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/library");
    });
  });

  test("wrong totp code shows an error", async () => {
    server.use(
      http.post("/api/v1/auth/login", () =>
        HttpResponse.json({ totp_required: true, totp_token: "totp-token" }),
      ),
      http.post("/api/v1/auth/totp/verify", () => HttpResponse.json({ message: "invalid" }, { status: 401 })),
    );

    const user = userEvent.setup();
    renderLoginPage();

    await user.type(await screen.findByLabelText(/username/i), "totp");
    await user.type(screen.getByLabelText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /sign in/i }));

    const codeInput = await screen.findByLabelText(/^code$/i);
    await user.type(codeInput, "123456");
    await user.click(screen.getByRole("button", { name: /^verify$/i }));

    expect(await screen.findByText(/invalid code/i)).toBeTruthy();
  });

  test("backup code mode uses the backup endpoint", async () => {
    let backupCalled = false;
    server.use(
      http.post("/api/v1/auth/login", () =>
        HttpResponse.json({ totp_required: true, totp_token: "totp-token" }),
      ),
      http.post("/api/v1/auth/totp/verify-backup", () => {
        backupCalled = true;
        return HttpResponse.json(makeAuthSession());
      }),
    );

    const user = userEvent.setup();
    renderLoginPage();

    await user.type(await screen.findByLabelText(/username/i), "totp");
    await user.type(screen.getByLabelText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /sign in/i }));

    await user.click(await screen.findByRole("button", { name: /use a backup code instead/i }));
    const backupInput = await screen.findByLabelText(/backup code/i);
    expect(backupInput).toHaveAttribute("maxlength", "8");

    await user.type(backupInput, "ABCDEFGH");
    await user.click(screen.getByRole("button", { name: /^verify$/i }));

    await waitFor(() => {
      expect(backupCalled).toBe(true);
    });
  });

  test("TOTP input autofocuses after the step change", async () => {
    server.use(
      http.post("/api/v1/auth/login", () =>
        HttpResponse.json({ totp_required: true, totp_token: "totp-token" }),
      ),
    );

    const user = userEvent.setup();
    renderLoginPage();

    await user.type(await screen.findByLabelText(/username/i), "totp");
    await user.type(screen.getByLabelText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /sign in/i }));

    expect(await screen.findByLabelText(/^code$/i)).toHaveFocus();
  });
});
