import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { http, HttpResponse } from "msw";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useAuthStore } from "../../lib/auth-store";
import { makeAnnotation, makeBook, makeUser } from "../../test/fixtures";
import { server } from "../../test/setup";
import { EpubReader } from "./EpubReader";

const relocatedCallbacks: Array<(payload: { start: { percentage: number; cfi: string } }) => void> = [];
const selectedCallbacks: Array<(cfiRange: string, contents: any) => void> = [];
const markClickedCallbacks: Array<(cfiRange: string, data: any, contents: any, pointerEvent: any) => void> = [];

const nextMock = vi.fn(async () => undefined);
const prevMock = vi.fn(async () => undefined);

const mockCreateBook = vi.fn(() => ({
  renderTo: () => ({
    display: vi.fn(async () => undefined),
    next: nextMock,
    prev: prevMock,
    on: vi.fn((event: string, callback: (...args: any[]) => void) => {
      if (event === "relocated") {
        relocatedCallbacks.push(callback as (payload: { start: { percentage: number; cfi: string } }) => void);
      }
      if (event === "selected") {
        selectedCallbacks.push(callback as (cfiRange: string, contents: any) => void);
      }
      if (event === "markClicked") {
        markClickedCallbacks.push(
          callback as (cfiRange: string, data: any, contents: any, pointerEvent: any) => void,
        );
      }
    }),
    annotations: {
      add: vi.fn(),
      remove: vi.fn(),
    },
    themes: {
      default: vi.fn(),
      fontSize: vi.fn(),
    },
    destroy: vi.fn(),
  }),
  loaded: {
    navigation: Promise.resolve({
      toc: [{ id: "chap1", label: "Chapter 1", href: "chap1.xhtml" }],
    }),
  },
  destroy: vi.fn(),
}));

vi.mock("epubjs", () => ({
  default: mockCreateBook,
}));

function renderReader() {
  useAuthStore.setState({
    access_token: "test-token",
    refresh_token: "test-refresh",
    user: makeUser(),
    setAuth: useAuthStore.getState().setAuth,
    clearAuth: useAuthStore.getState().clearAuth,
  });

  return render(
    <EpubReader
      book={makeBook()}
      format="epub"
      streamUrl="/api/v1/books/1/formats/epub/stream"
      initialProgress={null}
      onProgressChange={vi.fn()}
    />,
  );
}

describe("EpubReader", () => {
  beforeEach(() => {
    relocatedCallbacks.length = 0;
    selectedCallbacks.length = 0;
    markClickedCallbacks.length = 0;
    nextMock.mockClear();
    prevMock.mockClear();
    mockCreateBook.mockClear();
  });

  afterEach(() => {
    useAuthStore.getState().clearAuth();
  });

  test("renders the book annotations in the table of contents panel", async () => {
    renderReader();
    const user = userEvent.setup();

    fireEvent.mouseMove(await screen.findByTestId("epub-reader"));
    await user.click(await screen.findByLabelText(/open table of contents/i));
    await user.click(screen.getByRole("button", { name: /annotations/i }));

    expect(await screen.findByText("Arrakis")).toBeTruthy();
  });

  test("relocated events are forwarded as progress updates", async () => {
    const onProgressChange = vi.fn();
    render(
      <EpubReader
        book={makeBook()}
        format="epub"
        streamUrl="/api/v1/books/1/formats/epub/stream"
        initialProgress={null}
        onProgressChange={onProgressChange}
      />,
    );

    await waitFor(() => {
      expect(relocatedCallbacks.length).toBeGreaterThan(0);
    });

    await act(async () => {
      relocatedCallbacks[0]({ start: { percentage: 0.5, cfi: "x" } });
    });

    expect(onProgressChange).toHaveBeenCalledWith({ percentage: 50, cfi: "x", page: null });
  });

  test("creating a highlight posts the annotation payload", async () => {
    let payload: unknown = null;
    server.use(
      http.post("/api/v1/books/:id/annotations", async ({ request }) => {
        payload = await request.json();
        return HttpResponse.json(makeAnnotation(), { status: 201 });
      }),
    );

    const user = userEvent.setup();
    renderReader();

    await waitFor(() => {
      expect(selectedCallbacks.length).toBeGreaterThan(0);
    });

    await act(async () => {
      selectedCallbacks[0]("epubcfi(/6/2)", {
        window: {
          getSelection: () => ({
            toString: () => "Arrakis",
          }),
        },
      });
    });

    await user.click(await screen.findByRole("button", { name: /create yellow highlight/i }));

    await waitFor(() => {
      expect(payload).toMatchObject({
        type: "highlight",
        cfi_range: "epubcfi(/6/2)",
        highlighted_text: "Arrakis",
      });
    });
  });

  test("deleting an annotation calls the delete endpoint", async () => {
    let deletedAnnotationId = "";
    server.use(
      http.delete("/api/v1/books/:id/annotations/:annotationId", ({ params }) => {
        deletedAnnotationId = String(params.annotationId);
        return HttpResponse.json(null, { status: 204 });
      }),
    );

    const user = userEvent.setup();
    renderReader();

    await waitFor(() => {
      expect(markClickedCallbacks.length).toBeGreaterThan(0);
    });

    await act(async () => {
      markClickedCallbacks[0]("epubcfi(/6/2)", { id: "ann-1" }, {}, { clientX: 100, clientY: 100 });
    });
    await user.click(await screen.findByRole("button", { name: /^delete$/i }));

    await waitFor(() => {
      expect(deletedAnnotationId).toBe("ann-1");
    });
  });

  test("arrow keys page next and previous", async () => {
    renderReader();

    await waitFor(() => {
      expect(relocatedCallbacks.length).toBeGreaterThan(0);
    });

    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
    });

    await waitFor(() => {
      expect(nextMock).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(prevMock).toHaveBeenCalled();
    });
  });

  test("escape closes the settings panel", async () => {
    const user = userEvent.setup();
    renderReader();

    fireEvent.mouseMove(await screen.findByTestId("epub-reader"));
    await user.click(await screen.findByLabelText(/open settings/i));
    expect(await screen.findByText(/reader settings/i)).toBeTruthy();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));

    await waitFor(() => {
      expect(screen.queryByText(/reader settings/i)).toBeNull();
    });
  });

  test("the back link points to the book detail page", async () => {
    renderReader();

    fireEvent.mouseMove(await screen.findByTestId("epub-reader"));
    expect((await screen.findByLabelText(/back/i)).getAttribute("href")).toBe("/books/1");
  });
});
