import { afterEach, describe, expect, test } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MediaCard } from "./MediaCard";
import { makeBookSummary } from "../../test/fixtures";

describe("MediaCard", () => {
  afterEach(() => {
    cleanup();
  });

  test("renders cover image when book.has_cover is true", () => {
    render(<MediaCard book={makeBookSummary()} progressPercentage={67} />);

    expect(screen.getByRole("img", { name: /dune cover/i })).toBeTruthy();
  });

  test("renders CoverPlaceholder when book.has_cover is false", () => {
    render(<MediaCard book={makeBookSummary({ has_cover: false, title: "Hyperion" })} />);

    expect(screen.getByTestId("cover-placeholder")).toBeTruthy();
  });

  test("renders progress bar when progressPercentage > 0", () => {
    render(<MediaCard book={makeBookSummary()} progressPercentage={67} />);

    expect(screen.getByTestId("progress-bar")).toHaveStyle({ width: "67%" });
  });

  test("progress bar is absent when progressPercentage is 0", () => {
    render(<MediaCard book={makeBookSummary()} progressPercentage={0} />);

    expect(screen.queryByTestId("progress-bar")).toBeNull();
  });
});
