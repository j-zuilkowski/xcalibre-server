import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ComicReaderProps } from "./types";
import { useAuthStore } from "../../lib/auth-store";

type ComicPageEntry = {
  index: number;
  url: string;
};

type ComicPagesResponse = {
  total_pages: number;
  pages: ComicPageEntry[];
};

function authHeaders(token: string | null): HeadersInit {
  return token ? { Authorization: `Bearer ${token}` } : {};
}

export function ComicReader({ bookId, onProgressChange }: ComicReaderProps) {
  const { t } = useTranslation();
  const token = useAuthStore((state) => state.access_token);
  const [pages, setPages] = useState<ComicPageEntry[]>([]);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [currentSrc, setCurrentSrc] = useState<string | null>(null);
  const [loadingPages, setLoadingPages] = useState(true);
  const [loadingImage, setLoadingImage] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const cacheRef = useRef<Map<number, string>>(new Map());

  const totalPages = pages.length;

  useEffect(() => {
    let cancelled = false;

    async function loadPages() {
      try {
        setLoadingPages(true);
        const response = await fetch(`/api/v1/books/${encodeURIComponent(bookId)}/comic/pages`, {
          headers: authHeaders(token),
        });
        if (!response.ok) {
          throw new Error(`status ${response.status}`);
        }
        const data = (await response.json()) as ComicPagesResponse;
        if (cancelled) {
          return;
        }
        setPages(data.pages);
        setCurrentIndex(0);
        setError(null);
      } catch {
        if (!cancelled) {
          setError(t("reader.unable_to_load_comic_pages"));
        }
      } finally {
        if (!cancelled) {
          setLoadingPages(false);
        }
      }
    }

    void loadPages();

    return () => {
      cancelled = true;
    };
  }, [bookId, token]);

  useEffect(() => {
    if (totalPages === 0) {
      setCurrentSrc(null);
      return;
    }

    let cancelled = false;

    async function ensurePage(index: number): Promise<string | null> {
      const cached = cacheRef.current.get(index);
      if (cached) {
        return cached;
      }

      const page = pages[index];
      if (!page) {
        return null;
      }

      const response = await fetch(page.url, {
        headers: authHeaders(token),
      });
      if (!response.ok) {
        throw new Error(`status ${response.status}`);
      }
      const blob = await response.blob();
      const objectUrl = URL.createObjectURL(blob);
      cacheRef.current.set(index, objectUrl);
      return objectUrl;
    }

    async function loadCurrentPage() {
      try {
        setLoadingImage(true);
        const src = await ensurePage(currentIndex);
        if (cancelled) {
          return;
        }
        setCurrentSrc(src);
        if (currentIndex + 1 < pages.length) {
          void ensurePage(currentIndex + 1);
        }
      } catch {
        if (!cancelled) {
          setError(t("reader.unable_to_load_comic_page"));
        }
      } finally {
        if (!cancelled) {
          setLoadingImage(false);
        }
      }
    }

    void loadCurrentPage();

    return () => {
      cancelled = true;
    };
  }, [currentIndex, pages, token, totalPages]);

  useEffect(() => {
    return () => {
      for (const objectUrl of cacheRef.current.values()) {
        URL.revokeObjectURL(objectUrl);
      }
      cacheRef.current.clear();
    };
  }, []);

  useEffect(() => {
    if (totalPages > 0) {
      onProgressChange?.({
        percentage: ((currentIndex + 1) / totalPages) * 100,
        page: currentIndex + 1,
        cfi: null,
      });
    }
  }, [currentIndex, onProgressChange, totalPages]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "ArrowRight") {
        event.preventDefault();
        setCurrentIndex((previous) => Math.min(previous + 1, Math.max(0, pages.length - 1)));
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        setCurrentIndex((previous) => Math.max(previous - 1, 0));
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [pages.length]);

  const canGoPrevious = currentIndex > 0;
  const canGoNext = currentIndex + 1 < pages.length;

  const counterLabel = useMemo(() => {
    if (pages.length === 0) {
      return "0 / 0";
    }
    return `${currentIndex + 1} / ${pages.length}`;
  }, [currentIndex, pages.length]);

  if (loadingPages) {
    return (
      <div className="grid h-full place-items-center bg-zinc-950 text-zinc-200">
        {t("reader.loading_comic")}
      </div>
    );
  }

  if (error) {
    return (
      <div className="grid h-full place-items-center bg-zinc-950 text-red-300">
        {error}
      </div>
    );
  }

  if (pages.length === 0) {
    return (
      <div className="grid h-full place-items-center bg-zinc-950 text-zinc-300">
        {t("reader.no_comic_pages_available")}
      </div>
    );
  }

  return (
    <section className="flex h-full w-full flex-col bg-zinc-950 text-zinc-100" data-testid="comic-reader">
      <header className="flex items-center justify-between border-b border-zinc-800 px-4 py-3 text-sm text-zinc-300">
        <button
          type="button"
          onClick={() => setCurrentIndex((previous) => Math.max(previous - 1, 0))}
          disabled={!canGoPrevious}
          className="rounded border border-zinc-700 px-3 py-1.5 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {t("common.previous")}
        </button>
        <div data-testid="comic-counter">{counterLabel}</div>
        <button
          type="button"
          onClick={() => setCurrentIndex((previous) => Math.min(previous + 1, pages.length - 1))}
          disabled={!canGoNext}
          className="rounded border border-zinc-700 px-3 py-1.5 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {t("common.next")}
        </button>
      </header>

      <div className="flex flex-1 items-center justify-center overflow-auto p-4">
        {currentSrc ? (
          <img
            src={currentSrc}
            alt={`Page ${currentIndex + 1}`}
            className={`max-h-[calc(100vh-8rem)] w-full max-w-full object-contain transition-opacity duration-300 ${
              loadingImage ? "opacity-50" : "opacity-100"
            }`}
          />
        ) : null}
      </div>
    </section>
  );
}
