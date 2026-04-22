import { useCallback, useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { apiClient } from "../../lib/api-client";
import type { ReaderComponentProps } from "./types";

function clampPercentage(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(100, value));
}

export function AudioReader({ book, format, initialProgress, onProgressChange }: ReaderComponentProps) {
  const { t } = useTranslation();
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const restoredRef = useRef(false);
  const streamUrl = useMemo(() => apiClient.streamUrl(book.id, format), [book.id, format]);

  const restorePosition = useCallback(() => {
    const audio = audioRef.current;
    if (!audio || restoredRef.current) {
      return;
    }

    const initialSeconds =
      typeof initialProgress?.page === "number" && Number.isFinite(initialProgress.page) && initialProgress.page >= 0
        ? initialProgress.page
        : null;

    if (initialSeconds !== null) {
      const maxDuration = Number.isFinite(audio.duration) && audio.duration > 0 ? audio.duration : initialSeconds;
      audio.currentTime = Math.min(initialSeconds, maxDuration);
    }

    restoredRef.current = true;
  }, [initialProgress?.page]);

  useEffect(() => {
    restoredRef.current = false;
    restorePosition();
  }, [book.id, format, initialProgress?.page, restorePosition]);

  const handleTimeUpdate = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) {
      return;
    }

    const percentage =
      Number.isFinite(audio.duration) && audio.duration > 0
        ? clampPercentage((audio.currentTime / audio.duration) * 100)
        : 0;

    onProgressChange({
      percentage,
      cfi: null,
      page: Math.floor(audio.currentTime),
    });
  }, [onProgressChange]);

  useEffect(() => {
    const id = window.setInterval(() => {
      const audio = audioRef.current;
      if (!audio || audio.paused || !Number.isFinite(audio.duration) || audio.duration <= 0) {
        return;
      }
      const percentage = clampPercentage((audio.currentTime / audio.duration) * 100);
      onProgressChange({
        percentage,
        cfi: null,
        page: Math.floor(audio.currentTime),
      });
    }, 30_000);
    return () => window.clearInterval(id);
  }, [onProgressChange]);

  return (
    <section className="grid h-full w-full place-items-center bg-zinc-950 p-4 text-zinc-100" data-testid="audio-reader">
      <div className="w-full max-w-xl rounded-xl border border-zinc-800 bg-zinc-900/70 p-5 shadow-xl">
        <div className="mb-4 flex items-center gap-4">
          {book.has_cover ? (
            <img
              src={apiClient.coverUrl(book.id)}
              alt={`${book.title} cover`}
              className="h-24 w-16 rounded object-cover"
            />
          ) : (
            <div className="grid h-24 w-16 place-items-center rounded bg-zinc-800 text-xs text-zinc-400">No Cover</div>
          )}

          <div className="min-w-0">
            <h2 className="truncate text-lg font-semibold">{book.title}</h2>
            <p className="truncate text-sm text-zinc-300">
              {book.authors.map((author) => author.name).join(", ") || t("common.unknown_author")}
            </p>
          </div>
        </div>

        <audio
          ref={audioRef}
          controls
          preload="metadata"
          src={streamUrl}
          onLoadedMetadata={restorePosition}
          onTimeUpdate={handleTimeUpdate}
          className="w-full"
        />
      </div>
    </section>
  );
}
