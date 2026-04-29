import { afterEach, describe, expect, test } from "vitest";
import { cleanup, screen, within } from "@testing-library/react";
import { renderWithProviders } from "../test/render";

describe("AppShell", () => {
  afterEach(() => {
    cleanup();
  });

  test("sidebar contains a Home nav link", async () => {
    renderWithProviders(<></>, {
      initialPath: "/library",
      authenticated: true,
    });

    const nav = await screen.findByRole("navigation", { name: "Main navigation" });
    const scoped = within(nav);

    expect(scoped.getByRole("link", { name: /home/i })).toHaveAttribute("href", "/home");
  });

  test("sidebar contains Browse category links", async () => {
    renderWithProviders(<></>, {
      initialPath: "/library",
      authenticated: true,
    });

    const nav = await screen.findByRole("navigation", { name: "Main navigation" });
    const scoped = within(nav);

    expect(scoped.getByRole("link", { name: /books/i })).toHaveAttribute("href", "/browse/books");
    expect(scoped.getByRole("link", { name: /reference/i })).toHaveAttribute("href", "/browse/reference");
    expect(scoped.getByRole("link", { name: /periodicals/i })).toHaveAttribute(
      "href",
      "/browse/periodicals",
    );
    expect(scoped.getByRole("link", { name: /magazines/i })).toHaveAttribute("href", "/browse/magazines");
  });
});
