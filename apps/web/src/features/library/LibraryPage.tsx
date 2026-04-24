import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { ListBooksParams } from "@autolibre/shared";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
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
  show_archived: boolean;
  page: number;
  view: ViewMode;
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
    show_archived: params.get("show_archived") === "true",
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
  if (state.show_archived) {
    params.set("show_archived", "true");
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
  const { t } = useTranslation();
  const [searchState, setSearchState] = useState<LibrarySearchState>(() =>
    parseSearch(window.location.search),
  );
  const gridRef = useRef<HTMLUListElement | null>(null);

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
      show_archived: searchState.show_archived ? true : undefined,
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

  function focusCardAtIndex(index: number) {
    const container = gridRef.current;
    if (!container) {
      return;
    }

    const cards = Array.from(container.querySelectorAll<HTMLElement>("[data-book-card='true']"));
    const target = cards[index];
    target?.focus();
  }

  function handleGridKeyDown(event: KeyboardEvent<HTMLUListElement>) {
    const container = gridRef.current;
    if (!container || searchState.view !== "grid") {
      return;
    }

    const cards = Array.from(container.querySelectorAll<HTMLElement>("[data-book-card='true']"));
    if (cards.length === 0) {
      return;
    }

    const activeElement = document.activeElement as HTMLElement | null;
    const currentCard = activeElement?.closest("[data-book-card='true']") as HTMLElement | null;
    const currentIndex = currentCard ? cards.indexOf(currentCard) : 0;
    if (currentIndex < 0) {
      return;
    }

    const computedColumns = window.getComputedStyle(container).gridTemplateColumns;
    const columns = Math.max(1, computedColumns.split(" ").length);
    let nextIndex = currentIndex;

    if (event.key === "ArrowRight") {
      nextIndex = Math.min(cards.length - 1, currentIndex + 1);
    } else if (event.key === "ArrowLeft") {
      nextIndex = Math.max(0, currentIndex - 1);
    } else if (event.key === "ArrowDown") {
      nextIndex = Math.min(cards.length - 1, currentIndex + columns);
    } else if (event.key === "ArrowUp") {
      nextIndex = Math.max(0, currentIndex - columns);
    } else {
      return;
    }

    event.preventDefault();
    focusCardAtIndex(nextIndex);
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
                  {t(`library.${chip.label}`)}
                </button>
              );
            })}
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <label htmlFor="sort" className="text-sm text-zinc-500">
                {t("library.sort")}
              </label>
              <select
                id="sort"
                value={searchState.sort ?? "title"}
                onChange={(event) =>
                  updateSearchState({ sort: event.target.value, page: 1 })
                }
                className="rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm"
              >
                <option value="title">{t("library.title")}</option>
                <option value="author">{t("library.author")}</option>
                <option value="created_at">{t("library.date_added")}</option>
                <option value="rating">{t("library.rating")}</option>
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
                {t("library.grid")}
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
                  {t("library.list")}
                </button>
            </div>

            <button
              type="button"
              onClick={() =>
                updateSearchState({
                  show_archived: !searchState.show_archived,
                  page: 1,
                })
              }
              className={`rounded-lg border px-3 py-2 text-sm ${
                searchState.show_archived
                  ? "border-amber-600 bg-amber-600 text-white"
                  : "border-zinc-300 bg-white text-zinc-700"
              }`}
            >
              {t("library.show_archived")}
            </button>
          </div>
        </header>

        <div aria-live="polite" aria-atomic="true" className="sr-only">
          {booksQuery.isLoading ? "Loading books..." : `${books.length} books loaded`}
        </div>

        {booksQuery.isLoading ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6 2xl:grid-cols-8">
            {Array.from({ length: 8 }).map((_, index) => (
              <div key={index} className="aspect-[2/3] animate-pulse rounded-lg bg-zinc-200" />
            ))}
          </div>
        ) : null}

        {booksQuery.isError ? (
          <div className="rounded-xl border border-red-200 bg-red-50 p-6 text-red-700">
            {t("library.unable_to_load")}
          </div>
        ) : null}

        {!booksQuery.isLoading && !booksQuery.isError && books.length === 0 ? (
          <section className="rounded-xl border border-zinc-200 bg-white p-10 text-center">
            <h1 className="text-2xl font-semibold text-zinc-900">{t("library.empty_title")}</h1>
            <p className="mt-2 text-zinc-500">{t("library.empty_subtitle")}</p>
            <a
              href="/admin/import"
              className="mt-5 inline-flex rounded-lg bg-teal-600 px-4 py-2 text-sm font-semibold text-white"
            >
              {t("library.import_library")}
            </a>
          </section>
        ) : null}

        {!booksQuery.isLoading && !booksQuery.isError && books.length > 0 ? (
          <>
            {searchState.view === "grid" ? (
              <ul
                ref={gridRef}
                role="list"
                onKeyDown={handleGridKeyDown}
                className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6 2xl:grid-cols-8"
              >
                {books.map((book) => (
                  <li key={book.id}>
                    <BookCard book={book} progressPercentage={book.progress_percentage ?? 0} />
                  </li>
                ))}
              </ul>
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
