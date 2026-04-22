import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Shelf } from "@autolibre/shared";
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
  const booksQuery = useQuery({
    queryKey: ["shelf-books", shelfId],
    queryFn: () => apiClient.listShelfBooks(shelfId, { page: 1, page_size: SHELF_PAGE_SIZE }),
    enabled: Boolean(shelfId),
    staleTime: 30_000,
  });

  if (booksQuery.isLoading) {
    return <div className="rounded-xl border border-zinc-200 bg-white p-8 text-zinc-500">Loading shelf books...</div>;
  }

  if (booksQuery.isError) {
    return <div className="rounded-xl border border-red-200 bg-red-50 p-6 text-red-700">Unable to load shelf books.</div>;
  }

  const books = booksQuery.data?.items ?? [];

  if (books.length === 0) {
    return <div className="rounded-xl border border-zinc-200 bg-white p-8 text-zinc-500">No books on this shelf yet.</div>;
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

export function ShelvesPage() {
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
              <h1 className="text-2xl font-semibold">Shelves</h1>
              <p className="text-sm text-zinc-500">Organize books into reading lists.</p>
            </div>
            <button
              type="button"
              onClick={() => setShowCreateForm((open) => !open)}
              className="rounded-lg bg-zinc-900 px-3 py-2 text-sm font-semibold text-white"
            >
              Create
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
                <span className="mb-1 block font-medium text-zinc-700">Shelf name</span>
                <input
                  value={shelfName}
                  onChange={(event) => setShelfName(event.target.value)}
                  className="w-full rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm"
                  placeholder="Favorites"
                />
              </label>

              <label className="flex items-center gap-2 text-sm text-zinc-700">
                <input
                  type="checkbox"
                  checked={isPublic}
                  onChange={(event) => setIsPublic(event.target.checked)}
                />
                Public shelf
              </label>

              <div className="flex items-center gap-2">
                <button
                  type="submit"
                  className="rounded-lg bg-teal-600 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50"
                  disabled={createShelfMutation.isPending}
                >
                  Save
                </button>
                <button
                  type="button"
                  onClick={() => setShowCreateForm(false)}
                  className="rounded-lg border border-zinc-300 px-3 py-2 text-sm font-semibold text-zinc-700"
                >
                  Cancel
                </button>
              </div>
            </form>
          ) : null}

          <div className="mt-4 space-y-2">
            {shelvesQuery.isLoading ? (
              <p className="text-sm text-zinc-500">Loading shelves...</p>
            ) : shelvesQuery.isError ? (
              <p className="text-sm text-red-600">Unable to load shelves.</p>
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
                        {shelf.is_public ? "Public" : "Private"}
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
                No shelves yet.
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
                    {selectedShelf.is_public ? "Public shelf" : "Private shelf"} · {selectedShelf.book_count} books
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
              Create a shelf or select one from the list.
            </div>
          )}
        </section>
      </div>
    </main>
  );
}
