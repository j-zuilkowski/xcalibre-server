import { describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { makeAdminUser, makeUser, roleAdmin, roleUser } from "../../test/fixtures";
import { server } from "../../test/setup";
import { screen, waitFor } from "@testing-library/react";

function renderUsersPage() {
  return renderWithProviders(<></>, {
    initialPath: "/admin/users",
    authenticated: true,
    user: makeAdminUser(),
  });
}

describe("UsersPage", () => {
  test("renders the user list", async () => {
    renderUsersPage();

    expect(await screen.findByText("u")).toBeTruthy();
    expect(screen.getByText("u@x.io")).toBeTruthy();
  });

  test("create user submits the form", async () => {
    let createdUsername = "";
    server.use(
      http.post("/api/v1/admin/users", async ({ request }) => {
        const body = (await request.json()) as { username?: string };
        createdUsername = body.username ?? "";
        return HttpResponse.json(makeAdminUser({ id: "user-created", username: body.username ?? "new" }), {
          status: 201,
        });
      }),
    );

    const user = userEvent.setup();
    renderUsersPage();

    await user.type(await screen.findByLabelText(/username/i), "new-user");
    await user.type(screen.getByLabelText(/^email$/i), "new@example.com");
    await user.type(screen.getByLabelText(/^password$/i), "secret");
    await user.click(screen.getByRole("button", { name: /^create$/i }));

    await waitFor(() => {
      expect(createdUsername).toBe("new-user");
    });
  });

  test("changing a role and saving submits a patch", async () => {
    let patchBody: unknown = null;
    server.use(
      http.get("/api/v1/admin/roles", () => HttpResponse.json([roleAdmin, roleUser])),
      http.patch("/api/v1/admin/users/:id", async ({ request }) => {
        patchBody = await request.json();
        return HttpResponse.json(makeAdminUser({ role: roleUser }));
      }),
    );

    const user = userEvent.setup();
    renderUsersPage();

    const roleSelect = await screen.findByLabelText(/role for user u/i);
    await user.selectOptions(roleSelect, roleUser.id);
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(patchBody).toMatchObject({ role_id: roleUser.id });
    });
  });

  test("reset password and disable 2FA buttons call the right endpoints", async () => {
    let resetCalled = false;
    let disableCalled = false;
    server.use(
      http.get("/api/v1/admin/users", () => HttpResponse.json([makeAdminUser({ totp_enabled: true })])),
      http.post("/api/v1/admin/users/:id/reset-password", () => {
        resetCalled = true;
        return HttpResponse.json(null, { status: 204 });
      }),
      http.post("/api/v1/admin/users/:id/totp/disable", () => {
        disableCalled = true;
        return HttpResponse.json(null, { status: 204 });
      }),
    );

    const user = userEvent.setup();
    renderUsersPage();

    await user.click(await screen.findByRole("button", { name: /reset password/i }));
    await user.click(screen.getByRole("button", { name: /disable 2fa/i }));

    await waitFor(() => {
      expect(resetCalled).toBe(true);
      expect(disableCalled).toBe(true);
    });
  });

  test("delete user confirms before calling delete", async () => {
    let deletedPath = "";
    server.use(
      http.delete("/api/v1/admin/users/:id", ({ params }) => {
        deletedPath = String(params.id);
        return HttpResponse.json(null, { status: 204 });
      }),
    );

    const user = userEvent.setup();
    renderUsersPage();

    await user.click(await screen.findByRole("button", { name: /^delete$/i }));
    expect(await screen.findByRole("heading", { name: /delete user\?/i })).toBeTruthy();
    const dialogDeleteButton = screen.getAllByRole("button", { name: /^delete$/i }).at(-1);
    expect(dialogDeleteButton).toBeTruthy();
    await user.click(dialogDeleteButton as HTMLButtonElement);

    await waitFor(() => {
      expect(deletedPath).toBe("1");
    });
  });
});
