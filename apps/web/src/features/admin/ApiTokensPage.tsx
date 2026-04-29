import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { TokenScope } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";

type ScopeOption = {
  value: TokenScope;
  labelKey: string;
  descriptionKey: string;
  badgeClassName: string;
};

const SCOPE_OPTIONS: ScopeOption[] = [
  {
    value: "read",
    labelKey: "token.scope_read",
    descriptionKey: "token.scope_read_desc",
    badgeClassName: "bg-blue-500/15 text-blue-200 ring-blue-400/30",
  },
  {
    value: "write",
    labelKey: "token.scope_write",
    descriptionKey: "token.scope_write_desc",
    badgeClassName: "bg-emerald-500/15 text-emerald-200 ring-emerald-400/30",
  },
  {
    value: "admin",
    labelKey: "token.scope_admin",
    descriptionKey: "token.scope_admin_desc",
    badgeClassName: "bg-rose-500/15 text-rose-200 ring-rose-400/30",
  },
];

function scopeBadgeClass(scope: TokenScope): string {
  return (
    SCOPE_OPTIONS.find((option) => option.value === scope)?.badgeClassName ??
    "bg-zinc-500/15 text-zinc-200 ring-zinc-400/30"
  );
}

function scopeLabel(scope: TokenScope): string {
  return SCOPE_OPTIONS.find((option) => option.value === scope)?.labelKey ?? "token.scope_write";
}

export function ApiTokensPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const currentUser = useAuthStore((state) => state.user);
  const isAdmin = currentUser?.role.name === "admin";
  const [name, setName] = useState("");
  const [scope, setScope] = useState<TokenScope>("write");
  const [createdToken, setCreatedToken] = useState<string | null>(null);

  const tokensQuery = useQuery({
    queryKey: ["admin-api-tokens"],
    queryFn: () => apiClient.listApiTokens(),
  });

  const createMutation = useMutation({
    mutationFn: (payload: { name: string; scope: TokenScope }) => apiClient.createApiToken(payload),
    onSuccess: async (response) => {
      setCreatedToken(response.token);
      setName("");
      setScope("write");
      await queryClient.invalidateQueries({ queryKey: ["admin-api-tokens"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiClient.deleteApiToken(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-api-tokens"] });
    },
  });

  const tokens = tokensQuery.data ?? [];

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.tokens")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">API tokens</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5 shadow-lg">
        <h3 className="text-lg font-semibold text-zinc-50">Create token</h3>
        <form
          className="mt-4 grid gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            void createMutation.mutateAsync({ name: name.trim(), scope });
          }}
        >
          <div className="grid gap-2 md:max-w-md">
            <label htmlFor="api-token-name" className="text-sm font-medium text-zinc-200">
              Token name
            </label>
            <input
              id="api-token-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400"
              placeholder="Token name"
            />
          </div>

          <fieldset className="rounded-2xl border border-zinc-800 bg-zinc-950/60 p-4">
            <legend className="px-1 text-sm font-medium text-zinc-200">Token scope</legend>
            <div
              role="radiogroup"
              aria-label="Token scope"
              className="mt-3 grid gap-3 md:grid-cols-3"
            >
              {SCOPE_OPTIONS.map((option) => {
                const label = t(option.labelKey);
                const description = t(option.descriptionKey);
                const inputId = `token-scope-${option.value}`;
                const disabled = option.value === "admin" && !isAdmin;

                return (
                  <label
                    key={option.value}
                    htmlFor={inputId}
                    className={`flex cursor-pointer flex-col gap-2 rounded-2xl border p-4 transition ${
                      scope === option.value
                        ? "border-teal-400 bg-teal-500/10"
                        : "border-zinc-800 bg-zinc-950/60 hover:border-zinc-700"
                    } ${disabled ? "cursor-not-allowed opacity-60" : ""}`}
                  >
                    <div className="flex items-center gap-3">
                      <input
                        id={inputId}
                        type="radio"
                        name="token-scope"
                        value={option.value}
                        checked={scope === option.value}
                        onChange={() => setScope(option.value)}
                        disabled={disabled}
                        aria-label={label}
                        className="h-4 w-4 border-zinc-600 bg-zinc-900 text-teal-400 focus:ring-teal-400"
                      />
                      <div className="flex flex-col">
                        <span className="text-sm font-semibold text-zinc-50">{label}</span>
                        <span className="text-xs text-zinc-400">{description}</span>
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          </fieldset>

          <div className="flex items-center gap-3">
            <button
              type="submit"
              className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-60"
              disabled={createMutation.isPending}
            >
              {createMutation.isPending ? "Creating..." : "Create token"}
            </button>
          </div>
        </form>

        {createdToken ? (
          <div className="mt-5 rounded-2xl border border-emerald-500/30 bg-emerald-500/10 p-4">
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-200">New token</p>
            <p className="mt-2 text-sm text-emerald-50">Shown once after creation.</p>
            <code className="mt-3 block break-all rounded-xl bg-zinc-950 px-3 py-2 text-sm text-zinc-50">
              {createdToken}
            </code>
          </div>
        ) : null}
      </section>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5 shadow-lg">
        <h3 className="text-lg font-semibold text-zinc-50">Existing tokens</h3>

        {tokensQuery.isLoading ? (
          <p className="mt-4 text-sm text-zinc-400">Loading...</p>
        ) : tokens.length === 0 ? (
          <p className="mt-4 text-sm text-zinc-400">No tokens yet.</p>
        ) : (
          <div className="mt-4 overflow-hidden rounded-2xl border border-zinc-800">
            <table className="min-w-full border-collapse text-left text-sm">
              <thead className="bg-zinc-950/60 text-zinc-400">
                <tr>
                  <th scope="col" className="px-4 py-3 font-medium">
                    Name
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    Scope
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    Created
                  </th>
                  <th scope="col" className="px-4 py-3 font-medium">
                    Actions
                  </th>
                </tr>
              </thead>
              <tbody>
                {tokens.map((token) => (
                  <tr key={token.id} className="border-t border-zinc-800">
                    <td className="px-4 py-3 text-zinc-100">{token.name}</td>
                    <td className="px-4 py-3">
                      <span
                        className={`inline-flex rounded-full px-2.5 py-1 text-xs font-semibold ring-1 ring-inset ${scopeBadgeClass(token.scope)}`}
                      >
                        {t(scopeLabel(token.scope))}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-zinc-300">{token.created_at}</td>
                    <td className="px-4 py-3">
                      <button
                        type="button"
                        className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs font-semibold text-zinc-200 hover:border-rose-400 hover:text-rose-100"
                        onClick={() => void deleteMutation.mutateAsync(token.id)}
                      >
                        Revoke
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
