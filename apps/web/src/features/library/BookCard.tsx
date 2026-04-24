import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { BookSummary } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { CoverPlaceholder } from "./CoverPlaceholder";

type BookCardProps = {
  book: BookSummary;
  readFormat?: string;
  progressPercentage?: number;
  score?: number;
};

function authorLabel(book: BookSummary, t: (key: string) => string) {
  if (book.authors.length === 0) {
    return t("common.unknown_author");
  }
  return book.authors.map((author, index) => (
    <span key={author.id}>
      <a href={`/authors/${encodeURIComponent(author.id)}`} className="text-teal-700 hover:underline">
        {author.name}
      </a>
      {index < book.authors.length - 1 ? ", " : null}
    </span>
  ));
}

function ReadIcon() {
  return (
    <svg viewBox="0 0 20 20" fill="none" aria-hidden="true" className="h-4 w-4">
      <path d="M4 10.5l4 4L16 6.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function ArchiveIcon() {
  return (
    <svg viewBox="0 0 20 20" fill="none" aria-hidden="true" className="h-4 w-4">
      <path d="M3 5.5h14v2H3z" fill="currentColor" />
      <path d="M4 7.5h12v8H4z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
      <path d="M7 10.5h6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

export function BookCard({
  book,
  readFormat = "epub",
  progressPercentage = 0,
  score,
}: BookCardProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const safeProgress = Math.max(0, Math.min(100, progressPercentage));
  const scorePercentage =
    typeof score === "number" && score > 0 ? Math.round(Math.max(0, Math.min(1, score)) * 100) : null;
  const readHref = `/books/${encodeURIComponent(book.id)}/read/${encodeURIComponent(readFormat)}`;
  const downloadHref = apiClient.downloadUrl(book.id, readFormat);

  const readMutation = useMutation({
    mutationFn: (nextIsRead: boolean) => apiClient.setBookReadState(book.id, nextIsRead),
    onSuccess: () => {
      void queryClient.invalidateQueries();
    },
  });

  const archiveMutation = useMutation({
    mutationFn: (nextIsArchived: boolean) => apiClient.setBookArchivedState(book.id, nextIsArchived),
    onSuccess: () => {
      void queryClient.invalidateQueries();
    },
  });

  return (
    <article className={`group ${book.is_archived ? "opacity-75" : ""}`}>
      <div className="relative">
        <a href={`/books/${encodeURIComponent(book.id)}`} className="block">
          {book.has_cover ? (
            <img
              src={apiClient.coverUrl(book.id)}
              alt={`${book.title} cover`}
              loading="lazy"
              className={`aspect-[2/3] w-full rounded-lg object-cover ${book.is_archived ? "grayscale" : ""}`}
            />
          ) : (
            <CoverPlaceholder title={book.title} />
          )}
        </a>

        <div className="pointer-events-none absolute inset-0 flex items-center justify-center rounded-lg bg-black/50 opacity-0 transition-opacity duration-200 group-hover:opacity-100">
          <div className="pointer-events-auto flex gap-2">
            <a href={readHref} className="rounded-md bg-zinc-100 px-3 py-2 text-sm font-semibold text-zinc-900">
              {t("common.read")}
            </a>
            <a href={downloadHref} className="rounded-md bg-zinc-900 px-3 py-2 text-sm font-semibold text-zinc-100" download>
              {t("common.download")}
            </a>
          </div>
        </div>

        <div className="absolute right-2 top-2 z-10 flex gap-2">
          <button
            type="button"
            aria-label={book.is_read ? t("book.mark_unread") : t("book.mark_as_read")}
            aria-pressed={book.is_read}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              void readMutation.mutateAsync(!book.is_read);
            }}
            className={`inline-flex h-8 w-8 items-center justify-center rounded-full border shadow-sm transition ${
              book.is_read
                ? "border-teal-600 bg-teal-600 text-white"
                : "border-white/70 bg-white/90 text-zinc-700 hover:border-zinc-300"
            }`}
          >
            <ReadIcon />
          </button>
          <button
            type="button"
            aria-label={book.is_archived ? t("book.unarchive") : t("book.archive")}
            aria-pressed={book.is_archived}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              void archiveMutation.mutateAsync(!book.is_archived);
            }}
            className={`inline-flex h-8 w-8 items-center justify-center rounded-full border shadow-sm transition ${
              book.is_archived
                ? "border-amber-600 bg-amber-600 text-white"
                : "border-white/70 bg-white/90 text-zinc-700 hover:border-zinc-300"
            }`}
          >
            <ArchiveIcon />
          </button>
        </div>

        {safeProgress > 0 ? (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 h-[3px] bg-zinc-700 opacity-0 transition-opacity duration-200 group-hover:opacity-100">
            <div
              data-testid="progress-bar"
              className="h-full bg-teal-600"
              style={{ width: `${safeProgress}%` }}
            />
          </div>
        ) : null}
      </div>

      <div className="mt-2 flex items-center gap-2">
        <a
          href={`/books/${encodeURIComponent(book.id)}`}
          className="min-w-0 flex-1 truncate text-sm font-semibold text-zinc-900"
        >
          {book.title}
        </a>
        {scorePercentage !== null ? (
          <span className="inline-flex shrink-0 items-center rounded-full border border-teal-200 bg-teal-50 px-2 py-0.5 text-[10px] font-semibold text-teal-700">
            {t("search.match", { score: scorePercentage })}
          </span>
        ) : null}
      </div>
      <p className="truncate text-sm text-zinc-500">{authorLabel(book, t)}</p>
    </article>
  );
}
