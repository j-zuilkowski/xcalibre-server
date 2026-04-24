import { useTranslation } from "react-i18next";
import type { BookSummary } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { CoverPlaceholder } from "./CoverPlaceholder";

type BookListRowProps = {
  book: BookSummary;
  formats?: string[];
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

export function BookListRow({ book, formats = [] }: BookListRowProps) {
  const { t } = useTranslation();
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
        <p className="truncate text-sm text-zinc-500">{authorLabel(book, t)}</p>
      </div>

      <p className="truncate text-sm text-zinc-600">
        {book.series ? `${book.series.name}${book.series_index ? ` · ${book.series_index}` : ""}` : t("book.no_series")}
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
            {t("book.unknown_format")}
          </span>
        )}
      </div>
    </article>
  );
}
