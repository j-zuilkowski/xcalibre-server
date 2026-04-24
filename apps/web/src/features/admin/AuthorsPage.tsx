import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { AdminAuthor } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "../../components/ui/Sheet";
import { AuthorAutocomplete } from "./AuthorAutocomplete";

const PAGE_SIZE = 20;

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timeout = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timeout);
  }, [delayMs, value]);

  return debounced;
}

export function AuthorsPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  const [query, setQuery] = useState("");
  const debouncedQuery = useDebouncedValue(query.trim(), 250);
  const [page, setPage] = useState(1);
  const [mergeSource, setMergeSource] = useState<AdminAuthor | null>(null);
  const [mergeTarget, setMergeTarget] = useState<AdminAuthor | null>(null);

  useEffect(() => {
    setPage(1);
  }, [debouncedQuery]);

  const authorsQuery = useQuery({
    queryKey: ["admin-authors-management", debouncedQuery, page],
    queryFn: () =>
      apiClient.listAdminAuthors({
        q: debouncedQuery,
        page,
        page_size: PAGE_SIZE,
      }),
  });

  const authors = authorsQuery.data?.items ?? [];
  const total = authorsQuery.data?.total ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  useEffect(() => {
    if (page > pageCount) {
      setPage(pageCount);
    }
  }, [page, pageCount]);

  const mergeMutation = useMutation({
    mutationFn: (payload: { sourceId: string; targetId: string }) =>
      apiClient.mergeAuthor(payload.sourceId, payload.targetId),
    onSuccess: async () => {
      setMergeSource(null);
      setMergeTarget(null);
      await queryClient.invalidateQueries({ queryKey: ["admin-authors-management"] });
    },
  });

  function beginMerge(author: AdminAuthor) {
    setMergeSource(author);
    setMergeTarget(null);
  }

  async function confirmMerge() {
    if (!mergeSource || !mergeTarget || mergeSource.id === mergeTarget.id) {
      return;
    }
    await mergeMutation.mutateAsync({ sourceId: mergeSource.id, targetId: mergeTarget.id });
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.authors")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">Author management</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <div className="grid gap-3 md:grid-cols-[1fr_auto] md:items-center">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={`${t("common.search")} authors`}
            aria-label={`${t("common.search")} authors`}
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
              <th scope="col" className="px-4 py-3 font-medium">Has profile</th>
              <th scope="col" className="px-4 py-3 font-medium">{t("common.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {authors.map((author) => (
              <tr key={author.id} className="border-t border-zinc-800 align-top">
                <td className="px-4 py-3 text-zinc-100">
                  <div className="font-medium">
                    <a href={`/authors/${encodeURIComponent(author.id)}`} className="hover:text-teal-300">
                      {author.name}
                    </a>
                  </div>
                  <div className="text-xs text-zinc-500">{author.sort_name}</div>
                </td>
                <td className="px-4 py-3 text-zinc-300">{author.book_count}</td>
                <td className="px-4 py-3 text-zinc-300">{author.has_profile ? "Yes" : "No"}</td>
                <td className="px-4 py-3">
                  <div className="flex flex-wrap gap-2">
                    <a
                      href={`/authors/${encodeURIComponent(author.id)}`}
                      className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs font-semibold text-zinc-100"
                    >
                      Edit profile
                    </a>
                    <button
                      type="button"
                      onClick={() => beginMerge(author)}
                      className="rounded-lg bg-teal-500 px-3 py-1.5 text-xs font-semibold text-zinc-950"
                    >
                      Merge
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <div className="flex items-center justify-between gap-3">
        <button
          type="button"
          onClick={() => setPage((current) => Math.max(1, current - 1))}
          disabled={page <= 1}
          className="rounded-lg border border-zinc-700 px-3 py-2 text-sm font-semibold text-zinc-100 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {t("common.previous")}
        </button>
        <p className="text-sm text-zinc-400">
          {t("common.page_of", { page, total: pageCount })}
        </p>
        <button
          type="button"
          onClick={() => setPage((current) => Math.min(pageCount, current + 1))}
          disabled={page >= pageCount}
          className="rounded-lg border border-zinc-700 px-3 py-2 text-sm font-semibold text-zinc-100 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {t("common.next")}
        </button>
      </div>

      <Sheet
        open={mergeSource !== null}
        onOpenChange={(open) => {
          if (!open) {
            setMergeSource(null);
            setMergeTarget(null);
          }
        }}
      >
        <SheetContent side="right" className="max-w-xl">
          <SheetHeader>
            <SheetTitle>Merge author</SheetTitle>
          </SheetHeader>
          {mergeSource ? (
            <div className="flex h-full flex-col p-5 text-zinc-100">
              <div>
                <h3 className="text-2xl font-semibold">{mergeSource.name}</h3>
                <p className="text-sm text-zinc-400">{mergeSource.sort_name}</p>
              </div>

              <div className="mt-5 flex flex-1 flex-col gap-4">
                <AuthorAutocomplete
                  excludeId={mergeSource.id}
                  placeholder="Search merge target"
                  onSelect={(author) => setMergeTarget(author)}
                />

                {mergeTarget ? (
                  <div className="rounded-xl border border-zinc-800 bg-zinc-900/70 p-4">
                    <p className="text-xs uppercase tracking-[0.2em] text-zinc-500">Selected target</p>
                    <p className="mt-1 text-lg font-semibold text-zinc-100">{mergeTarget.name}</p>
                    <p className="text-sm text-zinc-400">{mergeTarget.sort_name}</p>
                    <p className="text-sm text-zinc-400">{mergeTarget.book_count} books</p>
                  </div>
                ) : (
                  <p className="text-sm text-zinc-400">Search and choose the author to merge into.</p>
                )}

                <div className="mt-auto flex gap-3">
                  <button
                    type="button"
                    onClick={() => void confirmMerge()}
                    disabled={!mergeTarget || mergeMutation.isPending}
                    className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-50"
                  >
                    {mergeMutation.isPending ? "Merging..." : "Merge"}
                  </button>
                </div>
              </div>
            </div>
          ) : null}
        </SheetContent>
      </Sheet>
    </div>
  );
}
