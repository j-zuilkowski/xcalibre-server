import { useId, useMemo, useRef, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { ApplyMetadataBody, Book, MetadataCandidate } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { Dialog } from "../../components/ui/Dialog";

type IdentifyModalProps = {
  book: Book;
  onClose: () => void;
  onApplied: () => void;
};

function Spinner({ className = "" }: { className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={`inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent ${className}`}
    />
  );
}

function buildDefaultQuery(book: Book): string {
  const firstAuthor = book.authors[0]?.name?.trim() ?? "";
  return [book.title.trim(), firstAuthor].filter(Boolean).join(" ");
}

function sourceLabel(candidate: MetadataCandidate, t: (key: string) => string): string {
  if (candidate.source === "google_books") {
    return t("identify.source_google");
  }
  return t("identify.source_open_library");
}

export function IdentifyModal({ book, onClose, onApplied }: IdentifyModalProps) {
  const { t } = useTranslation();
  const titleId = useId();
  const queryInputRef = useRef<HTMLInputElement | null>(null);
  const [query, setQuery] = useState(() => buildDefaultQuery(book));
  const [candidates, setCandidates] = useState<MetadataCandidate[]>([]);
  const [hasSearched, setHasSearched] = useState(false);
  const [applyingId, setApplyingId] = useState<string | null>(null);

  const searchMutation = useMutation({
    mutationFn: async (value: string) => apiClient.searchBookMetadata(book.id, value),
    onSuccess: (result) => {
      setHasSearched(true);
      setCandidates(result);
    },
    onError: () => {
      setHasSearched(true);
      setCandidates([]);
    },
  });

  const applyMutation = useMutation({
    mutationFn: async (candidate: MetadataCandidate) => {
      const body: ApplyMetadataBody = {
        source: candidate.source,
        external_id: candidate.external_id,
        title: candidate.title,
        authors: candidate.authors,
        description: candidate.description ?? undefined,
        publisher: candidate.publisher ?? undefined,
        published_date: candidate.published_date ?? undefined,
        isbn_13: candidate.isbn_13 ?? undefined,
        isbn_10: candidate.isbn_10 ?? undefined,
        cover_url: candidate.cover_url ?? undefined,
      };
      return apiClient.applyBookMetadata(book.id, body);
    },
    onSuccess: () => {
      onApplied();
      onClose();
      setApplyingId(null);
    },
    onError: () => {
      setApplyingId(null);
    },
  });

  const hasResults = candidates.length > 0;
  const trimmedQuery = useMemo(() => query.trim(), [query]);

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()} titleId={titleId} initialFocusRef={queryInputRef}>
      <div className="mx-auto w-[min(92vw,52rem)] rounded-2xl border border-zinc-200 bg-white shadow-2xl">
        <div className="flex items-start justify-between gap-3 border-b border-zinc-200 px-5 py-4">
          <div>
            <p className="text-xs font-semibold uppercase tracking-wide text-teal-700">{t("identify.title")}</p>
            <h3 id={titleId} className="mt-1 text-xl font-semibold text-zinc-900">
              {t("identify.title")}
            </h3>
            <p className="mt-1 text-sm text-zinc-600">
              {book.title}
              {book.authors[0]?.name ? ` · ${book.authors[0].name}` : ""}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-zinc-300 px-3 py-2 text-sm text-zinc-700"
          >
            {t("common.close")}
          </button>
        </div>

        <form
          className="space-y-4 px-5 py-4"
          onSubmit={(event) => {
            event.preventDefault();
            setHasSearched(true);
            void searchMutation.mutateAsync(trimmedQuery);
          }}
        >
          <label className="block text-sm font-medium text-zinc-700">
            {t("identify.search_label")}
            <input
              ref={queryInputRef}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="mt-1 w-full rounded-lg border border-zinc-300 px-3 py-2 text-sm"
            />
          </label>

          <div className="flex items-center gap-3">
            <button
              type="submit"
              disabled={searchMutation.isPending}
              className="inline-flex items-center gap-2 rounded-lg bg-teal-600 px-4 py-2 text-sm font-semibold text-white disabled:opacity-70"
            >
              {searchMutation.isPending ? <Spinner className="border-zinc-200 text-white" /> : null}
              {t("identify.search_button")}
            </button>
            {searchMutation.isPending ? <span className="text-sm text-zinc-500">{t("identify.searching")}</span> : null}
          </div>

        </form>

        <div className="max-h-[60vh] overflow-auto border-t border-zinc-200 px-5 py-4">
          {searchMutation.isPending ? null : hasResults ? (
            <ul className="space-y-3">
              {candidates.map((candidate) => {
                const isApplying = applyingId === candidate.external_id;
                return (
                  <li
                    key={`${candidate.source}:${candidate.external_id}`}
                    className="flex items-start gap-3 rounded-xl border border-zinc-200 bg-zinc-50 p-3"
                  >
                    <div className="h-[60px] w-[40px] shrink-0 overflow-hidden rounded-md border border-zinc-200 bg-zinc-200">
                      {candidate.thumbnail_url ? (
                        <img
                          src={candidate.thumbnail_url}
                          alt=""
                          className="h-full w-full object-cover"
                        />
                      ) : null}
                    </div>

                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <p className="truncate font-semibold text-zinc-900">{candidate.title}</p>
                        <span className="rounded-full border border-teal-200 bg-teal-50 px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide text-teal-700">
                          {sourceLabel(candidate, t)}
                        </span>
                      </div>
                      <p className="mt-1 text-sm text-zinc-600">
                        {candidate.authors.length > 0
                          ? candidate.authors.join(", ")
                          : t("common.unknown_author")}
                        {candidate.published_date ? ` · ${candidate.published_date}` : ""}
                      </p>
                      {candidate.description ? (
                        <p className="mt-2 line-clamp-3 text-sm text-zinc-600">{candidate.description}</p>
                      ) : null}
                    </div>

                    <button
                      type="button"
                      disabled={isApplying || applyMutation.isPending}
                      onClick={() => {
                        setApplyingId(candidate.external_id);
                        void applyMutation.mutateAsync(candidate);
                      }}
                      className="inline-flex items-center gap-2 rounded-lg border border-teal-600 px-3 py-2 text-sm font-semibold text-teal-700 disabled:opacity-70"
                    >
                      {isApplying ? <Spinner className="border-teal-600 text-teal-700" /> : null}
                      {isApplying ? t("identify.applying") : t("identify.apply")}
                    </button>
                  </li>
                );
              })}
            </ul>
          ) : hasSearched ? (
            <p className="text-sm text-zinc-500">{t("identify.no_results")}</p>
          ) : (
            <p className="text-sm text-zinc-500">{t("identify.search_label")}</p>
          )}
        </div>
      </div>
    </Dialog>
  );
}
