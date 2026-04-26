/**
 * ShelvesPage — personal reading shelves manager.
 *
 * Route: /shelves
 *
 * Layout:
 *   - Left sidebar: shelf list with book-count badges and a "Create" button
 *     that expands an inline form (name + public toggle).  The first shelf
 *     in the list is auto-selected on load.
 *   - Main area: ShelfBooksGrid for the selected shelf — a 2–6 column
 *     cover grid with a "Remove" overlay button on each book.
 *
 * Shelf creation: POST /api/v1/shelves; on success the newly created shelf
 * is selected and the shelves list is invalidated.
 *
 * Book removal: DELETE /api/v1/shelves/:id/books/:bookId; invalidates both
 * ["shelf-books", shelfId] and ["shelves"] so the sidebar count updates.
 *
 * API calls:
 *   GET    /api/v1/shelves                    — shelf list
 *   GET    /api/v1/shelves/:id/books          — books for selected shelf
 *   POST   /api/v1/shelves                    — create shelf
 *   DELETE /api/v1/shelves/:id/books/:bookId  — remove book from shelf
 */
import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { Shelf } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { BookCard } from "./BookCard";

const SHELF_PAGE_SIZE = 100;

function ShelfBooksGrid({
  shelfId,
  onRemoveBook,
}: {
  shelfId: string;
  onRemoveBook: (bookId: string) => void;
}) {
  const { t } = useTranslation();
  const booksQuery = useQuery({
    queryKey: ["shelf-books", shelfId],
    queryFn: () => apiClient.listShelfBooks(shelfId, { page: 1, page_size: SHELF_PAGE_SIZE }),
    enabled: Boolean(shelfId),
    staleTime: 30_000,
  });

  if (booksQuery.isLoading) {
    return <div className="rounded-xl border border-zinc-200 bg-white p-8 text-zinc-500">{t("shelves.loading_books")}</div>;
  }

  if (booksQuery.isError) {
    return <div className="rounded-xl border border-red-200 bg-red-50 p-6 text-red-700">{t("shelves.unable_to_load_books")}</div>;
  }

  const books = booksQuery.data?.items ?? [];

  if (books.length === 0) {
    return <div className="rounded-xl border border-zinc-200 bg-white p-8 text-zinc-500">{t("shelves.no_books_yet")}</div>;
  }

  return (
    <section className="grid grid-cols-2 gap-4 md:grid-cols-4 xl:grid-cols-6">
      {books.map((book) => (
        <div key={book.id} className="relative">
          <BookCard book={book} />
          <button
            type="button"
            onClick={() => onRemoveBook(book.id)}
            className="absolute right-2 top-2 rounded-full bg-zinc-950/85 px-3 py-1 text-xs font-semibold text-white shadow"
          >
            Remove
          </button>
        </div>
      ))}
    </section>
  );
}

/**
 * ShelvesPage renders the two-panel shelves view: a sidebar shelf list and
 * a book grid for the active shelf.
 */
export function ShelvesPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [selectedShelfId, setSelectedShelfId] = useState<string | null>(null);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [shelfName, setShelfName] = useState("");
  const [isPublic, setIsPublic] = useState(false);

  const shelvesQuery = useQuery({
    queryKey: ["shelves"],
    queryFn: () => apiClient.listShelves(),
    staleTime: 30_000,
  });

  useEffect(() => {
    const shelves = shelvesQuery.data ?? [];
    if (selectedShelfId && shelves.some((shelf) => shelf.id === selectedShelfId)) {
      return;
    }
    if (shelves.length > 0) {
      setSelectedShelfId(shelves[0].id);
    } else {
      setSelectedShelfId(null);
    }
  }, [shelvesQuery.data, selectedShelfId]);

  const selectedShelf = useMemo(() => {
    return shelvesQuery.data?.find((shelf) => shelf.id === selectedShelfId) ?? null;
  }, [selectedShelfId, shelvesQuery.data]);

  const createShelfMutation = useMutation({
    mutationFn: () =>
      apiClient.createShelf({
        name: shelfName.trim(),
        is_public: isPublic,
      }),
    onSuccess: (created) => {
      setShelfName("");
      setIsPublic(false);
      setShowCreateForm(false);
      setSelectedShelfId(created.id);
      queryClient.invalidateQueries({ queryKey: ["shelves"] });
    },
  });

  const removeBookMutation = useMutation({
    mutationFn: (bookId: string) => {
      if (!selectedShelfId) {
        throw new Error("Missing shelf");
      }
      return apiClient.removeBookFromShelf(selectedShelfId, bookId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["shelf-books", selectedShelfId] });
      queryClient.invalidateQueries({ queryKey: ["shelves"] });
    },
  });

  return (
    <main className="min-h-screen bg-zinc-50 px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto grid max-w-7xl gap-5 lg:grid-cols-[320px_1fr]">
        <aside className="rounded-xl border border-zinc-200 bg-white p-4 shadow-sm">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h1 className="text-2xl font-semibold">{t("shelves.page_title")}</h1>
              <p className="text-sm text-zinc-500">{t("shelves.subtitle")}</p>
            </div>
            <button
              type="button"
              onClick={() => setShowCreateForm((open) => !open)}
              className="rounded-lg bg-zinc-900 px-3 py-2 text-sm font-semibold text-white"
            >
              {t("shelves.create")}
            </button>
          </div>

          {showCreateForm ? (
            <form
              className="mt-4 space-y-3 rounded-xl border border-zinc-200 bg-zinc-50 p-3"
              onSubmit={(event) => {
                event.preventDefault();
                if (shelfName.trim().length === 0) {
                  return;
                }
                createShelfMutation.mutate();
              }}
            >
              <label className="block text-sm">
                <span className="mb-1 block font-medium text-zinc-700">{t("shelves.shelf_name")}</span>
                <input
                  value={shelfName}
                  onChange={(event) => setShelfName(event.target.value)}
                  className="w-full rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm"
                  placeholder={t("shelves.favorites_placeholder")}
                />
              </label>

              <label className="flex items-center gap-2 text-sm text-zinc-700">
                <input
                  type="checkbox"
                  checked={isPublic}
                  onChange={(event) => setIsPublic(event.target.checked)}
                />
                {t("shelves.public_shelf")}
              </label>

              <div className="flex items-center gap-2">
                <button
                  type="submit"
                  className="rounded-lg bg-teal-600 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50"
                  disabled={createShelfMutation.isPending}
                >
                  {t("common.save")}
                </button>
                <button
                  type="button"
                  onClick={() => setShowCreateForm(false)}
                  className="rounded-lg border border-zinc-300 px-3 py-2 text-sm font-semibold text-zinc-700"
                >
                  {t("common.cancel")}
                </button>
              </div>
            </form>
          ) : null}

          <div className="mt-4 space-y-2">
            {shelvesQuery.isLoading ? (
              <p className="text-sm text-zinc-500">{t("shelves.loading")}</p>
            ) : shelvesQuery.isError ? (
              <p className="text-sm text-red-600">{t("shelves.unable_to_load")}</p>
            ) : shelvesQuery.data?.length ? (
              shelvesQuery.data.map((shelf: Shelf) => {
                const active = shelf.id === selectedShelfId;
                return (
                  <button
                    key={shelf.id}
                    type="button"
                    onClick={() => setSelectedShelfId(shelf.id)}
                    className={`flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left transition ${
                      active
                        ? "border-teal-600 bg-teal-50"
                        : "border-zinc-200 bg-white hover:border-zinc-300"
                    }`}
                  >
                    <span className="min-w-0">
                      <span className="block truncate font-medium">{shelf.name}</span>
                      <span className="block text-xs text-zinc-500">
                        {shelf.is_public ? t("shelves.public") : t("shelves.private")}
                      </span>
                    </span>
                    <span className="rounded-full bg-zinc-100 px-2 py-0.5 text-xs text-zinc-600">
                      {shelf.book_count}
                    </span>
                  </button>
                );
              })
            ) : (
              <div className="rounded-lg border border-dashed border-zinc-300 p-4 text-sm text-zinc-500">
                {t("shelves.empty")}
              </div>
            )}
          </div>
        </aside>

        <section className="rounded-xl border border-zinc-200 bg-white p-4 shadow-sm md:p-6">
          {selectedShelf ? (
            <>
              <div className="mb-5 flex flex-wrap items-end justify-between gap-3">
                <div>
                  <h2 className="text-2xl font-semibold">{selectedShelf.name}</h2>
                  <p className="text-sm text-zinc-500">
                    {selectedShelf.is_public ? t("shelves.public_shelf") : t("shelves.private_shelf")} · {selectedShelf.book_count} {t("shelves.books")}
                  </p>
                </div>
              </div>

              <ShelfBooksGrid
                shelfId={selectedShelf.id}
                onRemoveBook={(bookId) => removeBookMutation.mutate(bookId)}
              />
            </>
          ) : (
            <div className="grid min-h-[320px] place-items-center rounded-xl border border-dashed border-zinc-300 text-zinc-500">
              {t("shelves.create_or_select")}
            </div>
          )}
        </section>
      </div>
    </main>
  );
}
