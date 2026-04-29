import type { KeyboardEvent } from "react";
import type { BookSummary } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { CoverPlaceholder } from "./CoverPlaceholder";

type MediaCardProps = {
  book: BookSummary;
  progressPercentage?: number;
};

export function MediaCard({ book, progressPercentage = 0 }: MediaCardProps) {
  const safeProgress = Math.max(0, Math.min(100, progressPercentage));
  const href = `/books/${encodeURIComponent(book.id)}`;

  function openBookDetail() {
    if (typeof window === "undefined") {
      return;
    }

    window.location.assign(href);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLAnchorElement>) {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }

    event.preventDefault();
    openBookDetail();
  }

  return (
    <a
      href={href}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      className="group block outline-none transition focus-visible:ring-2 focus-visible:ring-teal-500"
      data-book-card="true"
    >
      <div className="relative">
        <div className="relative overflow-hidden rounded-xl shadow-md">
          {book.has_cover ? (
            <img
              src={apiClient.coverUrl(book.id)}
              alt={`${book.title} cover`}
              loading="lazy"
              className={`aspect-[2/3] w-full object-cover ${book.is_archived ? "grayscale" : ""}`}
            />
          ) : (
            <CoverPlaceholder title={book.title} className="rounded-xl shadow-md" />
          )}

          {safeProgress > 0 ? (
            <div className="absolute inset-x-0 bottom-0 h-1 bg-zinc-200">
              <div
                data-testid="progress-bar"
                className="h-full bg-teal-500"
                style={{ width: `${safeProgress}%` }}
              />
            </div>
          ) : null}
        </div>

        <p className="mt-2 line-clamp-2 text-xs font-semibold text-zinc-900">{book.title}</p>
      </div>
    </a>
  );
}
