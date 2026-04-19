import { afterEach, describe, expect, test } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { BookSummary } from "@calibre/shared";
import { BookCard } from "../features/library/BookCard";

const baseBook: BookSummary = {
  id: "book-1",
  title: "Dune",
  sort_title: "Dune",
  authors: [{ id: "author-1", name: "Frank Herbert", sort_name: "Herbert, Frank" }],
  series: null,
  series_index: null,
  cover_url: null,
  has_cover: true,
  language: "en",
  rating: 5,
  last_modified: "2026-04-19T00:00:00Z",
};

describe("BookCard", () => {
  afterEach(() => {
    cleanup();
  });

  test("test_shows_cover_image_when_has_cover_true", () => {
    render(<BookCard book={baseBook} />);

    expect(screen.getByRole("img", { name: "Dune cover" })).toBeTruthy();
  });

  test("test_shows_placeholder_when_has_cover_false", () => {
    render(<BookCard book={{ ...baseBook, has_cover: false, title: "Hyperion" }} />);

    expect(screen.getByTestId("cover-placeholder")).toBeTruthy();
  });

  test("test_progress_bar_visible_when_progress_nonzero", () => {
    render(<BookCard book={baseBook} progressPercentage={42} />);

    expect(screen.getByTestId("progress-bar")).toBeTruthy();
  });

  test("test_progress_bar_hidden_when_no_progress", () => {
    render(<BookCard book={baseBook} progressPercentage={0} />);

    expect(screen.queryByTestId("progress-bar")).toBeNull();
  });
});
