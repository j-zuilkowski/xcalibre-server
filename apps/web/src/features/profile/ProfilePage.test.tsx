import { describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { makeUser } from "../../test/fixtures";
import { server } from "../../test/setup";
import { screen, waitFor } from "@testing-library/react";

function renderProfilePage() {
  return renderWithProviders(<></>, {
    initialPath: "/profile",
    authenticated: true,
  });
}

describe("ProfilePage", () => {
  test("renders the current username and email", async () => {
    renderProfilePage();

    expect(await screen.findByText("u")).toBeTruthy();
    expect(screen.getByText("u@x.io")).toBeTruthy();
  });

  test("enabling two-factor authentication shows the QR code and backup codes", async () => {
    const user = userEvent.setup();
    renderProfilePage();

    await user.click(await screen.findByRole("button", { name: /enable two-factor authentication/i }));

    expect(await screen.findByText(/manual entry code/i)).toBeTruthy();
    expect(screen.getByText("JBSWY3DPEHPK3PXP")).toBeTruthy();

    await user.type(screen.getByLabelText(/confirmation code/i), "123456");
    await user.click(screen.getByRole("button", { name: /confirm/i }));

    expect(await screen.findByText(/save these backup codes/i)).toBeTruthy();
    expect(screen.getByText("ABC12345")).toBeTruthy();
  });

  test("disabling two-factor authentication calls the endpoint", async () => {
    server.use(
      http.get("/api/v1/auth/me", () => HttpResponse.json(makeUser({ totp_enabled: true }))),
    );

    let disableCalled = false;
    server.use(
      http.post("/api/v1/auth/totp/disable", () => {
        disableCalled = true;
        return HttpResponse.json(null, { status: 204 });
      }),
    );

    const user = userEvent.setup();
    renderProfilePage();

    await user.click(await screen.findByRole("button", { name: /disable 2fa/i }));
    await user.type(screen.getByPlaceholderText(/password/i), "secret");
    await user.click(screen.getByRole("button", { name: /^disable$/i }));

    await waitFor(() => {
      expect(disableCalled).toBe(true);
    });
  });
});
