import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { AdminUser, Role } from "@xs/shared";
import { UsersPage } from "../features/admin/UsersPage";
import { apiClient } from "../lib/api-client";
import { makeTestQueryClient } from "../test/query-client";

const listUsersMock = vi.spyOn(apiClient, "listUsers");
const listRolesMock = vi.spyOn(apiClient, "listRoles");
const createUserMock = vi.spyOn(apiClient, "createUser");
const updateUserMock = vi.spyOn(apiClient, "updateUser");

function makeRole(id: string, name: string): Role {
  return {
    id,
    name,
    can_upload: true,
    can_bulk: false,
    can_edit: true,
    can_download: true,
  };
}

function makeUser(id: string, username: string, role: Role): AdminUser {
  return {
    id,
    username,
    email: `${username}@example.com`,
    role,
    is_active: true,
    force_pw_reset: false,
    default_library_id: "default",
    totp_enabled: false,
    created_at: "2026-04-18T00:00:00Z",
    last_modified: "2026-04-19T00:00:00Z",
    last_login_at: "2026-04-19T00:00:00Z",
  };
}

function renderPage() {
  const queryClient = makeTestQueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: Infinity,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <UsersPage />
    </QueryClientProvider>,
  );
}

describe("AdminUsersPage", () => {
  beforeEach(() => {
    listUsersMock.mockReset();
    listRolesMock.mockReset();
    createUserMock.mockReset();
    updateUserMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  test("test_users_page_shows_user_row", async () => {
    const userRole = makeRole("role-1", "user");
    listRolesMock.mockResolvedValue([userRole]);
    listUsersMock.mockResolvedValue([makeUser("user-1", "harry", userRole)]);

    renderPage();

    const username = await screen.findByText("harry");
    const row = username.closest("tr");
    expect(row?.textContent ?? "").toContain("2026");
  });

  test("test_inline_save_updates_user", async () => {
    const userRole = makeRole("role-1", "user");
    const librarianRole = makeRole("role-2", "librarian");
    listRolesMock.mockResolvedValue([userRole, librarianRole]);
    listUsersMock.mockResolvedValue([makeUser("user-1", "harry", userRole)]);
    updateUserMock.mockResolvedValue(makeUser("user-1", "harry", librarianRole));

    renderPage();

    const row = await screen.findByText("harry");
    const tableRow = row.closest("tr");
    expect(tableRow).toBeTruthy();
    const scoped = within(tableRow as HTMLElement);

    fireEvent.change(scoped.getByRole("combobox"), { target: { value: "role-2" } });
    fireEvent.click(scoped.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(updateUserMock).toHaveBeenCalledWith("user-1", {
        role_id: "role-2",
        is_active: true,
        force_pw_reset: false,
      });
    });
  });

  test("test_create_user_submits_new_user", async () => {
    const userRole = makeRole("role-1", "user");
    listRolesMock.mockResolvedValue([userRole]);
    listUsersMock.mockResolvedValue([]);
    createUserMock.mockResolvedValue(makeUser("user-9", "new-user", userRole));

    renderPage();

    await screen.findByText("Create user");

    fireEvent.change(screen.getByPlaceholderText("Username"), { target: { value: "new-user" } });
    fireEvent.change(screen.getByPlaceholderText("Email"), { target: { value: "new-user@example.com" } });
    fireEvent.change(screen.getByPlaceholderText("Password"), { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => {
      expect(createUserMock).toHaveBeenCalledWith({
        username: "new-user",
        email: "new-user@example.com",
        password: "secret",
        role_id: "role-1",
        is_active: true,
      });
    });
  });
});
