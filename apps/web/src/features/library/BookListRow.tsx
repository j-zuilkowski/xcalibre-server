import type { BookSummary } from "@calibre/shared";
import { apiClient } from "../../lib/api-client";
import { CoverPlaceholder } from "./CoverPlaceholder";

type BookListRowProps = {
  book: BookSummary;
  formats?: string[];
};

function authorLabel(book: BookSummary): string {
  if (book.authors.length === 0) {
    return "Unknown author";
  }
  return book.authors.map((author) => author.name).join(", ");
}

export function BookListRow({ book, formats = [] }: BookListRowProps) {
  return (
    <article className="grid grid-cols-[56px_1fr_1fr_1fr] items-center gap-3 rounded-lg border border-zinc-200 bg-white p-2">
      <div className="w-12">
        {book.has_cover ? (
          <img
            src={apiClient.coverUrl(book.id)}
            alt={`${book.title} thumbnail`}
            loading="lazy"
            className="aspect-[2/3] w-full rounded object-cover"
          />
        ) : (
          <CoverPlaceholder title={book.title} className="rounded" />
        )}
      </div>

      <div className="min-w-0">
        <a href={`/books/${encodeURIComponent(book.id)}`} className="block truncate font-semibold text-zinc-900">
          {book.title}
        </a>
        <p className="truncate text-sm text-zinc-500">{authorLabel(book)}</p>
      </div>

      <p className="truncate text-sm text-zinc-600">
        {book.series ? `${book.series.name}${book.series_index ? ` · ${book.series_index}` : ""}` : "No series"}
      </p>

      <div className="flex flex-wrap gap-1">
        {formats.length > 0 ? (
          formats.map((format) => (
            <span
              key={`${book.id}-${format}`}
              className="rounded-full border border-zinc-300 px-2 py-0.5 text-xs text-zinc-600"
            >
              {format}
            </span>
          ))
        ) : (
          <span className="rounded-full border border-zinc-300 px-2 py-0.5 text-xs text-zinc-500">
            Unknown format
          </span>
        )}
      </div>
    </article>
  );
}
