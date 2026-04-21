import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SearchBar } from "../features/search/SearchBar";
import { apiClient } from "../lib/api-client";

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

const searchSuggestionsMock = vi.spyOn(apiClient, "searchSuggestions");

function renderBar() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <SearchBar />
    </QueryClientProvider>,
  );
}

describe("SearchBar", () => {
  beforeEach(() => {
    navigateMock.mockReset();
    searchSuggestionsMock.mockReset();
    window.localStorage.removeItem?.("calibre-web.recent-searches:anon");
  });

  afterEach(() => {
    cleanup();
  });

  test("test_suggestions_appear_on_input", async () => {
    searchSuggestionsMock.mockResolvedValue({
      suggestions: ["Dune", "Dune Messiah"],
    });

    renderBar();

    fireEvent.change(screen.getByPlaceholderText("Search title, author, tag"), {
      target: { value: "dune" },
    });

    expect(await screen.findByRole("button", { name: "Dune" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Dune Messiah" })).toBeTruthy();
  });

  test("test_commit_search_navigates_to_search_page", async () => {
    searchSuggestionsMock.mockResolvedValue({
      suggestions: ["Dune"],
    });

    renderBar();

    fireEvent.change(screen.getByPlaceholderText("Search title, author, tag"), {
      target: { value: "dune" },
    });

    expect(await screen.findByRole("button", { name: "Dune" })).toBeTruthy();
    fireEvent.click(await screen.findByRole("button", { name: "Search" }));

    expect(navigateMock).toHaveBeenCalledWith({
      to: "/search",
      search: { q: "dune" },
    });
  });
});
