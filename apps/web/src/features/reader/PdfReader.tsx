/**
 * PdfReader — pdfjs-dist-based PDF reading component.
 *
 * pdf.js is loaded via dynamic import so the worker bundle (~900 kB) is only
 * fetched when the reader opens.  The worker src is set once from
 * `import.meta.url` so Vite can include it in the build output.
 *
 * Page rendering:
 *   Each page is rendered to an HTML5 <canvas> at the chosen zoom level.
 *   A separate useEffect renders the canvas whenever `currentPage` or `zoom`
 *   changes, using a `cancelled` flag to discard stale async renders if the
 *   user navigates quickly.
 *
 * Page-based progress:
 *   Progress percentage = (currentPage / totalPages) * 100.
 *   `onProgressChange` is called on every page change; the parent
 *   (ReaderPage) debounces and persists the value.
 *   On mount, `initialProgress.page` is used as the starting page; if only
 *   a percentage is available it is converted to a page number.
 *
 * Settings sheet (right): zoom slider (0.8×–1.8×).
 * TOC sheet (left): placeholder — PDF outline not yet implemented.
 *
 * Keyboard navigation: ArrowLeft / ArrowRight change page and smoothly scroll
 * the viewport, matching the epub reader's key bindings.
 *
 * API calls:
 *   GET /api/v1/books/:id/formats/:format/stream  (PDF binary, credentials included)
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { apiClient } from "../../lib/api-client";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "../../components/ui/sheet";
import { useReaderToolbar } from "./useReaderToolbar";
import type { ReaderComponentProps } from "./types";

type PdfDocumentProxy = {
  numPages: number;
  getPage: (page: number) => Promise<{
    getViewport: (options: { scale: number }) => { width: number; height: number };
    render: (options: { canvasContext: CanvasRenderingContext2D; viewport: { width: number; height: number } }) => {
      promise: Promise<void>;
    };
  }>;
};

function clampPage(page: number, totalPages: number): number {
  return Math.max(1, Math.min(totalPages || 1, page));
}

/**
 * PdfReader renders a PDF page on a <canvas> using pdfjs-dist.
 *
 * @param book            - Book metadata (id, title, authors).
 * @param format          - Format string (e.g. "pdf").
 * @param initialProgress - Saved page number and/or percentage for resuming.
 * @param onProgressChange - Called on each page change with the new percentage
 *                          and page number.
 */
export function PdfReader({ book, format, initialProgress, onProgressChange }: ReaderComponentProps) {
  const { t } = useTranslation();
  const streamUrl = useMemo(() => apiClient.streamUrl(book.id, format), [book.id, format]);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const pdfDocRef = useRef<PdfDocumentProxy | null>(null);

  const [totalPages, setTotalPages] = useState(1);
  const [currentPage, setCurrentPage] = useState(Math.max(1, initialProgress?.page ?? 1));
  const [zoom, setZoom] = useState(1.1);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [tocOpen, setTocOpen] = useState(false);
  const [engineUnavailable, setEngineUnavailable] = useState(false);
  const { toolbarVisible, showToolbar } = useReaderToolbar();

  useEffect(() => {
    let cancelled = false;

    async function loadPdf() {
      try {
        const module = (await import("pdfjs-dist")) as any;
        const pdfjs = module?.default ?? module;

        if (!pdfjs?.getDocument) {
          setEngineUnavailable(true);
          return;
        }

        if (pdfjs.GlobalWorkerOptions && !pdfjs.GlobalWorkerOptions.workerSrc) {
          try {
            pdfjs.GlobalWorkerOptions.workerSrc = new URL(
              "pdfjs-dist/build/pdf.worker.min.mjs",
              import.meta.url,
            ).toString();
          } catch {
            // no-op
          }
        }

        const loadingTask = pdfjs.getDocument({ url: streamUrl, withCredentials: true });
        const doc = (await loadingTask.promise) as PdfDocumentProxy;

        if (cancelled) {
          return;
        }

        pdfDocRef.current = doc;
        setTotalPages(doc.numPages);

        const startPage =
          initialProgress?.page ??
          Math.max(1, Math.round(((initialProgress?.percentage ?? 0) / 100) * doc.numPages));

        setCurrentPage(clampPage(startPage, doc.numPages));
      } catch {
        if (!cancelled) {
          setEngineUnavailable(true);
        }
      }
    }

    void loadPdf();

    return () => {
      cancelled = true;
      pdfDocRef.current = null;
    };
  }, [initialProgress?.page, initialProgress?.percentage, streamUrl]);

  useEffect(() => {
    let cancelled = false;

    async function renderPage() {
      if (!pdfDocRef.current || !canvasRef.current) {
        return;
      }

      try {
        const page = await pdfDocRef.current.getPage(currentPage);
        const viewport = page.getViewport({ scale: zoom });

        if (cancelled || !canvasRef.current) {
          return;
        }

        const canvas = canvasRef.current;
        const context = canvas.getContext("2d");
        if (!context) {
          return;
        }

        canvas.width = viewport.width;
        canvas.height = viewport.height;

        await page.render({ canvasContext: context, viewport }).promise;
      } catch {
        setEngineUnavailable(true);
      }
    }

    void renderPage();

    return () => {
      cancelled = true;
    };
  }, [currentPage, zoom]);

  useEffect(() => {
    const percentage = totalPages > 0 ? (currentPage / totalPages) * 100 : 0;
    onProgressChange({ percentage, page: currentPage, cfi: null });
  }, [currentPage, totalPages, onProgressChange]);

  const onArrowNavigation = useCallback(
    (direction: 1 | -1) => {
      if (viewportRef.current) {
        viewportRef.current.scrollBy({ top: direction * 220, behavior: "smooth" });
      }

      setCurrentPage((previous) => clampPage(previous + direction, totalPages));
    },
    [totalPages],
  );

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "ArrowRight") {
        event.preventDefault();
        onArrowNavigation(1);
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        onArrowNavigation(-1);
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onArrowNavigation]);

  const progress = totalPages > 0 ? Math.round((currentPage / totalPages) * 100) : 0;

  return (
    <div
      data-testid="pdf-reader"
      className="relative h-full w-full overflow-hidden bg-zinc-950"
      onMouseMove={showToolbar}
      onPointerMove={showToolbar}
    >
      <div ref={viewportRef} className="flex h-full w-full items-start justify-center overflow-auto p-6">
        <canvas ref={canvasRef} className="rounded border border-zinc-800 bg-white" />
      </div>

      {engineUnavailable ? (
        <div className="pointer-events-none absolute inset-0 grid place-items-center text-sm text-zinc-400">
          {t("reader.pdf_rendering_unavailable")}
        </div>
      ) : null}

      <header
        data-testid="reader-toolbar"
        data-visible={toolbarVisible ? "true" : "false"}
        aria-hidden={toolbarVisible ? "false" : "true"}
        onFocusCapture={showToolbar}
        className={`absolute left-0 right-0 top-0 z-20 border-b border-zinc-800 bg-zinc-950/90 px-4 py-3 transition-opacity duration-300 ${
          toolbarVisible ? "opacity-100" : "pointer-events-none opacity-0"
        }`}
      >
        <div className="flex items-center justify-between gap-3">
          <a
            href={`/books/${encodeURIComponent(book.id)}`}
            aria-label={t("common.back")}
            tabIndex={toolbarVisible ? 0 : -1}
            className="text-sm font-medium text-zinc-100"
          >
            ←
          </a>
          <div className="min-w-0 flex-1 truncate text-center text-sm text-zinc-300">
            {book.title} · {book.authors.map((author) => author.name).join(", ") || t("common.unknown_author")}
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              aria-label={t("reader.open_settings")}
              tabIndex={toolbarVisible ? 0 : -1}
              onClick={() => setSettingsOpen(true)}
              className="rounded border border-zinc-700 px-2 py-1 text-xs"
            >
              ⚙
            </button>
            <button
              type="button"
              aria-label={t("reader.open_table_of_contents")}
              tabIndex={toolbarVisible ? 0 : -1}
              onClick={() => setTocOpen(true)}
              className="rounded border border-zinc-700 px-2 py-1 text-xs"
            >
              ☰
            </button>
          </div>
        </div>
      </header>

      <div className="absolute inset-x-0 bottom-0 z-20 border-t border-zinc-800 bg-zinc-950/90 px-4 py-2">
        <div className="h-1 w-full overflow-hidden rounded bg-zinc-700">
          <div className="h-full bg-teal-500" style={{ width: `${progress}%` }} />
        </div>
        <p data-testid="reader-progress-label" className="mt-1 text-right text-xs text-zinc-300">
          {progress}%
        </p>
      </div>

      <Sheet open={settingsOpen} onOpenChange={setSettingsOpen}>
        <SheetContent side="right">
          <SheetHeader>
            <SheetTitle>{t("reader.reader_settings")}</SheetTitle>
          </SheetHeader>
          <div className="space-y-4 p-5 text-sm">
            <label className="block font-medium">{t("reader.zoom", { value: zoom.toFixed(1) })}</label>
            <input
              type="range"
              min={0.8}
              max={1.8}
              step={0.1}
              value={zoom}
              onChange={(event) => setZoom(Number(event.target.value))}
            />
          </div>
        </SheetContent>
      </Sheet>

      <Sheet open={tocOpen} onOpenChange={setTocOpen}>
        <SheetContent side="left">
          <SheetHeader>
            <SheetTitle>{t("reader.table_of_contents")}</SheetTitle>
          </SheetHeader>
          <div className="p-5 text-sm text-zinc-400">{t("reader.no_table_of_contents_pdf")}</div>
        </SheetContent>
      </Sheet>
    </div>
  );
}
