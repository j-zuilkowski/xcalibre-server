import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import type { BookSummary } from "@xs/shared";
import { HomePage } from "./HomePage";
import { makeBookSummary } from "../../test/fixtures";
import { makeTestQueryClient } from "../../test/query-client";
import { server } from "../../test/setup";

const { navigateMock } = vi.hoisted(() => ({
  navigateMock: vi.fn(),
}));

vi.mock("@tanstack/react-router", async () => {
  const actual = await vi.importActual<typeof import("@tanstack/react-router")>(
    "@tanstack/react-router",
  );

  return {
    ...actual,
    useNavigate: () => navigateMock,
  };
});

function renderHomePage() {
  const queryClient = makeTestQueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: Infinity,
      },
      mutations: {
        retry: 0,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <HomePage />
    </QueryClientProvider>,
  );
}

describe("HomePage", () => {
  beforeEach(() => {
    navigateMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  test("renders Continue Reading row when in-progress books exist", async () => {
    const startedBook: BookSummary = makeBookSummary({ id: "started-1", title: "Dune" });

    server.use(
      http.get("/api/v1/books/in-progress", () => HttpResponse.json([startedBook])),
    );

    renderHomePage();

    expect(await screen.findByRole("heading", { name: /continue reading/i })).toBeTruthy();
    expect(screen.getByText("Dune")).toBeTruthy();
  });

  test("hides Continue Reading row when no in-progress books", async () => {
    server.use(http.get("/api/v1/books/in-progress", () => HttpResponse.json([])));

    renderHomePage();

    expect(await screen.findByRole("heading", { name: /recently added/i })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: /continue reading/i })).toBeNull();
  });

  test("always renders Recently Added row", async () => {
    renderHomePage();

    expect(await screen.findByRole("heading", { name: /recently added/i })).toBeTruthy();
    expect(await screen.findByText("Children of Dune")).toBeTruthy();
  });

  test("search hero navigates to /search on submit", async () => {
    const user = userEvent.setup();
    renderHomePage();

    const input = await screen.findByPlaceholderText(/search books, authors, topics/i);
    await user.type(input, "dune{enter}");

    expect(navigateMock).toHaveBeenCalledWith({
      to: "/search",
      search: { q: "dune" },
    });
  });
});
