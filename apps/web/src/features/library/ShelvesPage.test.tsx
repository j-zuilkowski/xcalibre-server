import { describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { makeBookSummary, makeShelf } from "../../test/fixtures";
import { server } from "../../test/setup";
import { screen, waitFor } from "@testing-library/react";

function renderShelvesPage() {
  return renderWithProviders(<></>, {
    initialPath: "/shelves",
    authenticated: true,
  });
}

describe("ShelvesPage", () => {
  test("renders the shelf list and selected shelf books", async () => {
    renderShelvesPage();

    expect(await screen.findByRole("button", { name: /favorites/i })).toBeTruthy();
    expect(await screen.findByText("Dune")).toBeTruthy();
  });

  test("create shelf submits the new shelf name", async () => {
    let createdShelfName: string | null = null;
    server.use(
      http.post("/api/v1/shelves", async ({ request }) => {
        const body = (await request.json()) as { name?: string };
        createdShelfName = body.name ?? null;
        return HttpResponse.json(makeShelf({ id: "shelf-2", name: body.name ?? "New shelf" }), {
          status: 201,
        });
      }),
    );

    const user = userEvent.setup();
    renderShelvesPage();

    await user.click(await screen.findByRole("button", { name: /create shelf/i }));
    await user.type(screen.getByRole("textbox", { name: /shelf name/i }), "Sci-Fi");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(createdShelfName).toBe("Sci-Fi");
    });
  });

  test("clicking another shelf shows its books", async () => {
    server.use(
      http.get("/api/v1/shelves", () =>
        HttpResponse.json([
          makeShelf({ id: "shelf-1", name: "Favorites" }),
          makeShelf({ id: "shelf-2", name: "Read later", book_count: 1 }),
        ]),
      ),
      http.get("/api/v1/shelves/:id/books", ({ params }) =>
        HttpResponse.json({
          items:
            String(params.id) === "shelf-2"
              ? [makeBookSummary({ id: "2", title: "Children of Dune" })]
              : [makeBookSummary()],
          total: 1,
          page: 1,
          page_size: 100,
        }),
      ),
    );

    const user = userEvent.setup();
    renderShelvesPage();

    await user.click(await screen.findByRole("button", { name: /read later/i }));

    expect(await screen.findByText("Children of Dune")).toBeTruthy();
  });

  test("remove book from shelf calls the delete endpoint", async () => {
    let removedPath = "";
    server.use(
      http.delete("/api/v1/shelves/:id/books/:bookId", ({ params }) => {
        removedPath = `/api/v1/shelves/${params.id}/books/${params.bookId}`;
        return HttpResponse.json(null, { status: 204 });
      }),
    );

    const user = userEvent.setup();
    renderShelvesPage();

    await user.click(await screen.findByRole("button", { name: /remove/i }));

    await waitFor(() => {
      expect(removedPath).toBe("/api/v1/shelves/shelf-1/books/1");
    });
  });
});
