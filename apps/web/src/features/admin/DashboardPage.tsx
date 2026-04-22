import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { apiClient } from "../../lib/api-client";
import { formatBytes } from "./admin-utils";

function StatCard({
  label,
  value,
  detail,
}: {
  label: string;
  value: string;
  detail?: string;
}) {
  return (
    <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5 shadow-lg">
      <p className="text-sm text-zinc-400">{label}</p>
      <p className="mt-3 text-3xl font-semibold text-zinc-50">{value}</p>
      {detail ? <p className="mt-2 text-xs text-zinc-500">{detail}</p> : null}
    </section>
  );
}

export function DashboardPage() {
  const { t } = useTranslation();
  const systemQuery = useQuery({
    queryKey: ["admin-system"],
    queryFn: () => apiClient.getSystemStats(),
  });

  const usersQuery = useQuery({
    queryKey: ["admin-users"],
    queryFn: () => apiClient.listUsers(),
  });

  const system = systemQuery.data;
  const userCount = usersQuery.data?.length ?? 0;

  if (systemQuery.isLoading || usersQuery.isLoading) {
    return <div className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-8 text-zinc-300">{t("admin.loading_dashboard")}</div>;
  }

  if (systemQuery.isError) {
    return <div className="rounded-2xl border border-red-900 bg-red-950/60 p-8 text-red-200">{t("admin.unable_to_load_system_stats")}</div>;
  }

  if (!system) {
    return null;
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.dashboard")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">{t("admin.system_overview")}</h2>
      </header>

      <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        <StatCard label={t("admin.total_books")} value={String(system.book_count)} detail={t("admin.formats_detail", { count: system.format_count })} />
        <StatCard label={t("admin.users")} value={String(userCount)} detail={t("admin.database_detail", { engine: system.db_engine })} />
        <StatCard label={t("admin.storage_used")} value={formatBytes(system.storage_used_bytes)} detail={t("admin.db_size_detail", { size: formatBytes(system.db_size_bytes) })} />
        <StatCard
          label={t("admin.llm_status")}
          value={system.llm.enabled ? t("common.enabled") : t("common.disabled")}
          detail={
            system.llm.enabled
              ? t("admin.llm_detail", {
                  librarian: system.llm.librarian_available ? t("common.ready") : t("common.down"),
                  architect: system.llm.architect_available ? t("common.ready") : t("common.down"),
                })
              : t("admin.all_llm_surfaces_disabled")
          }
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-2">
        <div className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
          <h3 className="text-lg font-semibold text-zinc-50">{t("admin.search_indexing")}</h3>
          <p className="mt-2 text-sm text-zinc-400">
            {t("admin.indexed_of_books", { indexed: system.meilisearch.indexed_count, total: system.book_count })}
          </p>
          <p className="mt-1 text-sm text-zinc-400">{t("admin.pending", { count: system.meilisearch.pending_count })}</p>
          <p className="mt-1 text-sm text-zinc-400">
            {t("admin.status", { value: system.meilisearch.available ? t("common.available") : t("common.unavailable") })}
          </p>
        </div>

        <div className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
          <h3 className="text-lg font-semibold text-zinc-50">{t("admin.version")}</h3>
          <p className="mt-2 text-sm text-zinc-400">{t("admin.app_version", { version: system.version })}</p>
          <p className="mt-1 text-sm text-zinc-400">{t("admin.database_engine", { engine: system.db_engine })}</p>
        </div>
      </section>
    </div>
  );
}
