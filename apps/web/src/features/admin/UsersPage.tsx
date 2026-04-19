import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { AdminUser, Role } from "@calibre/shared";
import { apiClient } from "../../lib/api-client";
import { formatDateTime } from "./admin-utils";

type UserDraft = {
  role_id: string;
  is_active: boolean;
  force_pw_reset: boolean;
};

type CreateUserState = {
  username: string;
  email: string;
  password: string;
  role_id: string;
  is_active: boolean;
};

function roleLabel(user: AdminUser): string {
  return user.role.name;
}

function buildDraft(user: AdminUser): UserDraft {
  return {
    role_id: user.role.id,
    is_active: user.is_active,
    force_pw_reset: user.force_pw_reset,
  };
}

export function UsersPage() {
  const queryClient = useQueryClient();
  const [drafts, setDrafts] = useState<Record<string, UserDraft>>({});
  const [createForm, setCreateForm] = useState<CreateUserState>({
    username: "",
    email: "",
    password: "",
    role_id: "",
    is_active: true,
  });

  const usersQuery = useQuery({
    queryKey: ["admin-users"],
    queryFn: () => apiClient.listUsers(),
  });

  const rolesQuery = useQuery({
    queryKey: ["admin-roles"],
    queryFn: () => apiClient.listRoles(),
  });

  const roles = rolesQuery.data ?? [];
  const users = usersQuery.data ?? [];

  useEffect(() => {
    if (roles.length > 0 && !createForm.role_id) {
      setCreateForm((previous) => ({ ...previous, role_id: roles[0].id }));
    }
  }, [createForm.role_id, roles]);

  useEffect(() => {
    setDrafts((previous) => {
      const next = { ...previous };
      for (const user of users) {
        if (!next[user.id]) {
          next[user.id] = buildDraft(user);
        }
      }
      return next;
    });
  }, [users]);

  const createMutation = useMutation({
    mutationFn: (payload: Parameters<typeof apiClient.createUser>[0]) => apiClient.createUser(payload),
    onSuccess: async () => {
      setCreateForm({
        username: "",
        email: "",
        password: "",
        role_id: roles[0]?.id ?? "",
        is_active: true,
      });
      await queryClient.invalidateQueries({ queryKey: ["admin-users"] });
    },
  });

  const updateMutation = useMutation({
    mutationFn: (payload: { id: string; patch: Parameters<typeof apiClient.updateUser>[1] }) =>
      apiClient.updateUser(payload.id, payload.patch),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-users"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiClient.deleteUser(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-users"] });
    },
  });

  const resetMutation = useMutation({
    mutationFn: (id: string) => apiClient.resetUserPassword(id),
  });

  const roleById = useMemo(() => new Map(roles.map((role) => [role.id, role])), [roles]);

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">Users</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">User management</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <h3 className="text-lg font-semibold text-zinc-50">Create user</h3>
        <form
          className="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-5"
          onSubmit={(event) => {
            event.preventDefault();
            void createMutation.mutateAsync(createForm);
          }}
        >
          <input
            value={createForm.username}
            onChange={(event) => setCreateForm((previous) => ({ ...previous, username: event.target.value }))}
            placeholder="Username"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          />
          <input
            value={createForm.email}
            onChange={(event) => setCreateForm((previous) => ({ ...previous, email: event.target.value }))}
            placeholder="Email"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          />
          <input
            value={createForm.password}
            onChange={(event) => setCreateForm((previous) => ({ ...previous, password: event.target.value }))}
            placeholder="Password"
            type="password"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          />
          <select
            value={createForm.role_id}
            onChange={(event) => setCreateForm((previous) => ({ ...previous, role_id: event.target.value }))}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          >
            {roles.map((role) => (
              <option key={role.id} value={role.id}>
                {role.name}
              </option>
            ))}
          </select>
          <button
            type="submit"
            className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950"
          >
            {createMutation.isPending ? "Creating..." : "Create"}
          </button>
          <label className="flex items-center gap-2 text-sm text-zinc-300 md:col-span-2 xl:col-span-5">
            <input
              type="checkbox"
              checked={createForm.is_active}
              onChange={(event) => setCreateForm((previous) => ({ ...previous, is_active: event.target.checked }))}
            />
            Active
          </label>
        </form>
      </section>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th className="px-4 py-3 font-medium">Username</th>
              <th className="px-4 py-3 font-medium">Role</th>
              <th className="px-4 py-3 font-medium">Active</th>
              <th className="px-4 py-3 font-medium">Last login</th>
              <th className="px-4 py-3 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {users.map((user) => {
              const draft = drafts[user.id] ?? buildDraft(user);
              const role = roleById.get(draft.role_id);
              return (
                <tr key={user.id} className="border-t border-zinc-800">
                  <td className="px-4 py-3 text-zinc-100">
                    <div className="font-medium">{user.username}</div>
                    <div className="text-xs text-zinc-500">{user.email}</div>
                  </td>
                  <td className="px-4 py-3">
                    <select
                      value={draft.role_id}
                      onChange={(event) =>
                        setDrafts((previous) => ({
                          ...previous,
                          [user.id]: { ...draft, role_id: event.target.value },
                        }))
                      }
                      className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
                    >
                      {roles.map((entry: Role) => (
                        <option key={entry.id} value={entry.id}>
                          {entry.name}
                        </option>
                      ))}
                    </select>
                    <p className="mt-1 text-xs text-zinc-500">{role?.name ?? "Unknown role"}</p>
                  </td>
                  <td className="px-4 py-3">
                    <label className="flex items-center gap-2 text-zinc-300">
                      <input
                        type="checkbox"
                        checked={draft.is_active}
                        onChange={(event) =>
                          setDrafts((previous) => ({
                            ...previous,
                            [user.id]: { ...draft, is_active: event.target.checked },
                          }))
                        }
                      />
                      {draft.is_active ? "Yes" : "No"}
                    </label>
                    <label className="mt-2 flex items-center gap-2 text-xs text-zinc-400">
                      <input
                        type="checkbox"
                        checked={draft.force_pw_reset}
                        onChange={(event) =>
                          setDrafts((previous) => ({
                            ...previous,
                            [user.id]: { ...draft, force_pw_reset: event.target.checked },
                          }))
                        }
                      />
                      Force reset
                    </label>
                  </td>
                  <td className="px-4 py-3 text-zinc-300">{formatDateTime(user.last_login_at)}</td>
                  <td className="px-4 py-3">
                    <div className="flex flex-wrap gap-2">
                      <button
                        type="button"
                        onClick={() =>
                          void updateMutation.mutateAsync({
                            id: user.id,
                            patch: {
                              role_id: draft.role_id,
                              is_active: draft.is_active,
                              force_pw_reset: draft.force_pw_reset,
                            },
                          })
                        }
                        className="rounded-lg border border-teal-500 px-3 py-2 text-xs text-teal-300"
                      >
                        Save
                      </button>
                      <button
                        type="button"
                        onClick={() => void resetMutation.mutateAsync(user.id)}
                        className="rounded-lg border border-zinc-700 px-3 py-2 text-xs text-zinc-200"
                      >
                        Reset password
                      </button>
                      <button
                        type="button"
                        onClick={() => void deleteMutation.mutateAsync(user.id)}
                        className="rounded-lg border border-red-900 px-3 py-2 text-xs text-red-300"
                      >
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>
    </div>
  );
}
