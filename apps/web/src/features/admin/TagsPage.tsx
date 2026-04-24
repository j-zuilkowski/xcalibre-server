import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { AdminTagWithCount, ApiError, TagLookupItem } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { Dialog } from "../../components/ui/Dialog";
import { TagAutocomplete } from "./TagAutocomplete";

const PAGE_SIZE = 20;

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timeout = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timeout);
  }, [delayMs, value]);

  return debounced;
}

export function TagsPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  const [query, setQuery] = useState("");
  const debouncedQuery = useDebouncedValue(query.trim(), 250);
  const [page, setPage] = useState(1);

  const [editingTagId, setEditingTagId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [renameError, setRenameError] = useState<string | null>(null);

  const [mergeTagId, setMergeTagId] = useState<string | null>(null);
  const [mergeTarget, setMergeTarget] = useState<TagLookupItem | null>(null);
  const [deleteCandidate, setDeleteCandidate] = useState<AdminTagWithCount | null>(null);
  const deleteDialogTitleId = useId();
  const deleteCancelRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    setPage(1);
  }, [debouncedQuery]);

  const tagsQuery = useQuery({
    queryKey: ["admin-tags-management", debouncedQuery, page],
    queryFn: () =>
      apiClient.listAdminTags({
        q: debouncedQuery,
        page,
        page_size: PAGE_SIZE,
      }),
  });

  const tags = tagsQuery.data?.items ?? [];
  const total = tagsQuery.data?.total ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  useEffect(() => {
    if (page > pageCount) {
      setPage(pageCount);
    }
  }, [page, pageCount]);

  const renameMutation = useMutation({
    mutationFn: (payload: { id: string; name: string }) =>
      apiClient.renameAdminTag(payload.id, payload.name),
    onSuccess: async () => {
      setEditingTagId(null);
      setEditingName("");
      setRenameError(null);
      await queryClient.invalidateQueries({ queryKey: ["admin-tags-management"] });
    },
    onError: (error) => {
      const apiError = error as ApiError;
      if (apiError.status === 409) {
        setRenameError("Tag name already exists.");
        return;
      }
      setRenameError("Unable to rename tag.");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiClient.deleteAdminTag(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-tags-management"] });
    },
  });

  const mergeMutation = useMutation({
    mutationFn: (payload: { id: string; intoTagId: string }) =>
      apiClient.mergeAdminTag(payload.id, payload.intoTagId),
    onSuccess: async () => {
      setMergeTagId(null);
      setMergeTarget(null);
      await queryClient.invalidateQueries({ queryKey: ["admin-tags-management"] });
    },
  });

  const isBusy = renameMutation.isPending || deleteMutation.isPending || mergeMutation.isPending;

  const activeMergeRow = useMemo(
    () => tags.find((tag) => tag.id === mergeTagId) ?? null,
    [mergeTagId, tags],
  );

  function beginRename(tag: AdminTagWithCount) {
    setRenameError(null);
    setEditingTagId(tag.id);
    setEditingName(tag.name);
  }

  function cancelRename() {
    setEditingTagId(null);
    setEditingName("");
    setRenameError(null);
  }

  async function saveRename() {
    const nextName = editingName.trim();
    if (!editingTagId || !nextName) {
      return;
    }
    await renameMutation.mutateAsync({ id: editingTagId, name: nextName });
  }

  async function confirmDelete(tag: AdminTagWithCount) {
    setDeleteCandidate(tag);
  }

  async function confirmMerge() {
    if (!mergeTagId || !mergeTarget || mergeTarget.id === mergeTagId) {
      return;
    }
    await mergeMutation.mutateAsync({ id: mergeTagId, intoTagId: mergeTarget.id });
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.tags")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">Global tag management</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <div className="grid gap-3 md:grid-cols-[1fr_auto] md:items-center">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={`${t("common.search")} tags`}
            aria-label={`${t("common.search")} tags`}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-400"
          />
          <p className="text-sm text-zinc-400">{total} total</p>
        </div>
      </section>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th scope="col" className="px-4 py-3 font-medium">{t("common.name")}</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("library.books")}</th>
              <th scope="col" className="px-4 py-3 font-medium">Confirmed</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("common.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {tags.map((tag) => {
              const canMergeIntoSelected = mergeTarget && mergeTarget.id !== tag.id;
              return (
                <tr key={tag.id} className="border-t border-zinc-800 align-top">
                  <td className="px-4 py-3 text-zinc-100">
                    {editingTagId === tag.id ? (
                      <div className="space-y-2">
                        <input
                          value={editingName}
                          onChange={(event) => setEditingName(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              event.preventDefault();
                              void saveRename();
                            }
                            if (event.key === "Escape") {
                              cancelRename();
                            }
                          }}
                          className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
                        />
                        <div className="flex gap-2">
                          <button
                            type="button"
                            disabled={renameMutation.isPending}
                            onClick={() => void saveRename()}
                            className="rounded-lg border border-teal-500 px-3 py-1.5 text-xs text-teal-300"
                          >
                            {t("common.save")}
                          </button>
                          <button
                            type="button"
                            onClick={cancelRename}
                            className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-300"
                          >
                            {t("common.cancel")}
                          </button>
                        </div>
                        {renameError ? <p className="text-xs text-red-300">{renameError}</p> : null}
                      </div>
                    ) : (
                      <button
                        type="button"
                        onClick={() => beginRename(tag)}
                        className="text-left font-medium text-zinc-100 hover:text-teal-300"
                      >
                        {tag.name}
                      </button>
                    )}
                  </td>
                  <td className="px-4 py-3 text-zinc-200">{tag.book_count}</td>
                  <td className="px-4 py-3 text-zinc-200">{tag.confirmed_count}</td>
                  <td className="px-4 py-3">
                    <div className="space-y-2">
                      <div className="flex flex-wrap gap-2">
                        <button
                          type="button"
                          onClick={() => {
                            setMergeTagId(tag.id);
                            setMergeTarget(null);
                          }}
                          className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-200"
                        >
                          Merge
                        </button>
                        <button
                          type="button"
                          disabled={deleteMutation.isPending}
                          onClick={() => void confirmDelete(tag)}
                          className="rounded-lg border border-red-500 px-3 py-1.5 text-xs text-red-300"
                        >
                          {t("common.delete")}
                        </button>
                      </div>

                      {mergeTagId === tag.id ? (
                        <div className="space-y-2 rounded-xl border border-zinc-700 bg-zinc-950/70 p-2">
                          <TagAutocomplete
                            onSelect={setMergeTarget}
                            placeholder="Search target tag"
                            disabled={mergeMutation.isPending}
                            className="placeholder:text-zinc-400"
                          />
                          {mergeTarget ? (
                            <p className="text-xs text-zinc-400">
                              Target: <span className="text-zinc-200">{mergeTarget.name}</span>
                            </p>
                          ) : null}
                          <div className="flex flex-wrap gap-2">
                            <button
                              type="button"
                              disabled={!canMergeIntoSelected || mergeMutation.isPending}
                              onClick={() => void confirmMerge()}
                              className="rounded-lg border border-teal-500 px-3 py-1.5 text-xs text-teal-300 disabled:border-zinc-700 disabled:text-zinc-500"
                            >
                              {t("book.confirm_merge")}
                            </button>
                            <button
                              type="button"
                              onClick={() => {
                                setMergeTagId(null);
                                setMergeTarget(null);
                              }}
                              className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-300"
                            >
                              {t("common.cancel")}
                            </button>
                          </div>
                        </div>
                      ) : null}
                    </div>
                  </td>
                </tr>
              );
            })}

            {tags.length === 0 ? (
              <tr className="border-t border-zinc-800">
                <td colSpan={4} className="px-4 py-6 text-center text-zinc-500">
                  {tagsQuery.isFetching ? t("common.searching") : "No tags found."}
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>

      <footer className="flex items-center justify-between">
        <p className="text-sm text-zinc-400">
          Page {page} of {pageCount}
        </p>
        <div className="flex gap-2">
          <button
            type="button"
            disabled={page <= 1 || isBusy}
            onClick={() => setPage((previous) => Math.max(1, previous - 1))}
            className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200 disabled:text-zinc-500"
          >
            {t("common.previous")}
          </button>
          <button
            type="button"
            disabled={page >= pageCount || isBusy}
            onClick={() => setPage((previous) => Math.min(pageCount, previous + 1))}
            className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200 disabled:text-zinc-500"
          >
            {t("common.next")}
          </button>
        </div>
      </footer>

      {activeMergeRow && mergeTarget && mergeTarget.id === activeMergeRow.id ? (
        <p className="text-sm text-amber-300">Target tag must be different from source tag.</p>
      ) : null}

      <Dialog
        open={deleteCandidate !== null}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteCandidate(null);
          }
        }}
        titleId={deleteDialogTitleId}
        initialFocusRef={deleteCancelRef}
      >
        <div className="mx-auto w-full max-w-md rounded-2xl border border-zinc-800 bg-zinc-950 p-5 text-zinc-100 shadow-2xl">
          <h3 id={deleteDialogTitleId} className="text-xl font-semibold text-zinc-50">
            Remove tag?
          </h3>
          <p className="mt-2 text-sm text-zinc-400">
            This will remove "{deleteCandidate?.name}" from {deleteCandidate?.book_count ?? 0} books.
          </p>
          <div className="mt-5 flex justify-end gap-2">
            <button
              ref={deleteCancelRef}
              type="button"
              onClick={() => setDeleteCandidate(null)}
              className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200"
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              onClick={() => {
                if (deleteCandidate) {
                  void deleteMutation.mutateAsync(deleteCandidate.id);
                }
                setDeleteCandidate(null);
              }}
              className="rounded-lg border border-red-500 px-3 py-2 text-sm text-red-300"
            >
              {t("common.delete")}
            </button>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
