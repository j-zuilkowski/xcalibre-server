import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { http, HttpResponse } from "msw";
import { screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { renderWithProviders } from "../../test/render";
import { makeProgress } from "../../test/fixtures";
import { server } from "../../test/setup";

vi.mock("./EpubReader", () => ({
  EpubReader: ({
    onProgressChange,
  }: {
    onProgressChange?: (progress: { percentage: number; cfi?: string | null; page?: number | null }) => void;
  }) => {
    useEffect(() => {
      onProgressChange?.({ percentage: 50, cfi: "epubcfi(/6/2)", page: null });
    }, [onProgressChange]);

    return <div data-testid="epub-reader" />;
  },
}));

vi.mock("./PdfReader", () => ({
  PdfReader: () => <div data-testid="pdf-reader" />,
}));

vi.mock("./ComicReader", () => ({
  ComicReader: () => <div data-testid="comic-reader" />,
}));

vi.mock("./DjvuReader", () => ({
  DjvuReader: () => <div data-testid="djvu-reader" />,
}));

vi.mock("./AudioReader", () => ({
  AudioReader: () => <div data-testid="audio-reader" />,
}));

describe("ReaderPage", () => {
  test("loads the EPUB reader for epub format", async () => {
    renderWithProviders(<></>, {
      initialPath: "/books/1/read/epub",
      authenticated: true,
    });

    expect(await screen.findByTestId("epub-reader")).toBeTruthy();
  });

  test("loads the PDF reader for pdf format", async () => {
    renderWithProviders(<></>, {
      initialPath: "/books/1/read/pdf",
      authenticated: true,
    });

    expect(await screen.findByTestId("pdf-reader")).toBeTruthy();
  });

  test("loads the comic reader for cbz format", async () => {
    renderWithProviders(<></>, {
      initialPath: "/books/1/read/cbz",
      authenticated: true,
    });

    expect(await screen.findByTestId("comic-reader")).toBeTruthy();
  });

  test("loads the djvu reader for djvu format", async () => {
    renderWithProviders(<></>, {
      initialPath: "/books/1/read/djvu",
      authenticated: true,
    });

    expect(await screen.findByTestId("djvu-reader")).toBeTruthy();
  });

  test("progress updates are forwarded to the save endpoint", async () => {
    let patchBody: unknown = null;
    server.use(
      http.patch("/api/v1/books/:id/progress", async ({ request }) => {
        patchBody = await request.json();
        return HttpResponse.json(makeProgress());
      }),
    );

    renderWithProviders(<></>, {
      initialPath: "/books/1/read/epub",
      authenticated: true,
    });

    await screen.findByTestId("epub-reader");

    await waitFor(() => {
      expect(patchBody).toMatchObject({ cfi: "epubcfi(/6/2)", percentage: 50 });
    });
  });
});
