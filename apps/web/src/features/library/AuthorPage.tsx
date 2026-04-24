import { type CSSProperties, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { AdminAuthor, AuthorDetail, AuthorProfilePatch } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";
import { BookCard } from "./BookCard";
import { CoverPlaceholder } from "./CoverPlaceholder";
import { AuthorAutocomplete } from "../admin/AuthorAutocomplete";

type AuthorPageProps = {
  authorId?: string;
};

const PAGE_SIZE = 24;

function resolveAuthorId(authorId?: string): string | null {
  if (authorId && authorId.trim().length > 0) {
    return authorId;
  }

  const segments = window.location.pathname.split("/").filter(Boolean);
  const authorsIndex = segments.findIndex((segment) => segment === "authors");
  if (authorsIndex < 0 || authorsIndex + 1 >= segments.length) {
    return null;
  }

  return decodeURIComponent(segments[authorsIndex + 1]);
}

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

function formatAuthorYears(profile: AuthorDetail["profile"] | undefined): string {
  if (!profile) {
    return "";
  }

  const parts = [profile.born, profile.died].filter(Boolean);
  return parts.join(" - ");
}

function authorBooksHeading(count: number): string {
  return `${count} book${count === 1 ? "" : "s"}`;
}

export function AuthorPage({ authorId }: AuthorPageProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const user = useAuthStore((state) => state.user);
  const resolvedAuthorId = useMemo(() => resolveAuthorId(authorId), [authorId]);
  const [page, setPage] = useState<number>(() => parsePage(window.location.search ? new URLSearchParams(window.location.search).get("page") : null));
  const [bioExpanded, setBioExpanded] = useState(false);
  const [drawerMode, setDrawerMode] = useState<"edit" | "merge" | null>(null);
  const [mergeTarget, setMergeTarget] = useState<AdminAuthor | null>(null);
  const [editDraft, setEditDraft] = useState<AuthorProfilePatch>({
    bio: "",
    born: "",
    died: "",
    website_url: "",
    openlibrary_id: "",
  });

  const canEdit = Boolean(user?.role.can_edit || user?.role.name?.toLowerCase() === "admin");
  const isAdmin = user?.role.name?.toLowerCase() === "admin";

  useEffect(() => {
    const onPopState = () => {
      setPage(parsePage(new URLSearchParams(window.location.search).get("page")));
    };

    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  const authorQuery = useQuery({
    queryKey: ["author-detail", resolvedAuthorId, page],
    queryFn: () => apiClient.getAuthor(resolvedAuthorId as string, { page, page_size: PAGE_SIZE }),
    enabled: Boolean(resolvedAuthorId),
  });

  const author = authorQuery.data;
  const books = author?.books ?? [];
  const totalPages = Math.max(1, Math.ceil((author?.book_count ?? 0) / (author?.page_size ?? PAGE_SIZE)));

  useEffect(() => {
    if (!author?.profile) {
      setEditDraft({
        bio: "",
        born: "",
        died: "",
        website_url: "",
        openlibrary_id: "",
      });
      return;
    }

    setEditDraft({
      bio: author.profile.bio ?? "",
      born: author.profile.born ?? "",
      died: author.profile.died ?? "",
      website_url: author.profile.website_url ?? "",
      openlibrary_id: author.profile.openlibrary_id ?? "",
    });
  }, [author?.profile]);

  useEffect(() => {
    setBioExpanded(false);
  }, [author?.id]);

  const saveMutation = useMutation({
    mutationFn: (patch: AuthorProfilePatch) => apiClient.patchAuthor(resolvedAuthorId as string, patch),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["author-detail", resolvedAuthorId] });
      setDrawerMode(null);
    },
  });

  const mergeMutation = useMutation({
    mutationFn: (targetId: string) => apiClient.mergeAuthor(resolvedAuthorId as string, targetId),
    onSuccess: async (_result, targetId) => {
      await queryClient.invalidateQueries();
      setDrawerMode(null);
      setMergeTarget(null);
      window.location.assign(`/authors/${encodeURIComponent(targetId)}`);
    },
  });

  function updatePage(nextPage: number) {
    const normalized = Math.max(1, nextPage);
    setPage(normalized);
    const params = new URLSearchParams(window.location.search);
    if (normalized > 1) {
      params.set("page", String(normalized));
    } else {
      params.delete("page");
    }
    const nextSearch = params.toString();
    const nextUrl = nextSearch ? `${window.location.pathname}?${nextSearch}` : window.location.pathname;
    window.history.replaceState({}, "", nextUrl);
  }

  function beginEdit() {
    setDrawerMode("edit");
    setMergeTarget(null);
  }

  function beginMerge() {
    setDrawerMode("merge");
    setMergeTarget(null);
  }

  function closeDrawer() {
    setDrawerMode(null);
    setMergeTarget(null);
  }

  async function submitEdit() {
    await saveMutation.mutateAsync({
      bio: editDraft.bio?.trim() ? editDraft.bio.trim() : null,
      born: editDraft.born?.trim() ? editDraft.born.trim() : null,
      died: editDraft.died?.trim() ? editDraft.died.trim() : null,
      website_url: editDraft.website_url?.trim() ? editDraft.website_url.trim() : null,
      openlibrary_id: editDraft.openlibrary_id?.trim() ? editDraft.openlibrary_id.trim() : null,
    });
  }

  async function submitMerge() {
    if (!mergeTarget || mergeTarget.id === resolvedAuthorId) {
      return;
    }
    await mergeMutation.mutateAsync(mergeTarget.id);
  }

  if (!resolvedAuthorId) {
    return (
      <div className="mx-auto max-w-6xl rounded-2xl border border-zinc-200 bg-white p-6 text-zinc-600">
        {t("common.not_applicable")}
      </div>
    );
  }

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,3fr)]">
        <aside className="rounded-2xl border border-zinc-200 bg-white p-5 shadow-sm">
          {author?.profile?.photo_url ? (
            <img
              src={author.profile.photo_url}
              alt={`${author.name} portrait`}
              className="aspect-[2/3] w-full rounded-xl object-cover"
            />
          ) : (
            <CoverPlaceholder title={author?.name ?? "Author"} />
          )}

          <div className="mt-4 flex items-start justify-between gap-3">
            <div>
              <p className="text-xs uppercase tracking-[0.2em] text-zinc-500">{t("library.author")}</p>
              <h1 className="mt-1 text-3xl font-semibold text-zinc-900">{author?.name ?? t("common.loading")}</h1>
              <p className="text-sm text-zinc-500">{author?.sort_name}</p>
            </div>
          </div>

          <div className="mt-4 space-y-4 text-sm text-zinc-700">
            {author?.profile?.bio ? (
              <section>
                <div
                  className={`text-zinc-700 ${bioExpanded ? "" : "overflow-hidden"}`}
                  style={
                    bioExpanded
                      ? undefined
                      : ({
                          display: "-webkit-box",
                          WebkitBoxOrient: "vertical",
                          WebkitLineClamp: 4,
                        } as CSSProperties)
                  }
                >
                  {author.profile.bio}
                </div>
                {author.profile.bio.length > 160 ? (
                  <button
                    type="button"
                    onClick={() => setBioExpanded((value) => !value)}
                    className="mt-2 text-xs font-semibold uppercase tracking-[0.2em] text-teal-700"
                  >
                    {bioExpanded ? "Collapse" : "Expand"}
                  </button>
                ) : null}
              </section>
            ) : null}

            {formatAuthorYears(author?.profile) ? (
              <p className="text-zinc-600">{formatAuthorYears(author.profile)}</p>
            ) : null}

            {author?.profile?.website_url ? (
              <p>
                <a
                  href={author.profile.website_url}
                  target="_blank"
                  rel="noreferrer"
                  className="text-teal-700 hover:underline"
                >
                  {author.profile.website_url}
                </a>
              </p>
            ) : null}

            {author?.profile?.openlibrary_id ? (
              <p>
                <a
                  href={`https://openlibrary.org/authors/${encodeURIComponent(author.profile.openlibrary_id)}`}
                  target="_blank"
                  rel="noreferrer"
                  className="text-teal-700 hover:underline"
                >
                  OpenLibrary
                </a>
              </p>
            ) : null}
          </div>

          <div className="mt-5 flex flex-wrap gap-2">
            {canEdit ? (
              <button
                type="button"
                onClick={beginEdit}
                className="rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm font-semibold text-zinc-900"
              >
                Edit profile
              </button>
            ) : null}
            {isAdmin ? (
              <button
                type="button"
                onClick={beginMerge}
                className="rounded-lg bg-zinc-900 px-3 py-2 text-sm font-semibold text-white"
              >
                Merge author
              </button>
            ) : null}
          </div>
        </aside>

        <main className="min-w-0 rounded-2xl border border-zinc-200 bg-white p-5 shadow-sm">
          <div className="flex items-end justify-between gap-3">
            <div>
              <p className="text-xs uppercase tracking-[0.2em] text-zinc-500">{authorBooksHeading(author?.book_count ?? 0)}</p>
              <h2 className="mt-1 text-2xl font-semibold text-zinc-900">{t("library.books")}</h2>
            </div>
          </div>

          <section className="mt-5">
            {authorQuery.isLoading ? (
              <div className="text-sm text-zinc-500">{t("common.loading")}</div>
            ) : books.length > 0 ? (
              <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
                {books.map((book) => (
                  <BookCard
                    key={book.id}
                    book={book}
                    progressPercentage={book.progress_percentage ?? 0}
                  />
                ))}
              </div>
            ) : (
              <div className="rounded-xl border border-dashed border-zinc-200 p-8 text-center text-sm text-zinc-500">
                {t("common.none")}
              </div>
            )}
          </section>

          <div className="mt-6 flex items-center justify-between">
            <button
              type="button"
              onClick={() => updatePage(page - 1)}
              disabled={page <= 1}
              className="rounded-lg border border-zinc-300 px-3 py-2 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-50"
            >
              {t("common.previous")}
            </button>
            <p className="text-sm text-zinc-500">
              {t("common.page_of", { page, total: totalPages })}
            </p>
            <button
              type="button"
              onClick={() => updatePage(page + 1)}
              disabled={page >= totalPages}
              className="rounded-lg border border-zinc-300 px-3 py-2 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-50"
            >
              {t("common.next")}
            </button>
          </div>
        </main>
      </div>

      {drawerMode ? (
        <div className="fixed inset-0 z-50 flex">
          <button
            type="button"
            aria-label="Close drawer"
            onClick={closeDrawer}
            className="flex-1 bg-zinc-950/50"
          />
          <aside className="flex w-full max-w-xl flex-col border-l border-zinc-200 bg-white p-5 shadow-2xl">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-xs uppercase tracking-[0.2em] text-zinc-500">
                  {drawerMode === "edit" ? "Edit profile" : "Merge author"}
                </p>
                <h3 className="mt-1 text-2xl font-semibold text-zinc-900">{author?.name}</h3>
              </div>
              <button
                type="button"
                onClick={closeDrawer}
                className="rounded-full border border-zinc-300 px-3 py-1 text-sm text-zinc-600"
              >
                Close
              </button>
            </div>

            {drawerMode === "edit" ? (
              <form
                className="mt-5 flex flex-1 flex-col gap-4"
                onSubmit={(event) => {
                  event.preventDefault();
                  void submitEdit();
                }}
              >
                <label className="space-y-2 text-sm">
                  <span className="font-semibold text-zinc-700">Bio</span>
                  <textarea
                    value={editDraft.bio ?? ""}
                    onChange={(event) => setEditDraft((previous) => ({ ...previous, bio: event.target.value }))}
                    rows={6}
                    className="w-full rounded-xl border border-zinc-300 px-3 py-2 text-sm text-zinc-900"
                  />
                </label>
                <div className="grid gap-4 md:grid-cols-2">
                  <label className="space-y-2 text-sm">
                    <span className="font-semibold text-zinc-700">Born</span>
                    <input
                      value={editDraft.born ?? ""}
                      onChange={(event) => setEditDraft((previous) => ({ ...previous, born: event.target.value }))}
                      className="w-full rounded-xl border border-zinc-300 px-3 py-2 text-sm text-zinc-900"
                    />
                  </label>
                  <label className="space-y-2 text-sm">
                    <span className="font-semibold text-zinc-700">Died</span>
                    <input
                      value={editDraft.died ?? ""}
                      onChange={(event) => setEditDraft((previous) => ({ ...previous, died: event.target.value }))}
                      className="w-full rounded-xl border border-zinc-300 px-3 py-2 text-sm text-zinc-900"
                    />
                  </label>
                </div>
                <label className="space-y-2 text-sm">
                  <span className="font-semibold text-zinc-700">Website</span>
                  <input
                    value={editDraft.website_url ?? ""}
                    onChange={(event) =>
                      setEditDraft((previous) => ({ ...previous, website_url: event.target.value }))
                    }
                    className="w-full rounded-xl border border-zinc-300 px-3 py-2 text-sm text-zinc-900"
                  />
                </label>
                <label className="space-y-2 text-sm">
                  <span className="font-semibold text-zinc-700">OpenLibrary ID</span>
                  <input
                    value={editDraft.openlibrary_id ?? ""}
                    onChange={(event) =>
                      setEditDraft((previous) => ({ ...previous, openlibrary_id: event.target.value }))
                    }
                    className="w-full rounded-xl border border-zinc-300 px-3 py-2 text-sm text-zinc-900"
                  />
                </label>
                <div className="mt-auto flex gap-3">
                  <button
                    type="submit"
                    disabled={saveMutation.isPending}
                    className="rounded-lg bg-zinc-900 px-4 py-2 text-sm font-semibold text-white disabled:opacity-50"
                  >
                    {saveMutation.isPending ? t("common.saving") : t("common.save")}
                  </button>
                </div>
              </form>
            ) : (
              <div className="mt-5 flex flex-1 flex-col gap-4">
                <AuthorAutocomplete
                  excludeId={resolvedAuthorId ?? undefined}
                  placeholder="Search target author"
                  onSelect={(author) => setMergeTarget(author)}
                />
                {mergeTarget ? (
                  <div className="rounded-xl border border-zinc-200 p-4">
                    <p className="text-xs uppercase tracking-[0.2em] text-zinc-500">Selected target</p>
                    <p className="mt-1 text-lg font-semibold text-zinc-900">{mergeTarget.name}</p>
                    <p className="text-sm text-zinc-500">{mergeTarget.sort_name}</p>
                    <p className="text-sm text-zinc-500">{mergeTarget.book_count} books</p>
                  </div>
                ) : (
                  <p className="text-sm text-zinc-500">Search and select the author to merge into.</p>
                )}
                <div className="mt-auto flex gap-3">
                  <button
                    type="button"
                    onClick={() => void submitMerge()}
                    disabled={!mergeTarget || mergeMutation.isPending}
                    className="rounded-lg bg-zinc-900 px-4 py-2 text-sm font-semibold text-white disabled:opacity-50"
                  >
                    {mergeMutation.isPending ? "Merging..." : "Merge"}
                  </button>
                </div>
              </div>
            )}
          </aside>
        </div>
      ) : null}
    </div>
  );
}
