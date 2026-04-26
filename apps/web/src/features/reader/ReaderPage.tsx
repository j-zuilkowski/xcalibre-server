import { useCallback, useEffect, useMemo, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { ReadingProgressPatch } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { AudioReader } from "./AudioReader";
import { ComicReader } from "./ComicReader";
import { DjvuReader } from "./DjvuReader";
import { EpubReader } from "./EpubReader";
import { PdfReader } from "./PdfReader";
import type { ReaderProgressUpdate } from "./types";

type ReaderParams = {
  bookId: string;
  format: string;
};

const AUDIO_FORMATS = ["mp3", "m4b", "m4a", "ogg", "opus", "flac", "wav", "aac"] as const;

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
  const { t } = useTranslation();
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
  const selectedFormatId = useMemo(() => {
    if (!bookQuery.data || !params) {
      return null;
    }

    const exactMatch = bookQuery.data.formats.find(
      (entry) => entry.format.toLowerCase() === params.format.toLowerCase(),
    );
    return exactMatch?.id ?? bookQuery.data.formats[0]?.id ?? null;
  }, [bookQuery.data, params]);

  const flushProgress = useCallback(() => {
    if (!params || !pendingProgressRef.current) {
      return;
    }

    const next = pendingProgressRef.current;
    pendingProgressRef.current = null;

    if (!selectedFormatId) {
      return;
    }

    const payload: ReadingProgressPatch = {
      format_id: selectedFormatId,
      percentage: clampPercentage(next.percentage),
      cfi: next.cfi ?? null,
      page: next.page ?? null,
    };

    void apiClient.patchReadingProgress(params.bookId, payload);
  }, [params, selectedFormatId]);

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
        {t("reader.invalid_url")}
      </main>
    );
  }

  if (bookQuery.isLoading || progressQuery.isLoading) {
    return <main className="fixed inset-0 z-50 grid place-items-center bg-zinc-950 text-zinc-200">{t("reader.loading_reader")}</main>;
  }

  if (bookQuery.isError || !bookQuery.data) {
    return (
      <main className="fixed inset-0 z-50 grid place-items-center bg-zinc-950 text-red-300">
        {t("reader.unable_to_load")}
      </main>
    );
  }

  const normalizedFormat = params.format.toLowerCase();
  const isComic = normalizedFormat.includes("cbz") || normalizedFormat.includes("cbr");
  const isEpub = normalizedFormat.includes("epub");
  const isPdf = normalizedFormat.includes("pdf");
  const isDjvu = normalizedFormat === "djvu";
  const isMobiFamily = normalizedFormat === "mobi" || normalizedFormat === "azw3";
  const isAudio = AUDIO_FORMATS.includes(normalizedFormat as (typeof AUDIO_FORMATS)[number]);
  const epubStreamUrl = isMobiFamily
    ? `/api/v1/books/${encodeURIComponent(params.bookId)}/formats/${encodeURIComponent(params.format)}/to-epub`
    : undefined;

  return (
    <main className="fixed inset-0 z-50 bg-zinc-950 text-zinc-100" data-testid="reader-page">
      {isEpub || isMobiFamily ? (
        <EpubReader
          book={bookQuery.data}
          format={params.format}
          streamUrl={epubStreamUrl}
          initialProgress={progressQuery.data ?? null}
          onProgressChange={handleProgressChange}
        />
      ) : null}

      {isPdf ? (
        <PdfReader
          book={bookQuery.data}
          format={params.format}
          initialProgress={progressQuery.data ?? null}
          onProgressChange={handleProgressChange}
        />
      ) : null}

      {!isEpub && !isMobiFamily && !isPdf ? (
        isDjvu ? (
          <DjvuReader
            bookId={params.bookId}
            format="djvu"
            initialProgressPage={progressQuery.data?.page ?? null}
            onProgressChange={handleProgressChange}
          />
        ) : isAudio ? (
          <AudioReader
            book={bookQuery.data}
            format={params.format}
            initialProgress={progressQuery.data ?? null}
            onProgressChange={handleProgressChange}
          />
        ) : isComic ? (
          <ComicReader bookId={params.bookId} onProgressChange={handleProgressChange} />
        ) : (
          <div className="grid h-full place-items-center text-zinc-300">
            {t("reader.unsupported_format")}: {params.format}
          </div>
        )
      ) : null}
    </main>
  );
}
