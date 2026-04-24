import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { CollectionDetail, CollectionDomain, CollectionSummary, BookSummary } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "../../components/ui/Sheet";

const DOMAIN_OPTIONS: Array<{ value: CollectionDomain; label: string }> = [
  { value: "technical", label: "Technical" },
  { value: "electronics", label: "Electronics" },
  { value: "culinary", label: "Culinary" },
  { value: "legal", label: "Legal" },
  { value: "academic", label: "Academic" },
  { value: "narrative", label: "Narrative" },
];

function bookAuthorsLabel(book: BookSummary): string {
  return book.authors.map((author) => author.name).join(", ") || "Unknown author";
}

export function CollectionsPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  const [newName, setNewName] = useState("");
  const [newDescription, setNewDescription] = useState("");
  const [newDomain, setNewDomain] = useState<CollectionDomain>("technical");
  const [newIsPublic, setNewIsPublic] = useState(false);

  const [editingCollectionId, setEditingCollectionId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [editDomain, setEditDomain] = useState<CollectionDomain>("technical");
  const [editIsPublic, setEditIsPublic] = useState(false);
  const [bookQuery, setBookQuery] = useState("");

  const collectionsQuery = useQuery({
    queryKey: ["admin-collections"],
    queryFn: () => apiClient.listCollections(),
  });

  const editCollectionQuery = useQuery({
    queryKey: ["admin-collection-detail", editingCollectionId],
    queryFn: () => apiClient.getCollection(editingCollectionId as string),
    enabled: editingCollectionId !== null,
  });

  const bookPickerQuery = useQuery({
    queryKey: ["admin-collection-book-picker", bookQuery],
    queryFn: () =>
      apiClient.listBooks({
        q: bookQuery.trim() || undefined,
        page: 1,
        page_size: 10,
      }),
    enabled: editingCollectionId !== null,
  });

  const createMutation = useMutation({
    mutationFn: () =>
      apiClient.createCollection({
        name: newName,
        description: newDescription || undefined,
        domain: newDomain,
        is_public: newIsPublic,
      }),
    onSuccess: async () => {
      setNewName("");
      setNewDescription("");
      setNewDomain("technical");
      setNewIsPublic(false);
      await queryClient.invalidateQueries({ queryKey: ["admin-collections"] });
    },
  });

  const updateMutation = useMutation({
    mutationFn: () => {
      if (!editingCollectionId) {
        throw new Error("missing collection id");
      }

      return apiClient.updateCollection(editingCollectionId, {
        name: editName,
        description: editDescription || null,
        domain: editDomain,
        is_public: editIsPublic,
      });
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-collections"] });
      await queryClient.invalidateQueries({ queryKey: ["admin-collection-detail", editingCollectionId] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (collectionId: string) => apiClient.deleteCollection(collectionId),
    onSuccess: async () => {
      setEditingCollectionId(null);
      await queryClient.invalidateQueries({ queryKey: ["admin-collections"] });
    },
  });

  const addBookMutation = useMutation({
    mutationFn: (bookId: string) => {
      if (!editingCollectionId) {
        throw new Error("missing collection id");
      }
      return apiClient.addBooksToCollection(editingCollectionId, [bookId]);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-collections"] });
      await queryClient.invalidateQueries({ queryKey: ["admin-collection-detail", editingCollectionId] });
    },
  });

  const removeBookMutation = useMutation({
    mutationFn: (bookId: string) => {
      if (!editingCollectionId) {
        throw new Error("missing collection id");
      }
      return apiClient.removeBookFromCollection(editingCollectionId, bookId);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-collections"] });
      await queryClient.invalidateQueries({ queryKey: ["admin-collection-detail", editingCollectionId] });
    },
  });

  const collections = collectionsQuery.data ?? [];
  const editCollection = editCollectionQuery.data;
  const editBooks = editCollection?.books ?? [];
  const pickerBooks = bookPickerQuery.data?.items ?? [];

  useEffect(() => {
    if (!editCollection) {
      return;
    }

    setEditName(editCollection.name);
    setEditDescription(editCollection.description ?? "");
    setEditDomain(editCollection.domain);
    setEditIsPublic(editCollection.is_public);
  }, [editCollection?.id]);

  const visibleCount = useMemo(() => collections.length, [collections]);

  async function handleCreate(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!newName.trim()) {
      return;
    }
    await createMutation.mutateAsync();
  }

  async function handleDelete(collection: CollectionSummary) {
    if (!window.confirm(`Delete collection "${collection.name}"?`)) {
      return;
    }
    await deleteMutation.mutateAsync(collection.id);
  }

  function beginEdit(collectionId: string) {
    setEditingCollectionId(collectionId);
    setBookQuery("");
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">Collections</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">Collection management</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <form onSubmit={(event) => void handleCreate(event)} className="grid gap-3 md:grid-cols-4">
          <input
            value={newName}
            onChange={(event) => setNewName(event.target.value)}
            placeholder="Collection name"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400 md:col-span-1"
          />
          <input
            value={newDescription}
            onChange={(event) => setNewDescription(event.target.value)}
            placeholder="Description"
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400 md:col-span-1"
          />
          <select
            value={newDomain}
            onChange={(event) => setNewDomain(event.target.value as CollectionDomain)}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 md:col-span-1"
          >
            {DOMAIN_OPTIONS.map((domain) => (
              <option key={domain.value} value={domain.value}>
                {domain.label}
              </option>
            ))}
          </select>
          <div className="flex items-center gap-3 md:col-span-1 md:justify-end">
            <label className="flex items-center gap-2 text-sm text-zinc-300">
              <input
                type="checkbox"
                checked={newIsPublic}
                onChange={(event) => setNewIsPublic(event.target.checked)}
                className="rounded border-zinc-600 bg-zinc-950"
              />
              Public
            </label>
            <button
              type="submit"
              disabled={createMutation.isPending || !newName.trim()}
              className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-50"
            >
              New collection
            </button>
          </div>
        </form>
      </section>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th className="px-4 py-3 font-medium">Name</th>
              <th className="px-4 py-3 font-medium">Domain</th>
              <th className="px-4 py-3 font-medium">Books</th>
              <th className="px-4 py-3 font-medium">Public</th>
              <th className="px-4 py-3 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {collections.map((collection) => (
              <tr key={collection.id} className="border-t border-zinc-800 align-top">
                <td className="px-4 py-3 text-zinc-100">
                  <div className="font-medium">{collection.name}</div>
                  <div className="text-xs text-zinc-500">{collection.description ?? "No description"}</div>
                </td>
                <td className="px-4 py-3 text-zinc-300">{collection.domain}</td>
                <td className="px-4 py-3 text-zinc-300">
                  {collection.book_count} books, {collection.total_chunks} chunks
                </td>
                <td className="px-4 py-3 text-zinc-300">{collection.is_public ? "Yes" : "No"}</td>
                <td className="px-4 py-3">
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => beginEdit(collection.id)}
                      className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200"
                    >
                      Edit
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleDelete(collection)}
                      className="rounded-lg border border-red-500 px-3 py-1.5 text-xs text-red-300"
                    >
                      Delete
                    </button>
                  </div>
                </td>
              </tr>
            ))}

            {!collectionsQuery.isLoading && visibleCount === 0 ? (
              <tr>
                <td colSpan={5} className="px-4 py-8 text-center text-sm text-zinc-400">
                  No collections found.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>

      <Sheet
        open={editingCollectionId !== null}
        onOpenChange={(open) => {
          if (!open) {
            setEditingCollectionId(null);
            setBookQuery("");
          }
        }}
      >
        <SheetContent side="right" className="max-w-2xl">
          <SheetHeader>
            <SheetTitle>Edit collection</SheetTitle>
          </SheetHeader>
          {editCollection ? (
            <div className="flex h-full flex-col gap-5 p-5 text-zinc-100">
              <div className="grid gap-3 md:grid-cols-2">
                <label className="space-y-2">
                  <span className="text-xs uppercase tracking-[0.2em] text-zinc-500">Name</span>
                  <input
                    value={editName}
                    onChange={(event) => setEditName(event.target.value)}
                    className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
                  />
                </label>
                <label className="space-y-2">
                  <span className="text-xs uppercase tracking-[0.2em] text-zinc-500">Domain</span>
                  <select
                    value={editDomain}
                    onChange={(event) => setEditDomain(event.target.value as CollectionDomain)}
                    className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
                  >
                    {DOMAIN_OPTIONS.map((domain) => (
                      <option key={domain.value} value={domain.value}>
                        {domain.label}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              <label className="space-y-2">
                <span className="text-xs uppercase tracking-[0.2em] text-zinc-500">Description</span>
                <textarea
                  value={editDescription}
                  onChange={(event) => setEditDescription(event.target.value)}
                  rows={3}
                  className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
                />
              </label>

              <label className="flex items-center gap-2 text-sm text-zinc-300">
                <input
                  type="checkbox"
                  checked={editIsPublic}
                  onChange={(event) => setEditIsPublic(event.target.checked)}
                  className="rounded border-zinc-600 bg-zinc-950"
                />
                Public collection
              </label>

              <div className="flex gap-3">
                <button
                  type="button"
                  onClick={() => void updateMutation.mutateAsync()}
                  disabled={updateMutation.isPending || !editName.trim()}
                  className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-50"
                >
                  Save changes
                </button>
              </div>

              <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-4">
                <div className="mb-3">
                  <h3 className="text-lg font-semibold">Book picker</h3>
                  <p className="text-sm text-zinc-400">Search the library and add books to this collection.</p>
                </div>

                <input
                  value={bookQuery}
                  onChange={(event) => setBookQuery(event.target.value)}
                  placeholder="Search books"
                  className="mb-4 w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400"
                />

                <div className="space-y-2">
                  {pickerBooks.map((book) => {
                    const inCollection = editBooks.some((item) => item.id === book.id);
                    return (
                      <div
                        key={book.id}
                        className="flex items-center justify-between gap-3 rounded-xl border border-zinc-800 bg-zinc-950/60 px-3 py-2"
                      >
                        <div className="min-w-0">
                          <p className="truncate text-sm font-medium text-zinc-100">{book.title}</p>
                          <p className="truncate text-xs text-zinc-500">{bookAuthorsLabel(book)}</p>
                        </div>
                        <button
                          type="button"
                          onClick={() => void addBookMutation.mutateAsync(book.id)}
                          disabled={inCollection || addBookMutation.isPending}
                          className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200 disabled:opacity-50"
                        >
                          {inCollection ? "Added" : "Add"}
                        </button>
                      </div>
                    );
                  })}
                </div>
              </section>

              <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-4">
                <h3 className="mb-3 text-lg font-semibold">Books in collection</h3>
                <div className="space-y-2">
                  {editBooks.map((book) => (
                    <div
                      key={book.id}
                      className="flex items-center justify-between gap-3 rounded-xl border border-zinc-800 bg-zinc-950/60 px-3 py-2"
                    >
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium text-zinc-100">{book.title}</p>
                        <p className="truncate text-xs text-zinc-500">{bookAuthorsLabel(book)}</p>
                      </div>
                      <button
                        type="button"
                        onClick={() => void removeBookMutation.mutateAsync(book.id)}
                        disabled={removeBookMutation.isPending}
                        className="rounded-lg border border-red-500 px-3 py-1.5 text-xs text-red-300 disabled:opacity-50"
                      >
                        Remove
                      </button>
                    </div>
                  ))}

                  {editBooks.length === 0 ? (
                    <p className="text-sm text-zinc-500">No books in this collection yet.</p>
                  ) : null}
                </div>
              </section>
            </div>
          ) : null}
        </SheetContent>
      </Sheet>
    </div>
  );
}
