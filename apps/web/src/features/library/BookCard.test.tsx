import { describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BookCard } from "./BookCard";
import { makeBookSummary } from "../../test/fixtures";
import { server } from "../../test/setup";
import { makeTestQueryClient } from "../../test/query-client";

function renderCard(book = makeBookSummary()) {
  const queryClient = makeTestQueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Infinity },
      mutations: { retry: 0 },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <BookCard book={book} progressPercentage={67} score={0.85} />
    </QueryClientProvider>,
  );
}

describe("BookCard", () => {
  test("renders title and author links", () => {
    renderCard();

    expect(screen.getByText("Dune")).toBeTruthy();
    expect(screen.getByRole("link", { name: /frank herbert/i })).toHaveAttribute("href", "/authors/a1");
  });

  test("renders the cover image when available", () => {
    renderCard();

    expect(screen.getByRole("img", { name: /dune cover/i })).toBeTruthy();
  });

  test("renders the cover placeholder when there is no cover", () => {
    renderCard(makeBookSummary({ has_cover: false, title: "Hyperion" }));

    expect(screen.getByTestId("cover-placeholder")).toBeTruthy();
    // placeholder is a div with role="img"; assert no actual <img> element is rendered
    expect(document.querySelector("img")).toBeNull();
  });

  test("renders progress and score badges", () => {
    renderCard();

    expect(screen.getByTestId("progress-bar")).toHaveStyle({ width: "67%" });
    expect(screen.getByText(/match 85%/i)).toBeTruthy();
  });

  test("links to read and download urls", () => {
    renderCard();

    expect(screen.getByRole("link", { name: /read/i })).toHaveAttribute("href", "/books/1/read/epub");
    expect(screen.getByRole("link", { name: /download/i })).toHaveAttribute(
      "href",
      "/api/v1/books/1/formats/epub/download",
    );
  });

  test("archive button sends the archive payload", async () => {
    let archivedBody: unknown = null;
    server.use(
      http.post("/api/v1/books/:id/archive", async ({ request }) => {
        archivedBody = await request.json();
        return HttpResponse.json(null, { status: 204 });
      }),
    );

    const user = userEvent.setup();
    renderCard();

    await user.click(screen.getByRole("button", { name: /archive/i }));

    await waitFor(() => {
      expect(archivedBody).toEqual({ is_archived: true });
    });
  });
});
