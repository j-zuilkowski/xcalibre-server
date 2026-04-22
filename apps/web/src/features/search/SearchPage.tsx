import { useEffect, useMemo, useState } from "react";
import type { ListBooksParams, SearchQuery } from "@autolibre/shared";
import { useQuery } from "@tanstack/react-query";
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
  sort: string;
  page: number;
};

const PAGE_SIZE = 24;

const FILTER_CHIPS: Array<{ label: string; key: keyof ListBooksParams; value: string }> = [
  { label: "Author", key: "author_id", value: "author-default" },
  { label: "Series", key: "series_id", value: "series-default" },
  { label: "Tag", key: "tag", value: "fiction" },
  { label: "Language", key: "language", value: "en" },
  { label: "Format", key: "format", value: "epub" },
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
  if (state.sort) {
    params.set("sort", state.sort);
  }
  if (state.page > 1) {
    params.set("page", String(state.page));
  }

  return params.toString();
}

export function SearchPage() {
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

  return (
    <main className="min-h-screen bg-zinc-50 px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto flex max-w-[1440px] flex-col gap-5">
        <header className="rounded-xl border border-zinc-200 bg-white p-4 shadow-sm">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h1 className="text-2xl font-semibold text-zinc-900">Search</h1>
              <p className="text-sm text-zinc-500">
                {hasQuery ? `Results for "${queryText}"` : "Enter a search to browse results."}
              </p>
            </div>

            <div className="flex items-center gap-2">
              {(["library", "semantic"] as const).map((tab) => {
                const active = effectiveTab === tab;
                const disabled = tab === "semantic" && !semanticEnabled;
                return (
                  <button
                    key={tab}
                    type="button"
                    title={disabled ? "Semantic search is unavailable right now." : undefined}
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
                    {tab === "library" ? "Library" : "Semantic"}
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
                  {chip.label}
                </button>
              );
            })}

            <div className="ml-auto flex items-center gap-2">
              <label htmlFor="sort" className="text-sm text-zinc-500">
                Sort
              </label>
              <select
                id="sort"
                value={searchState.sort}
                onChange={(event) => updateSearchState({ sort: event.target.value, page: 1 })}
                className="rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm"
              >
                <option value="title">Title</option>
                <option value="author">Author</option>
                <option value="created_at">Date Added</option>
                <option value="rating">Rating</option>
              </select>
            </div>
          </div>
        </header>

        {hasQuery && booksQuery.isLoading ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6 2xl:grid-cols-8">
            {Array.from({ length: 8 }).map((_, index) => (
              <div key={index} className="aspect-[2/3] animate-pulse rounded-lg bg-zinc-200" />
            ))}
          </div>
        ) : null}

        {!hasQuery ? (
          <section className="rounded-xl border border-zinc-200 bg-white p-10 text-center">
            <h2 className="text-2xl font-semibold text-zinc-900">Search your library</h2>
            <p className="mt-2 text-zinc-500">
              Use the search bar above or refine with filters once you enter a query.
            </p>
          </section>
        ) : null}

        {hasQuery && booksQuery.isError ? (
          <section className="rounded-xl border border-red-200 bg-red-50 p-6 text-red-700">
            Unable to search right now.
          </section>
        ) : null}

        {hasQuery && !booksQuery.isLoading && !booksQuery.isError && books.length === 0 ? (
          <section className="rounded-xl border border-zinc-200 bg-white p-10 text-center">
            <h2 className="text-2xl font-semibold text-zinc-900">No matching books</h2>
            <p className="mt-2 text-zinc-500">Try a different query or clear the filters.</p>
          </section>
        ) : null}

        {hasQuery && !booksQuery.isLoading && books.length > 0 ? (
          <>
            <section className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6 2xl:grid-cols-8">
              {books.map((book) => (
                <BookCard key={book.id} book={book} score={book.score} />
              ))}
            </section>

            <footer className="flex items-center justify-between rounded-xl border border-zinc-200 bg-white p-4">
              <button
                type="button"
                onClick={() => updateSearchState({ page: Math.max(1, searchState.page - 1) })}
                disabled={searchState.page <= 1}
                className="rounded-lg border border-zinc-300 px-3 py-2 text-sm disabled:cursor-not-allowed disabled:opacity-50"
              >
                Previous
              </button>
              <p className="text-sm text-zinc-600">
                Page {searchState.page} of {totalPages}
              </p>
              <button
                type="button"
                onClick={() => updateSearchState({ page: Math.min(totalPages, searchState.page + 1) })}
                disabled={searchState.page >= totalPages}
                className="rounded-lg border border-zinc-300 px-3 py-2 text-sm disabled:cursor-not-allowed disabled:opacity-50"
              >
                Next
              </button>
            </footer>
          </>
        ) : null}
      </div>
    </main>
  );
}
