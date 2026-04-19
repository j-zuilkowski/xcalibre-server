import { useEffect, useMemo, useState } from "react";
import type { ListBooksParams } from "@calibre/shared";
import { useQuery } from "@tanstack/react-query";
import { apiClient } from "../../lib/api-client";
import { BookCard } from "./BookCard";
import { BookListRow } from "./BookListRow";

type ViewMode = "grid" | "list";

type LibrarySearchState = {
  author_id?: string;
  series_id?: string;
  tag?: string;
  language?: string;
  format?: string;
  sort?: string;
  page: number;
  view: ViewMode;
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
  if (Number.isNaN(parsed) || parsed < 1) {
    return 1;
  }
  return parsed;
}

function parseSearch(search: string): LibrarySearchState {
  const params = new URLSearchParams(search);
  const viewParam = params.get("view");
  const view = viewParam === "list" ? "list" : "grid";

  return {
    author_id: params.get("author_id") ?? undefined,
    series_id: params.get("series_id") ?? undefined,
    tag: params.get("tag") ?? undefined,
    language: params.get("language") ?? undefined,
    format: params.get("format") ?? undefined,
    sort: params.get("sort") ?? "title",
    page: parsePage(params.get("page")),
    view,
  };
}

function toSearch(state: LibrarySearchState): string {
  const params = new URLSearchParams();

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
  if (state.view !== "grid") {
    params.set("view", state.view);
  }

  return params.toString();
}

export function LibraryPage() {
  const [searchState, setSearchState] = useState<LibrarySearchState>(() =>
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

  const params = useMemo<ListBooksParams>(
    () => ({
      author_id: searchState.author_id,
      series_id: searchState.series_id,
      tag: searchState.tag,
      language: searchState.language,
      format: searchState.format,
      sort: searchState.sort,
      page: searchState.page,
      page_size: PAGE_SIZE,
    }),
    [searchState],
  );

  const booksQuery = useQuery({
    queryKey: ["books", params],
    queryFn: () => apiClient.listBooks(params),
  });

  const data = booksQuery.data;
  const books = data?.items ?? [];
  const pageSize = data?.page_size ?? PAGE_SIZE;
  const total = data?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));

  function updateSearchState(next: Partial<LibrarySearchState>) {
    setSearchState((previous) => {
      const updated: LibrarySearchState = {
        ...previous,
        ...next,
        page: next.page ?? previous.page,
      };

      const nextSearch = toSearch(updated);
      const nextUrl = nextSearch
        ? `${window.location.pathname}?${nextSearch}`
        : window.location.pathname;
      window.history.replaceState({}, "", nextUrl);

      return updated;
    });
  }

  function toggleFilter(key: keyof ListBooksParams, value: string) {
    const current = searchState[key as keyof LibrarySearchState];
    const nextValue = current === value ? undefined : value;

    updateSearchState({
      [key]: nextValue,
      page: 1,
    } as Partial<LibrarySearchState>);
  }

  return (
    <main className="min-h-screen bg-zinc-50 px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto flex max-w-[1440px] flex-col gap-5">
        <header className="flex flex-col gap-3 rounded-xl border border-zinc-200 bg-white p-4 shadow-sm">
          <div className="flex flex-wrap items-center gap-2">
            {FILTER_CHIPS.map((chip) => {
              const active = searchState[chip.key as keyof LibrarySearchState] === chip.value;
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
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <label htmlFor="sort" className="text-sm text-zinc-500">
                Sort
              </label>
              <select
                id="sort"
                value={searchState.sort ?? "title"}
                onChange={(event) =>
                  updateSearchState({ sort: event.target.value, page: 1 })
                }
                className="rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm"
              >
                <option value="title">Title</option>
                <option value="author">Author</option>
                <option value="created_at">Date Added</option>
                <option value="rating">Rating</option>
              </select>
            </div>

            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => updateSearchState({ view: "grid" })}
                className={`rounded-lg border px-3 py-2 text-sm ${
                  searchState.view === "grid"
                    ? "border-zinc-900 bg-zinc-900 text-zinc-50"
                    : "border-zinc-300 bg-white text-zinc-700"
                }`}
              >
                Grid
              </button>
              <button
                type="button"
                onClick={() => updateSearchState({ view: "list" })}
                className={`rounded-lg border px-3 py-2 text-sm ${
                  searchState.view === "list"
                    ? "border-zinc-900 bg-zinc-900 text-zinc-50"
                    : "border-zinc-300 bg-white text-zinc-700"
                }`}
              >
                List
              </button>
            </div>
          </div>
        </header>

        {booksQuery.isLoading ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6 2xl:grid-cols-8">
            {Array.from({ length: 8 }).map((_, index) => (
              <div key={index} className="aspect-[2/3] animate-pulse rounded-lg bg-zinc-200" />
            ))}
          </div>
        ) : null}

        {booksQuery.isError ? (
          <div className="rounded-xl border border-red-200 bg-red-50 p-6 text-red-700">
            Unable to load your library right now.
          </div>
        ) : null}

        {!booksQuery.isLoading && !booksQuery.isError && books.length === 0 ? (
          <section className="rounded-xl border border-zinc-200 bg-white p-10 text-center">
            <h1 className="text-2xl font-semibold text-zinc-900">No books in your library yet</h1>
            <p className="mt-2 text-zinc-500">Import your library to start browsing covers.</p>
            <a
              href="/admin/import"
              className="mt-5 inline-flex rounded-lg bg-teal-600 px-4 py-2 text-sm font-semibold text-white"
            >
              Import your library
            </a>
          </section>
        ) : null}

        {!booksQuery.isLoading && !booksQuery.isError && books.length > 0 ? (
          <>
            {searchState.view === "grid" ? (
              <section className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6 2xl:grid-cols-8">
                {books.map((book) => (
                  <BookCard key={book.id} book={book} />
                ))}
              </section>
            ) : (
              <section className="flex flex-col gap-2">
                {books.map((book) => (
                  <BookListRow key={book.id} book={book} />
                ))}
              </section>
            )}

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
