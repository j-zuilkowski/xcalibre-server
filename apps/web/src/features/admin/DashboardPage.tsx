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
    return <div className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-8 text-zinc-300">Loading dashboard...</div>;
  }

  if (systemQuery.isError) {
    return <div className="rounded-2xl border border-red-900 bg-red-950/60 p-8 text-red-200">Unable to load system stats.</div>;
  }

  if (!system) {
    return null;
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">Dashboard</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">System overview</h2>
      </header>

      <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        <StatCard label="Total books" value={String(system.book_count)} detail={`Formats: ${system.format_count}`} />
        <StatCard label="Users" value={String(userCount)} detail={`Database: ${system.db_engine}`} />
        <StatCard label="Storage used" value={formatBytes(system.storage_used_bytes)} detail={`DB size ${formatBytes(system.db_size_bytes)}`} />
        <StatCard
          label="LLM status"
          value={system.llm.enabled ? "Enabled" : "Disabled"}
          detail={
            system.llm.enabled
              ? `Librarian ${system.llm.librarian_available ? "ready" : "down"} · Architect ${system.llm.architect_available ? "ready" : "down"}`
              : "All LLM surfaces are disabled"
          }
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-2">
        <div className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
          <h3 className="text-lg font-semibold text-zinc-50">Search indexing</h3>
          <p className="mt-2 text-sm text-zinc-400">
            Indexed {system.meilisearch.indexed_count} of {system.book_count} books.
          </p>
          <p className="mt-1 text-sm text-zinc-400">
            Pending: {system.meilisearch.pending_count}
          </p>
          <p className="mt-1 text-sm text-zinc-400">
            Status: {system.meilisearch.available ? "Available" : "Unavailable"}
          </p>
        </div>

        <div className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
          <h3 className="text-lg font-semibold text-zinc-50">Version</h3>
          <p className="mt-2 text-sm text-zinc-400">App version {system.version}</p>
          <p className="mt-1 text-sm text-zinc-400">Database engine {system.db_engine}</p>
        </div>
      </section>
    </div>
  );
}
