import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAuthStore } from "../../lib/auth-store";
import { apiClient } from "../../lib/api-client";
import { useTranslation } from "react-i18next";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "../../components/ui/sheet";
import { useReaderToolbar } from "./useReaderToolbar";
import type { ReaderComponentProps } from "./types";

type EpubTheme = "light" | "sepia" | "dark";
type EpubFont = "Literata" | "Inter";

type EpubSettings = {
  fontFamily: EpubFont;
  fontSize: number;
  lineHeight: number;
  margin: number;
  theme: EpubTheme;
};

const DEFAULT_SETTINGS: EpubSettings = {
  fontFamily: "Literata",
  fontSize: 18,
  lineHeight: 1.6,
  margin: 20,
  theme: "light",
};

type TocItem = {
  id: string;
  label: string;
  href: string;
};

type EpubRendition = {
  display: (target?: string) => Promise<void>;
  next: () => Promise<void>;
  prev: () => Promise<void>;
  on: (event: string, callback: (payload: any) => void) => void;
  themes?: {
    default?: (styles: Record<string, unknown>) => void;
    fontSize?: (size: string) => void;
  };
  destroy?: () => void;
};

type EpubBook = {
  renderTo: (element: HTMLElement, options?: Record<string, unknown>) => EpubRendition;
  destroy?: () => void;
  loaded?: {
    navigation?: Promise<{
      toc?: Array<{ id?: string; label?: string; href?: string }>;
    }>;
  };
};

function settingsStorageKey(userId: string | null): string {
  return `reader:epub-settings:${userId ?? "anon"}`;
}

function readSettings(userId: string | null): EpubSettings {
  if (typeof localStorage === "undefined") {
    return DEFAULT_SETTINGS;
  }

  if (typeof localStorage.getItem !== "function") {
    return DEFAULT_SETTINGS;
  }

  const raw = localStorage.getItem(settingsStorageKey(userId));
  if (!raw) {
    return DEFAULT_SETTINGS;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<EpubSettings>;
    return {
      fontFamily: parsed.fontFamily === "Inter" ? "Inter" : "Literata",
      fontSize: typeof parsed.fontSize === "number" ? parsed.fontSize : DEFAULT_SETTINGS.fontSize,
      lineHeight:
        typeof parsed.lineHeight === "number" ? parsed.lineHeight : DEFAULT_SETTINGS.lineHeight,
      margin: typeof parsed.margin === "number" ? parsed.margin : DEFAULT_SETTINGS.margin,
      theme:
        parsed.theme === "sepia" || parsed.theme === "dark" || parsed.theme === "light"
          ? parsed.theme
          : DEFAULT_SETTINGS.theme,
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function themeStyles(theme: EpubTheme): { background: string; text: string } {
  if (theme === "sepia") {
    return { background: "#f4ecd8", text: "#3f2f1f" };
  }
  if (theme === "dark") {
    return { background: "#18181b", text: "#f4f4f5" };
  }
  return { background: "#ffffff", text: "#111827" };
}

function clampProgress(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(100, value));
}

export function EpubReader({ book, format, initialProgress, onProgressChange }: ReaderComponentProps) {
  const user = useAuthStore((state) => state.user);
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const renditionRef = useRef<EpubRendition | null>(null);
  const [engineUnavailable, setEngineUnavailable] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [tocOpen, setTocOpen] = useState(false);
  const [progress, setProgress] = useState(initialProgress?.percentage ?? 0);
  const [tocItems, setTocItems] = useState<TocItem[]>([]);
  const { toolbarVisible, showToolbar } = useReaderToolbar();

  const [settings, setSettings] = useState<EpubSettings>(() => readSettings(user?.id ?? null));
  const settingsRef = useRef(settings);

  useEffect(() => {
    setProgress(initialProgress?.percentage ?? 0);
  }, [initialProgress?.percentage]);

  useEffect(() => {
    if (typeof localStorage === "undefined") {
      return;
    }

    if (typeof localStorage.setItem !== "function") {
      return;
    }

    localStorage.setItem(settingsStorageKey(user?.id ?? null), JSON.stringify(settings));
  }, [settings, user?.id]);

  const streamUrl = useMemo(() => apiClient.streamUrl(book.id, format), [book.id, format]);

  const applyReaderTheme = useCallback((rendition: EpubRendition, nextSettings: EpubSettings) => {
    const palette = themeStyles(nextSettings.theme);

    rendition.themes?.default?.({
      body: {
        "font-family": nextSettings.fontFamily === "Inter" ? "Inter, sans-serif" : "Literata, serif",
        "font-size": `${nextSettings.fontSize}px`,
        "line-height": String(nextSettings.lineHeight),
        margin: `${nextSettings.margin}px`,
        color: palette.text,
        "background-color": palette.background,
      },
    });

    rendition.themes?.fontSize?.(`${nextSettings.fontSize}px`);
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function loadEpub() {
      try {
        const module = (await import("epubjs")) as any;
        const createBook = module?.default ?? module;

        if (!createBook || typeof createBook !== "function" || !containerRef.current) {
          setEngineUnavailable(true);
          return;
        }

        const epubBook = createBook(streamUrl) as EpubBook;
        const rendition = epubBook.renderTo(containerRef.current, {
          width: "100%",
          height: "100%",
          flow: "paginated",
          spread: "none",
        });

        if (cancelled) {
          rendition.destroy?.();
          epubBook.destroy?.();
          return;
        }

        renditionRef.current = rendition;
        applyReaderTheme(rendition, settingsRef.current);

        const startCfi = initialProgress?.cfi ?? undefined;
        await rendition.display(startCfi);

        rendition.on("relocated", (location: any) => {
          const nextPercentage = clampProgress(Number((location?.start?.percentage ?? 0) * 100));
          const nextCfi = (location?.start?.cfi as string | undefined) ?? null;
          setProgress(nextPercentage);
          onProgressChange({ percentage: nextPercentage, cfi: nextCfi, page: null });
        });

        try {
          const navigation = await epubBook.loaded?.navigation;
          if (!cancelled) {
            const nextToc = (navigation?.toc ?? []).map((entry, index) => ({
              id: entry.id ?? `${index}`,
              label: entry.label ?? `Chapter ${index + 1}`,
              href: entry.href ?? "",
            }));
            setTocItems(nextToc);
          }
        } catch {
          if (!cancelled) {
            setTocItems([]);
          }
        }
      } catch {
        if (!cancelled) {
          setEngineUnavailable(true);
        }
      }
    }

    void loadEpub();

    return () => {
      cancelled = true;
      renditionRef.current?.destroy?.();
      renditionRef.current = null;
    };
  }, [applyReaderTheme, initialProgress?.cfi, onProgressChange, streamUrl]);

  useEffect(() => {
    if (!renditionRef.current) {
      return;
    }

    applyReaderTheme(renditionRef.current, settings);
  }, [applyReaderTheme, settings]);

  const goNext = useCallback(() => {
    if (renditionRef.current) {
      void renditionRef.current.next();
      return;
    }

    setProgress((previous) => {
      const next = clampProgress(previous + 5);
      onProgressChange({ percentage: next, cfi: null, page: null });
      return next;
    });
  }, [onProgressChange]);

  const goPrevious = useCallback(() => {
    if (renditionRef.current) {
      void renditionRef.current.prev();
      return;
    }

    setProgress((previous) => {
      const next = clampProgress(previous - 5);
      onProgressChange({ percentage: next, cfi: null, page: null });
      return next;
    });
  }, [onProgressChange]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "ArrowRight") {
        event.preventDefault();
        goNext();
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        goPrevious();
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [goNext, goPrevious]);

  const progressLabel = `${Math.round(progress)}%`;

  return (
    <div
      data-testid="epub-reader"
      className="relative h-full w-full overflow-hidden bg-zinc-950"
      onMouseMove={showToolbar}
      onPointerMove={showToolbar}
    >
      <div ref={containerRef} className="h-full w-full" />

      {engineUnavailable ? (
        <div className="pointer-events-none absolute inset-0 grid place-items-center text-sm text-zinc-400">
          {t("reader.epub_rendering_unavailable")}
        </div>
      ) : null}

      <header
        data-testid="reader-toolbar"
        data-visible={toolbarVisible ? "true" : "false"}
        className={`absolute left-0 right-0 top-0 z-20 border-b border-zinc-800 bg-zinc-950/90 px-4 py-3 transition-opacity duration-300 ${
          toolbarVisible ? "opacity-100" : "pointer-events-none opacity-0"
        }`}
      >
        <div className="flex items-center justify-between gap-3">
          <a href={`/books/${encodeURIComponent(book.id)}`} className="text-sm font-medium text-zinc-100">
            ←
          </a>
          <div className="min-w-0 flex-1 truncate text-center text-sm text-zinc-300">
            {book.title} · {book.authors.map((author) => author.name).join(", ") || t("common.unknown_author")}
          </div>
          <div className="flex items-center gap-2">
            <button type="button" aria-label={t("reader.open_settings")} onClick={() => setSettingsOpen(true)} className="rounded border border-zinc-700 px-2 py-1 text-xs">
              ⚙
            </button>
            <button
              type="button"
              aria-label={t("reader.open_table_of_contents")}
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
          {progressLabel}
        </p>
      </div>

      <Sheet open={settingsOpen} onOpenChange={setSettingsOpen}>
        <SheetContent side="right">
          <SheetHeader>
            <SheetTitle>{t("reader.reader_settings")}</SheetTitle>
          </SheetHeader>

          <div className="space-y-6 p-5 text-sm">
            <div>
              <p className="mb-2 font-medium">{t("reader.font")}</p>
              <label className="mr-4 inline-flex items-center gap-2">
                <input
                  type="radio"
                  name="epub-font"
                  checked={settings.fontFamily === "Literata"}
                  onChange={() => setSettings((previous) => ({ ...previous, fontFamily: "Literata" }))}
                />
                Literata
              </label>
              <label className="inline-flex items-center gap-2">
                <input
                  type="radio"
                  name="epub-font"
                  checked={settings.fontFamily === "Inter"}
                  onChange={() => setSettings((previous) => ({ ...previous, fontFamily: "Inter" }))}
                />
                Inter
              </label>
            </div>

            <div>
              <label className="mb-2 block font-medium">{t("reader.font_size", { size: settings.fontSize })}</label>
              <input
                type="range"
                min={14}
                max={24}
                value={settings.fontSize}
                onChange={(event) =>
                  setSettings((previous) => ({ ...previous, fontSize: Number(event.target.value) }))
                }
              />
            </div>

            <div>
              <label className="mb-2 block font-medium">{t("reader.line_height", { value: settings.lineHeight.toFixed(1) })}</label>
              <input
                type="range"
                min={1.2}
                max={2.4}
                step={0.1}
                value={settings.lineHeight}
                onChange={(event) =>
                  setSettings((previous) => ({ ...previous, lineHeight: Number(event.target.value) }))
                }
              />
            </div>

            <div>
              <label className="mb-2 block font-medium">{t("reader.margin", { size: settings.margin })}</label>
              <input
                type="range"
                min={0}
                max={40}
                value={settings.margin}
                onChange={(event) =>
                  setSettings((previous) => ({ ...previous, margin: Number(event.target.value) }))
                }
              />
            </div>

            <div>
              <p className="mb-2 font-medium">{t("reader.theme")}</p>
              <div className="flex gap-2">
                {(["light", "sepia", "dark"] as const).map((theme) => (
                  <button
                    key={theme}
                    type="button"
                    onClick={() => setSettings((previous) => ({ ...previous, theme }))}
                    className={`rounded border px-3 py-1 text-xs uppercase ${
                      settings.theme === theme
                        ? "border-teal-500 bg-teal-500/20 text-teal-200"
                        : "border-zinc-700 text-zinc-300"
                    }`}
                  >
                    {t(`reader.${theme}`)}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </SheetContent>
      </Sheet>

      <Sheet open={tocOpen} onOpenChange={setTocOpen}>
        <SheetContent side="left">
          <SheetHeader>
            <SheetTitle>{t("reader.table_of_contents")}</SheetTitle>
          </SheetHeader>

          <div className="p-5 text-sm">
            {tocItems.length > 0 ? (
              <ul className="space-y-2">
                {tocItems.map((item) => (
                  <li key={item.id}>
                    <button
                      type="button"
                      className="text-left text-zinc-200 hover:text-white"
                      onClick={() => {
                        if (item.href && renditionRef.current) {
                          void renditionRef.current.display(item.href);
                        }
                        setTocOpen(false);
                      }}
                    >
                      {item.label}
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-zinc-400">{t("reader.no_table_of_contents")}</p>
            )}
          </div>
        </SheetContent>
      </Sheet>
    </div>
  );
}
