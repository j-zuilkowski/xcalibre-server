import { useEffect, useMemo, useState } from "react";
import type { ListBooksParams, SearchQuery } from "@autolibre/shared";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { apiClient } from "../../lib/api-client";
import { BookCard } from "../library/BookCard";

type SearchTab = "library" | "semantic";
type SearchState = {
  q: string;
  tab: SearchTab;
  author_id?: string;
  series_id?: string;
  tag?: string;
  language?: string;
  format?: string;
  collection_id?: string;
  sort: string;
  page: number;
};

const PAGE_SIZE = 24;

const FILTER_CHIPS: Array<{ label: string; key: keyof ListBooksParams; value: string }> = [
  { label: "author", key: "author_id", value: "author-default" },
  { label: "series", key: "series_id", value: "series-default" },
  { label: "tag", key: "tag", value: "fiction" },
  { label: "language", key: "language", value: "en" },
  { label: "format", key: "format", value: "epub" },
];

function parsePage(value: string | null): number {
  if (!value) {
    return 1;
  }
  const parsed = Number.parseInt(value, 10);
  return Number.isNaN(parsed) || parsed < 1 ? 1 : parsed;
}

function parseSearch(search: string): SearchState {
  const params = new URLSearchParams(search);
  return {
    q: params.get("q") ?? "",
    tab: params.get("tab") === "semantic" ? "semantic" : "library",
    author_id: params.get("author_id") ?? undefined,
    series_id: params.get("series_id") ?? undefined,
    tag: params.get("tag") ?? undefined,
    language: params.get("language") ?? undefined,
    format: params.get("format") ?? undefined,
    collection_id: params.get("collection_id") ?? undefined,
    sort: params.get("sort") ?? "title",
    page: parsePage(params.get("page")),
  };
}

function toSearch(state: SearchState): string {
  const params = new URLSearchParams();

  if (state.q) {
    params.set("q", state.q);
  }
  if (state.tab !== "library") {
    params.set("tab", state.tab);
  }
  if (state.author_id) {
    params.set("author_id", state.author_id);
  }
  if (state.series_id) {
    params.set("series_id", state.series_id);
  }
  if (state.tag) {
    params.set("tag", state.tag);
  }
  if (state.language) {
    params.set("language", state.language);
  }
  if (state.format) {
    params.set("format", state.format);
  }
  if (state.collection_id) {
    params.set("collection_id", state.collection_id);
  }
  if (state.sort) {
    params.set("sort", state.sort);
  }
  if (state.page > 1) {
    params.set("page", String(state.page));
  }

  return params.toString();
}

export function SearchPage() {
  const { t } = useTranslation();
  const [searchState, setSearchState] = useState<SearchState>(() =>
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

  const searchStatusQuery = useQuery({
    queryKey: ["search-status"],
    queryFn: () => apiClient.getSearchStatus(),
    staleTime: 60_000,
  });

  const collectionsQuery = useQuery({
    queryKey: ["search-collections"],
    queryFn: () => apiClient.listCollections(),
    staleTime: 60_000,
  });

  const queryText = searchState.q.trim();
  const hasQuery = queryText.length > 0;
  const semanticEnabled = searchStatusQuery.data?.semantic === true;
  const effectiveTab = searchState.tab === "semantic" && semanticEnabled ? "semantic" : "library";

  const params = useMemo<SearchQuery>(
    () => ({
      q: queryText || undefined,
      author_id: searchState.author_id,
      series_id: searchState.series_id,
      tag: searchState.tag,
      language: searchState.language,
      format: searchState.format,
      collection_id: searchState.collection_id,
      sort: searchState.sort,
      page: searchState.page,
      page_size: PAGE_SIZE,
      semantic: effectiveTab === "semantic",
    }),
    [effectiveTab, queryText, searchState],
  );

  const booksQuery = useQuery({
    queryKey: ["search-books", params],
    queryFn: () => apiClient.search(params),
    enabled: hasQuery,
  });

  function updateSearchState(next: Partial<SearchState>) {
    setSearchState((previous) => {
      const updated: SearchState = {
        ...previous,
        ...next,
        page: next.page ?? previous.page,
      };

      const nextSearch = toSearch(updated);
      const nextUrl = nextSearch ? `${window.location.pathname}?${nextSearch}` : window.location.pathname;
      window.history.replaceState({}, "", nextUrl);

      return updated;
    });
  }

  function toggleFilter(key: keyof ListBooksParams, value: string) {
    const current = searchState[key as keyof SearchState];
    const nextValue = current === value ? undefined : value;

    updateSearchState({
      [key]: nextValue,
      page: 1,
    } as Partial<SearchState>);
  }

  const books = hasQuery ? booksQuery.data?.items ?? [] : [];
  const total = hasQuery ? booksQuery.data?.total ?? 0 : 0;
  const pageSize = hasQuery ? booksQuery.data?.page_size ?? PAGE_SIZE : PAGE_SIZE;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const isSearching = hasQuery && booksQuery.isLoading;

  return (
    <main className="min-h-screen bg-zinc-50 px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto flex max-w-[1440px] flex-col gap-5">
        <header className="rounded-xl border border-zinc-200 bg-white p-4 shadow-sm">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h1 className="text-2xl font-semibold text-zinc-900">{t("search.page_title")}</h1>
              <p className="text-sm text-zinc-500">
                {hasQuery ? t("search.results_for", { query: queryText }) : t("search.enter_query")}
              </p>
            </div>

            <div className="flex items-center gap-2">
              <select
                value={searchState.collection_id ?? ""}
                onChange={(event) =>
                  updateSearchState({ collection_id: event.target.value || undefined, page: 1 })
                }
                className="rounded-full border border-zinc-300 bg-white px-4 py-2 text-sm text-zinc-700"
                aria-label="Collection"
              >
                <option value="">{t("common.collection", { defaultValue: "Collection" })}</option>
                {collectionsQuery.data?.map((collection) => (
                  <option key={collection.id} value={collection.id}>
                    {collection.name}
                  </option>
                ))}
              </select>

              {(["library", "semantic"] as const).map((tab) => {
                const active = effectiveTab === tab;
                const disabled = tab === "semantic" && !semanticEnabled;
                return (
                  <button
                    key={tab}
                    type="button"
                    title={disabled ? t("search.semantic_unavailable") : undefined}
                    disabled={disabled}
                    onClick={() => updateSearchState({ tab, page: 1 })}
                    className={`rounded-lg border px-3 py-2 text-sm ${
                      active
                        ? "border-zinc-900 bg-zinc-900 text-zinc-50"
                        : disabled
                          ? "cursor-not-allowed border-zinc-300 bg-zinc-100 text-zinc-400"
                          : "border-zinc-300 bg-white text-zinc-700"
                    }`}
                  >
                    {tab === "library" ? t("search.library_tab") : t("search.semantic_tab")}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="mt-4 flex flex-wrap items-center gap-2">
            {FILTER_CHIPS.map((chip) => {
              const active = searchState[chip.key as keyof SearchState] === chip.value;
              return (
                <button
                  key={chip.key}
                  type="button"
                  onClick={() => toggleFilter(chip.key, chip.value)}
                  className={`rounded-full border px-3 py-1.5 text-sm transition ${
                    active
                      ? "border-teal-600 bg-teal-600 text-white"
                      : "border-zinc-300 bg-white text-zinc-700 hover:border-zinc-400"
                  }`}
                >
                  {t(`library.${chip.label}`)}
                </button>
              );
            })}

            <div className="ml-auto flex items-center gap-2">
              <label htmlFor="sort" className="text-sm text-zinc-500">
                {t("library.sort")}
              </label>
              <select
                id="sort"
                value={searchState.sort}
                onChange={(event) => updateSearchState({ sort: event.target.value, page: 1 })}
                className="rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm"
              >
                <option value="title">{t("library.title")}</option>
                <option value="author">{t("library.author")}</option>
                <option value="created_at">{t("library.date_added")}</option>
                <option value="rating">{t("library.rating")}</option>
              </select>
            </div>
          </div>
        </header>

        <div aria-live="polite" aria-atomic="true" className="sr-only">
          {hasQuery
            ? isSearching
              ? "Searching..."
              : books.length === 0
                ? "No results found"
                : `${books.length} results found`
            : ""}
        </div>

        {hasQuery && booksQuery.isLoading ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6 2xl:grid-cols-8">
            {Array.from({ length: 8 }).map((_, index) => (
              <div key={index} className="aspect-[2/3] animate-pulse rounded-lg bg-zinc-200" />
            ))}
          </div>
        ) : null}

        {!hasQuery ? (
          <section className="rounded-xl border border-zinc-200 bg-white p-10 text-center">
            <h2 className="text-2xl font-semibold text-zinc-900">{t("search.search_your_library")}</h2>
            <p className="mt-2 text-zinc-500">{t("search.search_prompt")}</p>
          </section>
        ) : null}

        {hasQuery && booksQuery.isError ? (
          <section className="rounded-xl border border-red-200 bg-red-50 p-6 text-red-700">
            {t("search.unable_to_search")}
          </section>
        ) : null}

        {hasQuery && !booksQuery.isLoading && !booksQuery.isError && books.length === 0 ? (
          <section className="rounded-xl border border-zinc-200 bg-white p-10 text-center">
            <h2 className="text-2xl font-semibold text-zinc-900">{t("search.no_matching_books")}</h2>
            <p className="mt-2 text-zinc-500">{t("search.try_different_query")}</p>
          </section>
        ) : null}

        {hasQuery && !booksQuery.isLoading && books.length > 0 ? (
          <>
            <section className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6 2xl:grid-cols-8">
              {books.map((book) => (
                <BookCard
                  key={book.id}
                  book={book}
                  progressPercentage={book.progress_percentage ?? 0}
                  score={book.score}
                />
              ))}
            </section>

            <footer className="flex items-center justify-between rounded-xl border border-zinc-200 bg-white p-4">
              <button
                type="button"
                onClick={() => updateSearchState({ page: Math.max(1, searchState.page - 1) })}
                disabled={searchState.page <= 1}
                className="rounded-lg border border-zinc-300 px-3 py-2 text-sm disabled:cursor-not-allowed disabled:opacity-50"
              >
                {t("common.previous")}
              </button>
              <p className="text-sm text-zinc-600">
                {t("common.page_of", { page: searchState.page, total: totalPages })}
              </p>
              <button
                type="button"
                onClick={() => updateSearchState({ page: Math.min(totalPages, searchState.page + 1) })}
                disabled={searchState.page >= totalPages}
                className="rounded-lg border border-zinc-300 px-3 py-2 text-sm disabled:cursor-not-allowed disabled:opacity-50"
              >
                {t("common.next")}
              </button>
            </footer>
          </>
        ) : null}
      </div>
    </main>
  );
}
