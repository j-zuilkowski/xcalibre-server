import { afterEach, describe, expect, test } from "vitest";
import { QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { I18nextProvider } from "react-i18next";
import { BrowsePage } from "./BrowsePage";
import i18n from "../../i18n";
import { makeBookSummary } from "../../test/fixtures";
import { makeTestQueryClient } from "../../test/query-client";
import { server } from "../../test/setup";

function renderBrowsePage(documentType = "Book") {
  const queryClient = makeTestQueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: Infinity,
      },
    },
  });

  return render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={queryClient}>
        <BrowsePage documentType={documentType} />
      </QueryClientProvider>
    </I18nextProvider>,
  );
}

describe("BrowsePage", () => {
  afterEach(() => {
    cleanup();
  });

  test("renders grid of books for the given documentType", async () => {
    server.use(
      http.get("/api/v1/books", ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get("document_type")).toBe("Book");
        return HttpResponse.json({
          items: [
            makeBookSummary({ id: "browse-a", title: "Atlas", sort_title: "Atlas" }),
            makeBookSummary({ id: "browse-b", title: "Binary", sort_title: "Binary" }),
            makeBookSummary({ id: "browse-z", title: "Zebra", sort_title: "Zebra" }),
          ],
          total: 3,
          page: 1,
          page_size: 200,
        });
      }),
    );

    const { container } = renderBrowsePage("Book");

    expect(await screen.findByText("Atlas")).toBeTruthy();
    expect(screen.getByText("Binary")).toBeTruthy();
    expect(screen.getByText("Zebra")).toBeTruthy();
    expect(container.querySelectorAll("[data-book-card='true']")).toHaveLength(3);
  });

  test("alpha sidebar renders clickable buttons only for letters that have books", async () => {
    renderBrowsePage("Book");

    expect(await screen.findByText("Atlas")).toBeTruthy();

    expect(screen.getByRole("button", { name: "A" })).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "B" })).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "Z" })).not.toBeDisabled();
  });

  test("letters with no books render as non-interactive", async () => {
    renderBrowsePage("Book");

    expect(await screen.findByText("Atlas")).toBeTruthy();

    expect(screen.getByRole("button", { name: "C" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "C" })).toHaveClass("pointer-events-none");
    expect(screen.getByRole("button", { name: "C" })).toHaveClass("opacity-40");
  });
});
