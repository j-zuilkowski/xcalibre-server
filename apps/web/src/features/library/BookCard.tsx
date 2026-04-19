import type { BookSummary } from "@calibre/shared";
import { apiClient } from "../../lib/api-client";
import { CoverPlaceholder } from "./CoverPlaceholder";

type BookCardProps = {
  book: BookSummary;
  readFormat?: string;
  progressPercentage?: number;
};

function authorLabel(book: BookSummary): string {
  if (book.authors.length === 0) {
    return "Unknown author";
  }
  return book.authors.map((author) => author.name).join(", ");
}

export function BookCard({ book, readFormat = "epub", progressPercentage = 0 }: BookCardProps) {
  const safeProgress = Math.max(0, Math.min(100, progressPercentage));
  const readHref = `/books/${encodeURIComponent(book.id)}/read/${encodeURIComponent(readFormat)}`;
  const downloadHref = apiClient.downloadUrl(book.id, readFormat);

  return (
    <article className="group">
      <div className="relative">
        <a href={`/books/${encodeURIComponent(book.id)}`} className="block">
          {book.has_cover ? (
            <img
              src={apiClient.coverUrl(book.id)}
              alt={`${book.title} cover`}
              loading="lazy"
              className="aspect-[2/3] w-full rounded-lg object-cover"
            />
          ) : (
            <CoverPlaceholder title={book.title} />
          )}
        </a>

        <div className="pointer-events-none absolute inset-0 flex items-center justify-center rounded-lg bg-black/50 opacity-0 transition-opacity duration-200 group-hover:opacity-100">
          <div className="pointer-events-auto flex gap-2">
            <a
              href={readHref}
              className="rounded-md bg-zinc-100 px-3 py-2 text-sm font-semibold text-zinc-900"
            >
              Read
            </a>
            <a
              href={downloadHref}
              className="rounded-md bg-zinc-900 px-3 py-2 text-sm font-semibold text-zinc-100"
              download
            >
              Download
            </a>
          </div>
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

      <a
        href={`/books/${encodeURIComponent(book.id)}`}
        className="mt-2 block truncate text-sm font-semibold text-zinc-900"
      >
        {book.title}
      </a>
      <p className="truncate text-sm text-zinc-500">{authorLabel(book)}</p>
    </article>
  );
}
