import { describe, expect, test } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { makeAdminUser } from "../../test/fixtures";
import { screen, waitFor, within } from "@testing-library/react";

function renderBookDetailPage() {
  return renderWithProviders(<></>, {
    initialPath: "/books/1",
    authenticated: true,
  });
}

describe("BookDetailPage", () => {
  test("renders the book title, authors, cover, and rating", async () => {
    const { container } = renderBookDetailPage();

    expect(await screen.findByRole("heading", { name: "Dune" })).toBeTruthy();
    expect(screen.getByText(/frank herbert/i)).toBeTruthy();
    expect(screen.getByRole("img", { name: /dune cover/i })).toBeTruthy();
    expect(screen.getByLabelText("rating-stars")).toHaveTextContent("★★★★");
    expect(screen.getByText(/dune · book 1/i)).toBeTruthy();
    expect(container).toBeTruthy();
  });

  test("shows the description and formats inside the collapsible sections", async () => {
    const user = userEvent.setup();
    renderBookDetailPage();

    await user.click(await screen.findByRole("button", { name: /description/i }));
    expect(await screen.findByText(/desert planet adventure/i)).toBeTruthy();

    const formatsTrigger = screen.getByRole("button", { name: /formats/i });
    await user.click(formatsTrigger);

    const formatsSection = formatsTrigger.parentElement;
    expect(formatsSection).not.toBeNull();
    const formatsPanel = within(formatsSection as HTMLElement);
    expect(formatsPanel.getByText("EPUB")).toBeTruthy();
    expect(formatsPanel.getByText("PDF")).toBeTruthy();
    expect(formatsPanel.getAllByRole("link")).toHaveLength(2);
  });

  test("the read link points to the reader", async () => {
    renderBookDetailPage();

    const readLink = await screen.findByRole("link", { name: /read/i });
    expect(readLink).toHaveAttribute("href", "/books/1/read/epub");
  });

  test("clicking a tag pushes a library filter into the url", async () => {
    const user = userEvent.setup();
    renderBookDetailPage();

    await user.click(await screen.findByRole("button", { name: /sci-fi/i }));

    await waitFor(() => {
      expect(window.location.pathname).toBe("/library");
      expect(window.location.search).toContain("tag=sci-fi");
    });
  });

  test("Identify button is visible for admin users and opens the modal", async () => {
    const user = userEvent.setup();
    renderWithProviders(<></>, {
      initialPath: "/books/1",
      authenticated: true,
      user: makeAdminUser(),
    });

    const identifyButton = await screen.findByRole("button", { name: /identify/i });
    await user.click(identifyButton);

    expect(await screen.findByRole("heading", { name: /identify book/i })).toBeTruthy();
  });
});
