/**
 * UsersPage — admin user management table.
 *
 * Route: /admin/users
 *
 * Features:
 *   - Inline create-user form (username, email, password, role, active).
 *     The role selector pre-selects the first available role once the roles
 *     list loads.
 *   - User table with per-row:
 *       - Role selector (drafts stored in `drafts` state map, applied on
 *         "Save").
 *       - Active checkbox and force-password-reset checkbox.
 *       - "Save" — PATCH /api/v1/users/:id with the draft values.
 *       - "Reset password" — POST /api/v1/users/:id/reset-password (sends
 *         email or generates a token depending on backend config).
 *       - "Tag restrictions" — opens TagRestrictionsModal.
 *       - "Disable 2FA" — POST /api/v1/users/:id/totp/disable (admin action,
 *         visible only when `user.totp_enabled` is true).
 *       - "Delete" — opens confirm Dialog.
 *
 * `drafts` is a per-user-id record initialised lazily as users load.  This
 * allows the admin to modify multiple rows without triggering saves until they
 * explicitly click "Save" for each user.
 *
 * TagRestrictionsModal (defined in the same file) manages allow/block tag
 * restrictions for a selected user.
 *
 * API calls:
 *   GET    /api/v1/users
 *   GET    /api/v1/roles
 *   POST   /api/v1/users
 *   PATCH  /api/v1/users/:id
 *   DELETE /api/v1/users/:id
 *   POST   /api/v1/users/:id/reset-password
 *   POST   /api/v1/users/:id/totp/disable
 *   GET    /api/v1/users/:id/tag-restrictions
 *   PUT    /api/v1/users/:id/tag-restrictions/:tagId
 *   DELETE /api/v1/users/:id/tag-restrictions/:tagId
 */
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { AdminUser, Role, TagLookupItem } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { Dialog } from "../../components/ui/Dialog";
import { formatDateTime } from "./admin-utils";
import { TagAutocomplete } from "./TagAutocomplete";

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

/**
 * UsersPage renders the admin user management table with create form, per-row
 * role/status editing, password reset, TOTP disable, tag restrictions, and
 * delete confirmation dialog.
 */
export function UsersPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [drafts, setDrafts] = useState<Record<string, UserDraft>>({});
  const [tagRestrictionUser, setTagRestrictionUser] = useState<AdminUser | null>(null);
  const [deleteCandidate, setDeleteCandidate] = useState<AdminUser | null>(null);
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
      let changed = false;
      const next = { ...previous };
      for (const user of users) {
        if (!next[user.id]) {
          next[user.id] = buildDraft(user);
          changed = true;
        }
      }
      return changed ? next : previous;
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

  const disableTotpMutation = useMutation({
    mutationFn: (id: string) => apiClient.disableUserTotp(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-users"] });
    },
  });

  const resetMutation = useMutation({
    mutationFn: (id: string) => apiClient.resetUserPassword(id),
  });

  const roleById = useMemo(() => new Map(roles.map((role) => [role.id, role])), [roles]);
  const deleteDialogTitleId = useId();
  const deleteCancelRef = useRef<HTMLButtonElement | null>(null);

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.users")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">{t("admin.user_management")}</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <h3 className="text-lg font-semibold text-zinc-50">{t("admin.create_user")}</h3>
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
            placeholder={t("auth.username")}
            aria-label={t("auth.username")}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400"
          />
          <input
            value={createForm.email}
            onChange={(event) => setCreateForm((previous) => ({ ...previous, email: event.target.value }))}
            placeholder={t("auth.email")}
            aria-label={t("auth.email")}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400"
          />
          <input
            value={createForm.password}
            onChange={(event) => setCreateForm((previous) => ({ ...previous, password: event.target.value }))}
            placeholder={t("auth.password")}
            type="password"
            aria-label={t("auth.password")}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400"
          />
          <select
            value={createForm.role_id}
            onChange={(event) => setCreateForm((previous) => ({ ...previous, role_id: event.target.value }))}
            aria-label={t("admin.role")}
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
            {createMutation.isPending ? t("common.creating") : t("common.create")}
          </button>
          <label className="flex items-center gap-2 text-sm text-zinc-300 md:col-span-2 xl:col-span-5">
            <input
              type="checkbox"
              checked={createForm.is_active}
              onChange={(event) => setCreateForm((previous) => ({ ...previous, is_active: event.target.checked }))}
            />
            {t("common.active")}
          </label>
        </form>
      </section>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th scope="col" className="px-4 py-3 font-medium">{t("auth.username")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("admin.role")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("common.active")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("admin.last_login")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("common.actions")}</th>
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
                      aria-label={`Role for user ${user.username}`}
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
                    <p className="mt-1 text-xs text-zinc-500">{role?.name ?? t("common.unknown_role")}</p>
                  </td>
                  <td className="px-4 py-3">
                    <label className="flex items-center gap-2 text-zinc-300">
                      <input
                        type="checkbox"
                        checked={draft.is_active}
                        aria-label={`Active status for user ${user.username}`}
                        onChange={(event) =>
                          setDrafts((previous) => ({
                            ...previous,
                            [user.id]: { ...draft, is_active: event.target.checked },
                          }))
                        }
                      />
                      {draft.is_active ? t("common.yes") : t("common.no")}
                    </label>
                    <label className="mt-2 flex items-center gap-2 text-xs text-zinc-400">
                      <input
                        type="checkbox"
                        checked={draft.force_pw_reset}
                        aria-label={`Force password reset for user ${user.username}`}
                        onChange={(event) =>
                          setDrafts((previous) => ({
                            ...previous,
                            [user.id]: { ...draft, force_pw_reset: event.target.checked },
                          }))
                        }
                      />
                      {t("admin.force_reset")}
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
                        {t("common.save")}
                      </button>
                      <button
                        type="button"
                        onClick={() => void resetMutation.mutateAsync(user.id)}
                        className="rounded-lg border border-zinc-700 px-3 py-2 text-xs text-zinc-200"
                      >
                        {t("admin.reset_password")}
                      </button>
                      <button
                        type="button"
                        onClick={() => setTagRestrictionUser(user)}
                        className="rounded-lg border border-zinc-700 px-3 py-2 text-xs text-zinc-200"
                      >
                        {t("admin.tag_restrictions")}
                      </button>
                      {user.totp_enabled ? (
                        <button
                          type="button"
                          onClick={() => void disableTotpMutation.mutateAsync(user.id)}
                          className="rounded-lg border border-amber-700 px-3 py-2 text-xs text-amber-300"
                        >
                          Disable 2FA
                        </button>
                      ) : null}
                      <button
                        type="button"
                        onClick={() => setDeleteCandidate(user)}
                        className="rounded-lg border border-red-900 px-3 py-2 text-xs text-red-300"
                      >
                        {t("common.delete")}
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>

      {tagRestrictionUser ? (
        <TagRestrictionsModal
          user={tagRestrictionUser}
          onClose={() => setTagRestrictionUser(null)}
        />
      ) : null}

      <Dialog
        open={deleteCandidate !== null}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteCandidate(null);
          }
        }}
        titleId={deleteDialogTitleId}
        initialFocusRef={deleteCancelRef}
      >
        <div className="mx-auto w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-950 p-5 text-zinc-100 shadow-2xl">
          <h3 id={deleteDialogTitleId} className="text-xl font-semibold text-zinc-50">
            Delete user?
          </h3>
          <p className="mt-2 text-sm text-zinc-400">
            {deleteCandidate ? `This will remove ${deleteCandidate.username}.` : null}
          </p>
          <div className="mt-5 flex justify-end gap-2">
            <button
              ref={deleteCancelRef}
              type="button"
              onClick={() => setDeleteCandidate(null)}
              className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200"
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              onClick={() => {
                if (deleteCandidate) {
                  void deleteMutation.mutateAsync(deleteCandidate.id);
                }
                setDeleteCandidate(null);
              }}
              className="rounded-lg border border-red-500 px-3 py-2 text-sm text-red-300"
            >
              {t("common.delete")}
            </button>
          </div>
        </div>
      </Dialog>
    </div>
  );
}

function TagRestrictionsModal({
  user,
  onClose,
}: {
  user: AdminUser;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [selectedTag, setSelectedTag] = useState<TagLookupItem | null>(null);
  const [mode, setMode] = useState<"allow" | "block">("block");
  const titleId = useId();
  const closeRef = useRef<HTMLButtonElement | null>(null);

  const restrictionsQuery = useQuery({
    queryKey: ["admin-user-tag-restrictions", user.id],
    queryFn: () => apiClient.listUserTagRestrictions(user.id),
  });

  const addMutation = useMutation({
    mutationFn: (payload: { tagId: string; mode: "allow" | "block" }) =>
      apiClient.setUserTagRestriction(user.id, {
        tag_id: payload.tagId,
        mode: payload.mode,
      }),
    onSuccess: async () => {
      setSelectedTag(null);
      await queryClient.invalidateQueries({ queryKey: ["admin-user-tag-restrictions", user.id] });
    },
  });

  const removeMutation = useMutation({
    mutationFn: (tagId: string) => apiClient.deleteUserTagRestriction(user.id, tagId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-user-tag-restrictions", user.id] });
    },
  });

  const restrictions = restrictionsQuery.data ?? [];

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          onClose();
        }
      }}
      titleId={titleId}
      initialFocusRef={closeRef}
    >
      <div className="mx-auto w-full max-w-2xl rounded-2xl border border-zinc-700 bg-zinc-950 p-5 shadow-2xl">
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-xs uppercase tracking-[0.2em] text-teal-300">{t("admin.tag_restrictions")}</p>
            <h3 id={titleId} className="mt-1 text-xl font-semibold text-zinc-50">
              {user.username}
            </h3>
            <p className="text-sm text-zinc-400">{user.email}</p>
          </div>
            <button
              ref={closeRef}
              type="button"
              onClick={onClose}
              className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200"
            >
              {t("common.close")}
            </button>
        </div>

        <div className="mt-5 grid gap-4 md:grid-cols-[1fr_auto]">
          <div className="space-y-2">
            <label className="text-sm text-zinc-300">{t("admin.tag")}</label>
            <TagAutocomplete onSelect={setSelectedTag} placeholder={t("admin.search_existing_tags")} />
            {selectedTag ? (
              <p className="text-xs text-zinc-400">
                {t("common.selected")} <span className="font-medium text-zinc-200">{selectedTag.name}</span>
              </p>
            ) : null}
          </div>

          <div className="space-y-2">
            <label className="text-sm text-zinc-300">{t("admin.mode")}</label>
            <select
              value={mode}
              onChange={(event) => setMode(event.target.value as "allow" | "block")}
              className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
            >
              <option value="block">{t("admin.block")}</option>
              <option value="allow">{t("admin.allow")}</option>
            </select>
            <button
              type="button"
              disabled={!selectedTag || addMutation.isPending}
              onClick={() => {
                if (!selectedTag) {
                  return;
                }
                void addMutation.mutateAsync({ tagId: selectedTag.id, mode });
              }}
              className="w-full rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-60"
            >
              {t("common.add")}
            </button>
          </div>
        </div>

        <div className="mt-6">
          <h4 className="text-sm font-semibold uppercase tracking-[0.18em] text-zinc-400">
            {t("admin.current_restrictions")}
          </h4>
          <div className="mt-3 space-y-2">
            {restrictions.length > 0 ? (
              restrictions.map((restriction) => (
                <div
                  key={restriction.tag_id}
                  className="flex items-center justify-between gap-3 rounded-xl border border-zinc-800 bg-zinc-900 px-3 py-2"
                >
                  <div>
                    <p className="text-sm font-medium text-zinc-100">{restriction.tag_name}</p>
                    <p className="text-xs uppercase tracking-[0.18em] text-zinc-500">{restriction.mode}</p>
                  </div>
                  <button
                    type="button"
                    onClick={() => void removeMutation.mutateAsync(restriction.tag_id)}
                    className="rounded-lg border border-zinc-700 px-3 py-2 text-xs text-zinc-200"
                  >
                    {t("common.remove")}
                  </button>
                </div>
              ))
            ) : (
              <p className="text-sm text-zinc-500">{t("admin.no_restrictions_yet")}</p>
            )}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
