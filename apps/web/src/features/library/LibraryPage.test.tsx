import { describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { makeBookSummary } from "../../test/fixtures";
import { server } from "../../test/setup";
import { screen, waitFor } from "@testing-library/react";

function renderLibraryPage(path = "/library") {
  return renderWithProviders(<></>, {
    initialPath: path,
    authenticated: true,
  });
}

describe("LibraryPage", () => {
  test("shows the empty state by default", async () => {
    renderLibraryPage();

    expect(await screen.findByText(/no books in your library yet/i)).toBeTruthy();
  });

  test("shows a loading skeleton while books are loading", async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });

    server.use(
      http.get("/api/v1/books", async () => {
        await pending;
        return HttpResponse.json({
          items: [makeBookSummary()],
          total: 1,
          page: 1,
          page_size: 24,
        });
      }),
    );

    renderLibraryPage();

    await waitFor(() => {
      expect(document.querySelectorAll(".animate-pulse").length).toBeGreaterThan(0);
    });
    release();
  });

  test("renders book cards", async () => {
    server.use(
      http.get("/api/v1/books", () =>
        HttpResponse.json({
          items: [makeBookSummary(), makeBookSummary({ id: "2", title: "Children of Dune" })],
          total: 2,
          page: 1,
          page_size: 24,
        }),
      ),
    );

    const { container } = renderLibraryPage();

    expect(await screen.findByText("Dune")).toBeTruthy();
    expect(screen.getByText("Children of Dune")).toBeTruthy();
    expect(container.querySelectorAll("[data-book-card='true']")).toHaveLength(2);
  });

  test("shows an error state when loading fails", async () => {
    server.use(
      http.get("/api/v1/books", () => HttpResponse.json({ message: "oops" }, { status: 500 })),
    );

    renderLibraryPage();

    expect(await screen.findByText(/unable to load library/i)).toBeTruthy();
  });

  test("format filtering updates the query string", async () => {
    const requests: string[] = [];
    server.use(
      http.get("/api/v1/books", ({ request }) => {
        requests.push(new URL(request.url).search);
        return HttpResponse.json({
          items: [makeBookSummary()],
          total: 24,
          page: 1,
          page_size: 24,
        });
      }),
    );

    const user = userEvent.setup();
    renderLibraryPage();
    await screen.findByText("Dune");

    await user.click(screen.getByRole("button", { name: /^format$/i }));

    await waitFor(() => {
      expect(requests.at(-1)).toContain("format=epub");
    });
  });

  test("sort changes issue a new request", async () => {
    const requests: string[] = [];
    server.use(
      http.get("/api/v1/books", ({ request }) => {
        requests.push(new URL(request.url).search);
        return HttpResponse.json({
          items: [makeBookSummary()],
          total: 24,
          page: 1,
          page_size: 24,
        });
      }),
    );

    const user = userEvent.setup();
    renderLibraryPage();
    await screen.findByText("Dune");

    await user.selectOptions(screen.getByLabelText(/sort/i), "rating");

    await waitFor(() => {
      expect(requests.at(-1)).toContain("sort=rating");
    });
  });

  test("pagination requests the next page", async () => {
    const requests: string[] = [];
    server.use(
      http.get("/api/v1/books", ({ request }) => {
        const url = new URL(request.url);
        requests.push(url.search);
        const page = Number(url.searchParams.get("page") ?? "1");
        const items =
          page === 2
            ? [makeBookSummary({ id: "2", title: "Children of Dune" })]
            : [makeBookSummary()];
        return HttpResponse.json({
          items,
          total: 48,
          page,
          page_size: 24,
        });
      }),
    );

    const user = userEvent.setup();
    renderLibraryPage();
    await screen.findByText("Dune");

    await user.click(screen.getByRole("button", { name: /next/i }));

    await waitFor(() => {
      expect(requests.at(-1)).toContain("page=2");
    });
  });
});
