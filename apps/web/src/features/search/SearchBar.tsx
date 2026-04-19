import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import type { BookSummary } from "@calibre/shared";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";
import { CoverPlaceholder } from "../library/CoverPlaceholder";

type RecentSearch = {
  query: string;
  at: number;
};

const RECENT_SEARCH_LIMIT = 5;

function recentSearchStorageKey(userId: string | null): string {
  return `calibre-web.recent-searches:${userId ?? "anon"}`;
}

function readRecentSearches(userId: string | null): string[] {
  if (
    typeof localStorage === "undefined" ||
    typeof localStorage.getItem !== "function"
  ) {
    return [];
  }

  const raw = localStorage.getItem(recentSearchStorageKey(userId));
  if (!raw) {
    return [];
  }

  try {
    const parsed = JSON.parse(raw) as RecentSearch[];
    return parsed
      .filter((item) => typeof item?.query === "string")
      .sort((left, right) => right.at - left.at)
      .slice(0, RECENT_SEARCH_LIMIT)
      .map((item) => item.query);
  } catch {
    return [];
  }
}

function saveRecentSearch(userId: string | null, query: string): void {
  if (
    typeof localStorage === "undefined" ||
    typeof localStorage.getItem !== "function" ||
    typeof localStorage.setItem !== "function"
  ) {
    return;
  }

  const key = recentSearchStorageKey(userId);
  const existing = readRecentSearches(userId);
  const next = [query, ...existing.filter((item) => item.toLowerCase() !== query.toLowerCase())]
    .slice(0, RECENT_SEARCH_LIMIT)
    .map((item) => ({ query: item, at: Date.now() }));

  localStorage.setItem(key, JSON.stringify(next));
}

function authorLabel(book: BookSummary): string {
  if (book.authors.length === 0) {
    return "Unknown author";
  }

  return book.authors.map((author) => author.name).join(", ");
}

function SearchMiniCard({ book }: { book: BookSummary }) {
  return (
    <a
      href={`/books/${encodeURIComponent(book.id)}`}
      className="flex items-center gap-3 rounded-xl border border-zinc-200 bg-white p-2 text-left transition hover:border-teal-500 hover:shadow-sm"
    >
      <div className="w-10 shrink-0">
        {book.has_cover ? (
          <img
            src={apiClient.coverUrl(book.id)}
            alt={`${book.title} cover`}
            className="aspect-[2/3] w-full rounded-md object-cover"
          />
        ) : (
          <CoverPlaceholder title={book.title} className="rounded-md" />
        )}
      </div>

      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-semibold text-zinc-900">{book.title}</p>
        <p className="truncate text-xs text-zinc-500">{authorLabel(book)}</p>
      </div>
    </a>
  );
}

export function SearchBar() {
  const navigate = useNavigate();
  const user = useAuthStore((state) => state.user);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const closeTimerRef = useRef<number | null>(null);
  const recentSearches = useMemo(() => readRecentSearches(user?.id ?? null), [user?.id, open]);

  const suggestionsQuery = useQuery({
    queryKey: ["search-suggestions", query],
    queryFn: () => apiClient.listBooks({ q: query, page_size: 5, sort: "title" }),
    enabled: open && query.trim().length > 0,
  });

  useEffect(() => {
    return () => {
      if (closeTimerRef.current !== null) {
        window.clearTimeout(closeTimerRef.current);
      }
    };
  }, []);

  function commitSearch(nextQuery: string) {
    const trimmed = nextQuery.trim();
    if (!trimmed) {
      return;
    }

    saveRecentSearch(user?.id ?? null, trimmed);
    setQuery(trimmed);
    setOpen(false);
    void navigate({ to: "/search", search: { q: trimmed } });
  }

  function handleBlur() {
    closeTimerRef.current = window.setTimeout(() => {
      setOpen(false);
    }, 120);
  }

  const books = suggestionsQuery.data?.items ?? [];

  return (
    <div className="relative w-full max-w-[36rem]">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          commitSearch(query);
        }}
        className="relative"
      >
        <input
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={handleBlur}
          placeholder="Search title, author, tag"
          className={`w-full rounded-full border border-zinc-300 bg-white px-4 py-2.5 text-sm text-zinc-900 shadow-sm outline-none transition-all duration-200 focus:border-teal-500 ${
            open ? "ring-2 ring-teal-100" : ""
          }`}
        />
      </form>

      {open ? (
        <div className="absolute left-0 right-0 top-[calc(100%+0.5rem)] z-40 rounded-2xl border border-zinc-200 bg-zinc-50 p-3 shadow-2xl">
          {recentSearches.length > 0 ? (
            <section className="mb-4">
              <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">
                Recent searches
              </div>
              <div className="flex flex-wrap gap-2">
                {recentSearches.map((item) => (
                  <button
                    key={item}
                    type="button"
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => commitSearch(item)}
                    className="rounded-full border border-zinc-300 bg-white px-3 py-1.5 text-xs text-zinc-700 hover:border-teal-500"
                  >
                    {item}
                  </button>
                ))}
              </div>
            </section>
          ) : null}

          {query.trim().length > 0 ? (
            <section className="space-y-2">
              <div className="text-xs font-semibold uppercase tracking-wide text-zinc-500">
                Quick results
              </div>
              <div className="space-y-2">
                {books.length > 0 ? (
                  books.map((book) => <SearchMiniCard key={book.id} book={book} />)
                ) : (
                  <p className="px-1 py-2 text-sm text-zinc-500">No matching books.</p>
                )}
              </div>
            </section>
          ) : null}

          <div className="mt-3 flex items-center justify-between">
            <button
              type="button"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => commitSearch(query)}
              className="rounded-full bg-teal-600 px-3 py-1.5 text-xs font-semibold text-white"
            >
              Search
            </button>
            <a
              href={query.trim() ? `/search?q=${encodeURIComponent(query.trim())}` : "/search"}
              className="text-sm font-medium text-teal-700"
            >
              See all results →
            </a>
          </div>
        </div>
      ) : null}
    </div>
  );
}
