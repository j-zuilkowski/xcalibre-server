import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import type { BookSummary } from "@xs/shared";
import { useTranslation } from "react-i18next";
import { apiClient } from "../../lib/api-client";
import { BookCard } from "./BookCard";

type BrowsePageProps = {
  documentType: string;
};

const ALPHABET = ["#", ...Array.from({ length: 26 }, (_, index) => String.fromCharCode(65 + index))];

function letterForTitle(title: string): string {
  const firstCharacter = title.trim().charAt(0).toUpperCase();
  return /^[A-Z]$/.test(firstCharacter) ? firstCharacter : "#";
}

function browseTitleKey(documentType: string): string {
  if (documentType === "Book") {
    return "browse.books";
  }
  if (documentType === "Reference") {
    return "browse.reference";
  }
  if (documentType === "Periodical") {
    return "browse.periodicals";
  }
  if (documentType === "Magazine") {
    return "browse.magazines";
  }

  return "browse.books";
}

function groupBooks(books: BookSummary[]) {
  const grouped = new Map<string, BookSummary[]>();

  for (const book of books) {
    const letter = letterForTitle(book.title);
    const existing = grouped.get(letter) ?? [];
    existing.push(book);
    grouped.set(letter, existing);
  }

  return ALPHABET.map((letter) => ({
    letter,
    books: grouped.get(letter) ?? [],
  })).filter((group) => group.books.length > 0);
}

export function BrowsePage({ documentType }: BrowsePageProps) {
  const { t } = useTranslation();

  const booksQuery = useQuery({
    queryKey: ["browse-books", documentType],
    queryFn: () =>
      apiClient.listBooks({
        document_type: documentType,
        sort: "title",
        order: "asc",
        page_size: 200,
      } as Parameters<typeof apiClient.listBooks>[0]),
  });

  const books = booksQuery.data?.items ?? [];
  const groups = useMemo(() => groupBooks(books), [books]);
  const lettersWithBooks = useMemo(
    () => new Set(books.map((book) => letterForTitle(book.title))),
    [books],
  );

  function scrollToLetter(letter: string) {
    const target = document.getElementById(`alpha-${letter}`);
    target?.scrollIntoView({ behavior: "smooth" });
  }

  return (
    <main className="min-h-screen bg-zinc-50 px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto flex max-w-[1440px] gap-4">
        <aside className="sticky top-20 h-[calc(100vh-6rem)] w-10 shrink-0">
          <nav aria-label="Alphabetical navigation" className="flex flex-col gap-1">
            {ALPHABET.map((letter) => {
              const active = lettersWithBooks.has(letter);

              return (
                <button
                  key={letter}
                  type="button"
                  disabled={!active}
                  onClick={() => scrollToLetter(letter)}
                  className={`grid h-8 w-8 place-items-center rounded-md text-xs font-semibold transition ${
                    active
                      ? "border border-zinc-200 bg-white text-zinc-700 hover:border-teal-300 hover:text-teal-700"
                      : "pointer-events-none border border-transparent text-zinc-300 opacity-40"
                  }`}
                >
                  {letter}
                </button>
              );
            })}
          </nav>
        </aside>

        <section className="min-w-0 flex-1 space-y-8">
          <header className="space-y-2">
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-teal-700">
              {t(browseTitleKey(documentType))}
            </p>
            <h1 className="text-3xl font-semibold tracking-tight text-zinc-950">
              {t(browseTitleKey(documentType))}
            </h1>
          </header>

          {booksQuery.isLoading ? (
            <p className="text-sm text-zinc-500">{t("common.loading")}</p>
          ) : groups.length === 0 ? (
            <p className="rounded-2xl border border-dashed border-zinc-200 bg-white px-4 py-6 text-sm text-zinc-500">
              {t("browse.no_results")}
            </p>
          ) : (
            <div className="space-y-8">
              {groups.map((group) => (
                <section key={group.letter} id={`alpha-${group.letter}`} className="scroll-mt-24">
                  <div className="mb-3 flex items-center justify-between gap-3">
                    <h2 className="text-lg font-semibold text-zinc-900">{group.letter}</h2>
                    <span className="text-xs font-medium uppercase tracking-[0.16em] text-zinc-400">
                      {group.books.length}
                    </span>
                  </div>

                  <div className="grid grid-cols-3 gap-4 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6">
                    {group.books.map((book) => (
                      <BookCard
                        key={book.id}
                        book={book}
                        progressPercentage={book.progress_percentage ?? 0}
                      />
                    ))}
                  </div>
                </section>
              ))}
            </div>
          )}
        </section>
      </div>
    </main>
  );
}
