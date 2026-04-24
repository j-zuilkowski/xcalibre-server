import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { UserStats } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { ProfileSidebar } from "./ProfileSidebar";

function StatCard({
  icon,
  value,
  label,
}: {
  icon: string;
  value: string | number;
  label: string;
}) {
  return (
    <article className="rounded-3xl border border-zinc-800 bg-zinc-900/80 p-5 shadow-[0_24px_80px_-30px_rgba(15,118,110,0.35)]">
      <div className="flex items-center justify-between gap-3">
        <span className="inline-flex h-10 w-10 items-center justify-center rounded-2xl bg-teal-500/15 text-lg text-teal-200">
          {icon}
        </span>
        <span className="text-xs uppercase tracking-[0.24em] text-zinc-500">{label}</span>
      </div>
      <div className="mt-4 text-3xl font-semibold tracking-tight text-zinc-50">{value}</div>
    </article>
  );
}

function formatPercent(value: number, t: (key: string) => string): string {
  return `${value.toFixed(1)} ${t("stats.pp")}`;
}

export function StatsPage() {
  const { t } = useTranslation();

  const statsQuery = useQuery({
    queryKey: ["user-stats"],
    queryFn: () => apiClient.getUserStats(),
  });

  if (statsQuery.isLoading) {
    return (
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 lg:flex-row">
        <ProfileSidebar active="stats" />
        <main className="min-w-0 flex-1">
          <div className="rounded-3xl border border-zinc-800 bg-zinc-900/70 p-6 text-zinc-300">
            {t("common.loading")}
          </div>
        </main>
      </div>
    );
  }

  if (statsQuery.isError || !statsQuery.data) {
    return (
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 lg:flex-row">
        <ProfileSidebar active="stats" />
        <main className="min-w-0 flex-1">
          <div className="rounded-3xl border border-red-900 bg-red-950/60 p-6 text-red-200">
            {t("stats.unable_to_load")}
          </div>
        </main>
      </div>
    );
  }

  const stats = statsQuery.data as UserStats;
  const chartMax = Math.max(...stats.monthly_books.map((entry) => entry.count), 0);
  const formatEntries = Object.entries(stats.formats_read).sort((left, right) => {
    const countDelta = right[1] - left[1];
    if (countDelta !== 0) {
      return countDelta;
    }
    return left[0].localeCompare(right[0]);
  });
  const formatTotal = formatEntries.reduce((sum, [, count]) => sum + count, 0);

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 lg:flex-row">
      <ProfileSidebar active="stats" />

      <main className="min-w-0 flex-1">
        <div className="flex flex-col gap-6">
          <header>
            <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("nav.reading_stats")}</p>
            <h2 className="mt-2 text-3xl font-semibold text-zinc-50">{t("stats.page_title")}</h2>
          </header>

          <section className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <StatCard icon="📚" value={stats.total_books_read} label={t("stats.books_read")} />
            <StatCard icon="📅" value={stats.books_read_this_year} label={t("stats.this_year")} />
            <StatCard icon="🔥" value={t("stats.days", { value: stats.reading_streak_days })} label={t("stats.streak")} />
            <StatCard icon="⏳" value={stats.books_in_progress} label={t("stats.in_progress")} />
          </section>

          <section className="flex flex-wrap items-center gap-3 rounded-3xl border border-zinc-800 bg-zinc-900/60 px-5 py-4 text-sm text-zinc-300">
            <span className="rounded-full border border-zinc-700 bg-zinc-950/60 px-3 py-1">
              {t("stats.total_sessions")}: {stats.total_reading_sessions}
            </span>
            <span className="rounded-full border border-zinc-700 bg-zinc-950/60 px-3 py-1">
              {t("stats.average_progress")}: {formatPercent(stats.average_progress_per_session, t)}
            </span>
          </section>

          <section className="rounded-3xl border border-zinc-800 bg-zinc-900/80 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3 className="text-lg font-semibold text-zinc-50">{t("stats.monthly_books")}</h3>
                <p className="mt-1 text-sm text-zinc-400">{t("stats.last_12_months")}</p>
              </div>
              <span className="rounded-full border border-teal-500/30 bg-teal-500/10 px-3 py-1 text-xs font-semibold text-teal-200">
                {t("stats.books_read")}
              </span>
            </div>

            <svg
              className="mt-6 h-[280px] w-full overflow-visible"
              viewBox="0 0 960 280"
              role="img"
              aria-label={t("stats.monthly_books")}
            >
              {stats.monthly_books.map((entry, index) => {
                const x = 40 + index * 75;
                const barHeight = chartMax > 0 ? Math.max(8, (entry.count / chartMax) * 164) : 8;
                const y = 194 - barHeight;

                return (
                  <g key={entry.month}>
                    <text x={x + 18} y={34} textAnchor="middle" className="fill-zinc-300 text-[13px] font-medium">
                      {entry.count}
                    </text>
                    <rect
                      x={x}
                      y={y}
                      width={36}
                      height={barHeight}
                      rx={12}
                      className="fill-teal-500"
                    />
                    <rect
                      x={x}
                      y={y}
                      width={36}
                      height={barHeight}
                      rx={12}
                      fill="url(#tealGradient)"
                      opacity="0.9"
                    />
                    <text
                      x={x + 18}
                      y={236}
                      textAnchor="middle"
                      className="fill-zinc-400 text-[11px] uppercase tracking-[0.14em]"
                    >
                      {entry.month}
                    </text>
                  </g>
                );
              })}

              <defs>
                <linearGradient id="tealGradient" x1="0%" y1="0%" x2="0%" y2="100%">
                  <stop offset="0%" stopColor="#14b8a6" stopOpacity="1" />
                  <stop offset="100%" stopColor="#0f766e" stopOpacity="1" />
                </linearGradient>
              </defs>
            </svg>
          </section>

          <section className="grid gap-4 lg:grid-cols-2">
            <div className="rounded-3xl border border-zinc-800 bg-zinc-900/80 p-5">
              <h3 className="text-lg font-semibold text-zinc-50">{t("stats.top_authors")}</h3>
              <ol className="mt-4 space-y-3">
                {stats.top_authors.length > 0 ? (
                  stats.top_authors.map((author, index) => (
                    <li key={author.name} className="flex items-center justify-between gap-4 rounded-2xl border border-zinc-800 bg-zinc-950/50 px-4 py-3">
                      <div className="flex items-center gap-3">
                        <span className="flex h-8 w-8 items-center justify-center rounded-full bg-teal-500/15 text-sm font-semibold text-teal-200">
                          {index + 1}
                        </span>
                        <span className="text-zinc-100">{author.name}</span>
                      </div>
                      <span className="text-sm text-zinc-400">{author.count}</span>
                    </li>
                  ))
                ) : (
                  <li className="rounded-2xl border border-zinc-800 bg-zinc-950/50 px-4 py-3 text-sm text-zinc-400">
                    {t("common.none")}
                  </li>
                )}
              </ol>
            </div>

            <div className="rounded-3xl border border-zinc-800 bg-zinc-900/80 p-5">
              <h3 className="text-lg font-semibold text-zinc-50">{t("stats.top_tags")}</h3>
              <ol className="mt-4 space-y-3">
                {stats.top_tags.length > 0 ? (
                  stats.top_tags.map((tag, index) => (
                    <li key={tag.name} className="flex items-center justify-between gap-4 rounded-2xl border border-zinc-800 bg-zinc-950/50 px-4 py-3">
                      <div className="flex items-center gap-3">
                        <span className="flex h-8 w-8 items-center justify-center rounded-full bg-teal-500/15 text-sm font-semibold text-teal-200">
                          {index + 1}
                        </span>
                        <span className="text-zinc-100">{tag.name}</span>
                      </div>
                      <span className="text-sm text-zinc-400">{tag.count}</span>
                    </li>
                  ))
                ) : (
                  <li className="rounded-2xl border border-zinc-800 bg-zinc-950/50 px-4 py-3 text-sm text-zinc-400">
                    {t("common.none")}
                  </li>
                )}
              </ol>
            </div>
          </section>

          <section className="rounded-3xl border border-zinc-800 bg-zinc-900/80 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3 className="text-lg font-semibold text-zinc-50">{t("stats.formats_breakdown")}</h3>
                <p className="mt-1 text-sm text-zinc-400">{t("stats.formats_breakdown_subtitle")}</p>
              </div>
              <span className="rounded-full border border-teal-500/30 bg-teal-500/10 px-3 py-1 text-xs font-semibold text-teal-200">
                {t("stats.formats_breakdown")}
              </span>
            </div>

            {formatEntries.length > 0 ? (
              <>
                <div className="mt-5 flex h-4 overflow-hidden rounded-full bg-zinc-800">
                  {formatEntries.map(([format, count], index) => {
                    const width = `${(count / formatTotal) * 100}%`;
                    const palette = ["#14b8a6", "#0f766e", "#2dd4bf", "#115e59", "#5eead4"];
                    const color = palette[index % palette.length];
                    return (
                      <div
                        key={format}
                        className="h-full"
                        style={{ width, backgroundColor: color }}
                        title={`${format.toUpperCase()}: ${count}`}
                      />
                    );
                  })}
                </div>
                <div className="mt-4 flex flex-wrap gap-3 text-sm text-zinc-300">
                  {formatEntries.map(([format, count]) => (
                    <span
                      key={format}
                      className="rounded-full border border-zinc-700 bg-zinc-950/60 px-3 py-1"
                    >
                      {format.toUpperCase()}: {count}
                    </span>
                  ))}
                </div>
              </>
            ) : (
              <p className="mt-4 text-sm text-zinc-400">{t("common.none")}</p>
            )}
          </section>
        </div>
      </main>
    </div>
  );
}
