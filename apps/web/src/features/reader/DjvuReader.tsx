import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAuthStore } from "../../lib/auth-store";
import type { ReaderProgressUpdate } from "./types";

type DjvuReaderProps = {
  bookId: string;
  format: string;
  initialProgressPage?: number | null;
  onProgressChange?: (progress: ReaderProgressUpdate) => void;
};

type DjvuApp = {
  loadDocument?: (bytes: ArrayBuffer) => Promise<void> | void;
  load?: (bytes: ArrayBuffer) => Promise<void> | void;
  getPageCount?: () => number;
  getPagesQuantity?: () => number;
  pageCount?: number;
  pagesCount?: number;
  renderPage?: (...args: unknown[]) => Promise<void> | void;
  drawPage?: (...args: unknown[]) => Promise<void> | void;
  setPage?: (page: number) => Promise<void> | void;
  gotoPage?: (page: number) => Promise<void> | void;
  destroy?: () => void;
};

type DjvuAppCtor = new (...args: unknown[]) => DjvuApp;

function clampPage(value: number, total: number): number {
  if (!Number.isFinite(value)) {
    return 1;
  }
  return Math.max(1, Math.min(Math.max(1, total), Math.round(value)));
}

function authHeaders(token: string | null): HeadersInit {
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function loadDjvuModule(): Promise<{ App?: DjvuAppCtor }> {
  const fallback = (await import(
    /* @vite-ignore */ "https://cdn.jsdelivr.net/npm/djvu.js@0.3.2/dist/djvu.min.js"
  )) as Record<string, unknown>;
  const root = (fallback.default ?? fallback) as Record<string, unknown>;
  const nested = (root.DjVu ?? root.djvu ?? root) as Record<string, unknown>;
  return { App: nested.App as DjvuAppCtor | undefined };
}

function pageCountFromApp(app: DjvuApp): number {
  const explicit = app.getPageCount?.() ?? app.getPagesQuantity?.() ?? app.pageCount ?? app.pagesCount ?? 1;
  return Number.isFinite(explicit) && explicit > 0 ? Math.round(explicit) : 1;
}

async function renderWithBestEffort(app: DjvuApp, canvas: HTMLCanvasElement, page: number): Promise<void> {
  if (typeof app.renderPage === "function") {
    await Promise.resolve(app.renderPage(canvas, page));
    return;
  }

  if (typeof app.drawPage === "function") {
    await Promise.resolve(app.drawPage(canvas, page));
    return;
  }

  if (typeof app.setPage === "function") {
    await Promise.resolve(app.setPage(page));
    return;
  }

  if (typeof app.gotoPage === "function") {
    await Promise.resolve(app.gotoPage(page));
    return;
  }

  throw new Error("djvu renderer unavailable");
}

export function DjvuReader({ bookId, format, initialProgressPage, onProgressChange }: DjvuReaderProps) {
  const { t } = useTranslation();
  const token = useAuthStore((state) => state.access_token);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const appRef = useRef<DjvuApp | null>(null);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [totalPages, setTotalPages] = useState(1);
  const [currentPage, setCurrentPage] = useState(1);

  const streamUrl = useMemo(
    () => `/api/v1/books/${encodeURIComponent(bookId)}/formats/${encodeURIComponent(format)}/stream`,
    [bookId, format],
  );

  const renderPage = useCallback(async (page: number) => {
    const app = appRef.current;
    const canvas = canvasRef.current;
    if (!app || !canvas) {
      return;
    }

    const safePage = clampPage(page, totalPages);

    try {
      await renderWithBestEffort(app, canvas, safePage);
      setError(null);
    } catch {
      if (safePage > 1) {
        await renderWithBestEffort(app, canvas, safePage - 1);
      } else {
        throw new Error("djvu page render failed");
      }
    }
  }, [totalPages]);

  useEffect(() => {
    let cancelled = false;

    async function initialize() {
      try {
        setLoading(true);
        setError(null);

        const response = await fetch(streamUrl, { headers: authHeaders(token) });
        if (!response.ok) {
          throw new Error(`status ${response.status}`);
        }

        const bytes = await response.arrayBuffer();
        const module = await loadDjvuModule();
        if (!module.App) {
          throw new Error("djvu app constructor not found");
        }

        const app = new module.App();
        if (typeof app.loadDocument === "function") {
          await Promise.resolve(app.loadDocument(bytes));
        } else if (typeof app.load === "function") {
          await Promise.resolve(app.load(bytes));
        } else {
          throw new Error("djvu load function not available");
        }

        if (cancelled) {
          return;
        }

        appRef.current = app;
        const pages = pageCountFromApp(app);
        setTotalPages(pages);

        const initialPage =
          typeof initialProgressPage === "number" && Number.isFinite(initialProgressPage)
            ? clampPage(initialProgressPage, pages)
            : 1;

        setCurrentPage(initialPage);
        if (!canvasRef.current) {
          throw new Error("djvu canvas unavailable");
        }
        await renderWithBestEffort(app, canvasRef.current, initialPage);
      } catch {
        if (!cancelled) {
          setError(t("reader.unable_to_load"));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void initialize();

    return () => {
      cancelled = true;
      appRef.current?.destroy?.();
      appRef.current = null;
    };
  }, [initialProgressPage, streamUrl, t, token]);

  useEffect(() => {
    let cancelled = false;

    async function updatePage() {
      if (!appRef.current || !canvasRef.current || loading) {
        return;
      }
      try {
        await renderPage(currentPage);
      } catch {
        if (!cancelled) {
          setError(t("reader.unable_to_load"));
        }
      }
    }

    void updatePage();

    return () => {
      cancelled = true;
    };
  }, [currentPage, loading, renderPage, t]);

  useEffect(() => {
    const percentage = totalPages > 0 ? (currentPage / totalPages) * 100 : 0;
    onProgressChange?.({ percentage, page: currentPage, cfi: null });
  }, [currentPage, onProgressChange, totalPages]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "ArrowRight") {
        event.preventDefault();
        setCurrentPage((previous) => clampPage(previous + 1, totalPages));
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        setCurrentPage((previous) => clampPage(previous - 1, totalPages));
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [totalPages]);

  if (loading) {
    return (
      <section className="grid h-full w-full place-items-center bg-zinc-950 text-zinc-100" data-testid="djvu-reader-loading">
        <div className="flex items-center gap-3 text-sm">
          <span className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-zinc-500 border-t-teal-400" />
          <span>{t("reader.loading_reader")}</span>
        </div>
      </section>
    );
  }

  if (error) {
    return (
      <section className="grid h-full w-full place-items-center bg-zinc-950 text-red-300" data-testid="djvu-reader-error">
        {error}
      </section>
    );
  }

  return (
    <section className="flex h-full w-full flex-col bg-zinc-950 text-zinc-100" data-testid="djvu-reader">
      <header className="flex items-center justify-between border-b border-zinc-800 px-4 py-3 text-sm text-zinc-300">
        <button
          type="button"
          onClick={() => setCurrentPage((previous) => clampPage(previous - 1, totalPages))}
          disabled={currentPage <= 1}
          className="rounded border border-zinc-700 px-3 py-1.5 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {t("common.previous")}
        </button>
        <div>{currentPage} / {totalPages}</div>
        <button
          type="button"
          onClick={() => setCurrentPage((previous) => clampPage(previous + 1, totalPages))}
          disabled={currentPage >= totalPages}
          className="rounded border border-zinc-700 px-3 py-1.5 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {t("common.next")}
        </button>
      </header>

      <div className="flex flex-1 items-center justify-center p-4">
        <canvas ref={canvasRef} className="max-h-full max-w-full rounded border border-zinc-800 bg-zinc-900" />
      </div>
    </section>
  );
}
