import { afterEach, describe, expect, test } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { CoverPlaceholder } from "../features/library/CoverPlaceholder";

describe("CoverPlaceholder", () => {
  afterEach(() => {
    cleanup();
  });

  test("test_renders_first_letter_of_title", () => {
    render(<CoverPlaceholder title="the hobbit" />);

    expect(screen.getByText("T")).toBeTruthy();
  });

  test("test_same_title_always_same_color", () => {
    const firstRender = render(<CoverPlaceholder title="The Name of the Wind" />);
    const first = firstRender
      .getByTestId("cover-placeholder")
      .getAttribute("data-color-index");
    firstRender.unmount();

    const secondRender = render(<CoverPlaceholder title="The Name of the Wind" />);
    const second = secondRender
      .getByTestId("cover-placeholder")
      .getAttribute("data-color-index");

    expect(first).toBe(second);
  });
});
