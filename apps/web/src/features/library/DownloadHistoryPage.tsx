import { useEffect, useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { apiClient } from "../../lib/api-client";

type DownloadHistorySearchState = {
  page: number;
};

const PAGE_SIZE = 50;

function parsePage(value: string | null): number {
  if (!value) {
    return 1;
  }
  const parsed = Number.parseInt(value, 10);
  if (Number.isNaN(parsed) || parsed < 1) {
    return 1;
  }
  return parsed;
}

function parseSearch(search: string): DownloadHistorySearchState {
  const params = new URLSearchParams(search);
  return {
    page: parsePage(params.get("page")),
  };
}

function toSearch(state: DownloadHistorySearchState): string {
  const params = new URLSearchParams();
  if (state.page > 1) {
    params.set("page", String(state.page));
  }
  return params.toString();
}

export function DownloadHistoryPage() {
  const { t } = useTranslation();
  const [searchState, setSearchState] = useState<DownloadHistorySearchState>(() =>
    parseSearch(window.location.search),
  );

  useEffect(() => {
    const onPopState = () => {
      setSearchState(parseSearch(window.location.search));
    };

    window.addEventListener("popstate", onPopState);
    return () => {
      window.removeEventListener("popstate", onPopState);
    };
  }, []);

  const historyQuery = useQuery({
    queryKey: ["download-history", searchState.page],
    queryFn: () =>
      apiClient.listDownloadHistory({
        page: searchState.page,
        page_size: PAGE_SIZE,
      }),
  });

  const data = historyQuery.data;
  const items = data?.items ?? [];
  const pageSize = data?.page_size ?? PAGE_SIZE;
  const total = data?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));

  const pageLabel = useMemo(() => {
    if (total === 0) {
      return t("downloads.no_downloads_yet");
    }
    return t("common.page_of", { page: searchState.page, total: totalPages });
  }, [searchState.page, t, total, totalPages]);

  function updatePage(page: number) {
    const nextState = { page: Math.max(1, page) };
    setSearchState(nextState);
    const nextSearch = toSearch(nextState);
    const nextUrl = nextSearch ? `${window.location.pathname}?${nextSearch}` : window.location.pathname;
    window.history.replaceState({}, "", nextUrl);
  }

  return (
    <main className="min-h-screen bg-zinc-50 px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto flex max-w-5xl flex-col gap-5">
        <header className="rounded-xl border border-zinc-200 bg-white p-4 shadow-sm">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.18em] text-zinc-500">
                {t("downloads.activity")}
              </p>
              <h1 className="text-2xl font-semibold text-zinc-900">{t("downloads.page_title")}</h1>
            </div>
            <Link
              to="/library"
              className="rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm font-semibold text-zinc-700"
            >
              {t("downloads.back_to_library")}
            </Link>
          </div>
        </header>

        {historyQuery.isLoading ? (
          <div className="rounded-xl border border-zinc-200 bg-white p-6 text-zinc-500">
            {t("downloads.loading")}
          </div>
        ) : null}

        {historyQuery.isError ? (
          <div className="rounded-xl border border-red-200 bg-red-50 p-6 text-red-700">
            {t("downloads.unable_to_load")}
          </div>
        ) : null}

        {!historyQuery.isLoading && !historyQuery.isError ? (
          <section className="overflow-hidden rounded-xl border border-zinc-200 bg-white shadow-sm">
            <table className="w-full border-collapse text-left text-sm">
              <thead className="bg-zinc-50 text-zinc-600">
                <tr>
                  <th className="px-4 py-3 font-semibold">{t("downloads.title")}</th>
                  <th className="px-4 py-3 font-semibold">{t("downloads.format")}</th>
                  <th className="px-4 py-3 font-semibold">{t("downloads.downloaded")}</th>
                </tr>
              </thead>
              <tbody>
                {items.length > 0 ? (
                  items.map((item) => (
                    <tr key={`${item.book_id}-${item.downloaded_at}`} className="border-t border-zinc-100">
                      <td className="px-4 py-3">
                        <Link
                          to="/books/$id"
                          params={{ id: item.book_id }}
                          className="font-medium text-zinc-900 hover:text-teal-700"
                        >
                          {item.title}
                        </Link>
                      </td>
                      <td className="px-4 py-3 text-zinc-700">{item.format.toUpperCase()}</td>
                      <td className="px-4 py-3 text-zinc-500">
                        {new Date(item.downloaded_at).toLocaleString()}
                      </td>
                    </tr>
                  ))
                ) : (
                  <tr>
                    <td colSpan={3} className="px-4 py-10 text-center text-zinc-500">
                      {t("downloads.empty")}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </section>
        ) : null}

        {!historyQuery.isLoading && !historyQuery.isError ? (
          <div className="flex items-center justify-between gap-3">
            <p className="text-sm text-zinc-600">{pageLabel}</p>
            <div className="flex items-center gap-2">
              <button
                type="button"
                disabled={searchState.page <= 1}
                onClick={() => updatePage(searchState.page - 1)}
                className="rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm font-semibold text-zinc-700 disabled:opacity-50"
              >
                {t("common.previous")}
              </button>
              <button
                type="button"
                disabled={searchState.page >= totalPages}
                onClick={() => updatePage(searchState.page + 1)}
                className="rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm font-semibold text-zinc-700 disabled:opacity-50"
              >
                {t("common.next")}
              </button>
            </div>
          </div>
        ) : null}
      </div>
    </main>
  );
}
