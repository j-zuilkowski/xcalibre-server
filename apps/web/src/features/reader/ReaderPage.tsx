import { useCallback, useEffect, useMemo, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import type { ReadingProgressPatch } from "@calibre/shared";
import { apiClient } from "../../lib/api-client";
import { EpubReader } from "./EpubReader";
import { PdfReader } from "./PdfReader";
import type { ReaderProgressUpdate } from "./types";

type ReaderParams = {
  bookId: string;
  format: string;
};

function parseReaderParams(pathname: string): ReaderParams | null {
  const match = pathname.match(/^\/books\/([^/]+)\/read\/([^/?#]+)/);
  if (!match) {
    return null;
  }

  return {
    bookId: decodeURIComponent(match[1]),
    format: decodeURIComponent(match[2]),
  };
}

function clampPercentage(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(100, value));
}

export function ReaderPage() {
  const params = useMemo(() => parseReaderParams(window.location.pathname), []);
  const pendingProgressRef = useRef<ReaderProgressUpdate | null>(null);
  const saveTimerRef = useRef<number | null>(null);

  const bookQuery = useQuery({
    queryKey: ["book", params?.bookId],
    queryFn: () => apiClient.getBook(params?.bookId as string),
    enabled: Boolean(params?.bookId),
  });

  const progressQuery = useQuery({
    queryKey: ["reading-progress", params?.bookId],
    queryFn: () => apiClient.getReadingProgress(params?.bookId as string),
    enabled: Boolean(params?.bookId),
  });

  const flushProgress = useCallback(() => {
    if (!params || !pendingProgressRef.current) {
      return;
    }

    const next = pendingProgressRef.current;
    pendingProgressRef.current = null;

    const payload: ReadingProgressPatch = {
      format: params.format,
      percentage: clampPercentage(next.percentage),
      cfi: next.cfi ?? null,
      page: next.page ?? null,
    };

    void apiClient.patchReadingProgress(params.bookId, payload);
  }, [params]);

  const handleProgressChange = useCallback(
    (progress: ReaderProgressUpdate) => {
      pendingProgressRef.current = progress;

      if (saveTimerRef.current !== null) {
        window.clearTimeout(saveTimerRef.current);
      }

      saveTimerRef.current = window.setTimeout(() => {
        saveTimerRef.current = null;
        flushProgress();
      }, 600);
    },
    [flushProgress],
  );

  useEffect(() => {
    return () => {
      if (saveTimerRef.current !== null) {
        window.clearTimeout(saveTimerRef.current);
      }
      flushProgress();
    };
  }, [flushProgress]);

  if (!params) {
    return (
      <main className="fixed inset-0 z-50 grid place-items-center bg-zinc-950 text-zinc-200">
        Invalid reader URL.
      </main>
    );
  }

  if (bookQuery.isLoading || progressQuery.isLoading) {
    return <main className="fixed inset-0 z-50 grid place-items-center bg-zinc-950 text-zinc-200">Loading reader...</main>;
  }

  if (bookQuery.isError || !bookQuery.data) {
    return (
      <main className="fixed inset-0 z-50 grid place-items-center bg-zinc-950 text-red-300">
        Unable to load reader.
      </main>
    );
  }

  const normalizedFormat = params.format.toLowerCase();

  return (
    <main className="fixed inset-0 z-50 bg-zinc-950 text-zinc-100" data-testid="reader-page">
      {normalizedFormat.includes("epub") ? (
        <EpubReader
          book={bookQuery.data}
          format={params.format}
          initialProgress={progressQuery.data ?? null}
          onProgressChange={handleProgressChange}
        />
      ) : null}

      {normalizedFormat.includes("pdf") ? (
        <PdfReader
          book={bookQuery.data}
          format={params.format}
          initialProgress={progressQuery.data ?? null}
          onProgressChange={handleProgressChange}
        />
      ) : null}

      {!normalizedFormat.includes("epub") && !normalizedFormat.includes("pdf") ? (
        <div className="grid h-full place-items-center text-zinc-300">Unsupported reader format: {params.format}</div>
      ) : null}
    </main>
  );
}
