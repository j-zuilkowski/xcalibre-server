/**
 * EpubReader — epub.js-based reading component.
 *
 * Loaded via dynamic import (`import("epubjs")`) so the ~400 kB bundle is
 * only fetched when the reader is actually opened.  If the module fails to
 * load, `engineUnavailable` is set and a fallback message is shown.
 *
 * CFI-based progress:
 *   epub.js emits a "relocated" event with `location.start.percentage` and
 *   `location.start.cfi` on every page turn.  The CFI is persisted to the
 *   server via `onProgressChange` (debounced by the parent ReaderPage).
 *   On mount, `initialProgress.cfi` is passed to `rendition.display()` so the
 *   reader resumes at the exact character position.
 *
 * Annotation lifecycle:
 *   1. On mount, annotations are loaded from GET /api/v1/books/:id/annotations
 *      and each non-bookmark annotation is rendered via
 *      `rendition.annotations.add("highlight", cfiRange, …, className)`.
 *   2. Text selection fires epub.js "selected" event → `resolveSelectionMenuAnchor`
 *      computes overlay coordinates relative to the iframe, then a floating
 *      colour picker / note input appears (SelectionMenuState).
 *   3. Clicking an existing highlight fires "markClicked" → an AnnotationMenuState
 *      is shown allowing colour change, note edit, or delete.
 *   4. All mutations call the API and then update local React state via
 *      `upsertAnnotation` / `removeAnnotation` so the list stays consistent
 *      without a full refetch.
 *
 * TOC panel (Sheet, left side):
 *   - Chapters tab: epub nav loaded from `book.loaded.navigation`.
 *   - Annotations tab: annotations grouped and sorted by chapter using
 *     `chapterLabelForAnnotation`, which extracts the chapter token from the
 *     CFI bracket notation (e.g. `[chap01ref]`).
 *
 * Settings (Sheet, right side): font family, font size (14–24 px), line
 * height (1.2–2.4), margin (0–40 px), and theme.  Settings are persisted
 * per-user to localStorage and applied to the epub.js rendition immediately.
 *
 * Keyboard shortcuts: ← / → for page navigation, Esc to close panels or exit
 * the reader, ? to toggle the help overlay.  The handler skips editable
 * targets (input / textarea / contenteditable) so note fields still work.
 *
 * API calls:
 *   GET    /api/v1/books/:id/annotations
 *   POST   /api/v1/books/:id/annotations
 *   PATCH  /api/v1/books/:id/annotations/:annotationId
 *   DELETE /api/v1/books/:id/annotations/:annotationId
 *   GET    /api/v1/books/:id/formats/:format/stream  (epub binary)
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AnnotationColor, BookAnnotation } from "@xs/shared";
import { useAuthStore } from "../../lib/auth-store";
import { apiClient } from "../../lib/api-client";
import { useTranslation } from "react-i18next";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "../../components/ui/Sheet";
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

const ANNOTATION_COLORS: AnnotationColor[] = ["yellow", "green", "blue", "pink"];

type TocItem = {
  id: string;
  label: string;
  href: string;
};

type SelectionMenuState = {
  cfiRange: string;
  highlightedText: string;
  x: number;
  y: number;
  noteOpen: boolean;
  noteText: string;
};

type AnnotationMenuState = {
  annotationId: string;
  x: number;
  y: number;
  editingNote: boolean;
  noteDraft: string;
};

type EpubRendition = {
  display: (target?: string) => Promise<void>;
  next: () => Promise<void>;
  prev: () => Promise<void>;
  on: (event: string, callback: (...args: any[]) => void) => void;
  annotations?: {
    add?: (
      type: string,
      cfiRange: string,
      data?: Record<string, unknown>,
      callback?: ((...args: any[]) => void) | null,
      className?: string,
    ) => void;
    remove?: (cfiRange: string, type?: string) => void;
  };
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
    return { background: "#fdf6e3", text: "#3f2f1f" };
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

function clampOverlayCoordinates(x: number, y: number, container: HTMLDivElement | null): { x: number; y: number } {
  if (!container) {
    return { x, y };
  }

  const rect = container.getBoundingClientRect();
  return {
    x: Math.max(32, Math.min(rect.width - 32, x)),
    y: Math.max(24, Math.min(rect.height - 24, y)),
  };
}

function extractChapterToken(cfiRange: string): string | null {
  const match = cfiRange.match(/\[([^\]]+)\]/);
  if (!match || !match[1]) {
    return null;
  }
  return match[1].toLowerCase();
}

function chapterLabelForAnnotation(cfiRange: string, tocItems: TocItem[]): string {
  const token = extractChapterToken(cfiRange);
  if (!token) {
    return "Other";
  }

  const match = tocItems.find((item) => {
    const itemId = item.id.toLowerCase();
    const href = item.href.toLowerCase();
    return itemId.includes(token) || href.includes(token);
  });

  return match?.label ?? "Other";
}

function sortAnnotations(annotations: BookAnnotation[]): BookAnnotation[] {
  return [...annotations].sort((left, right) => {
    const cfiSort = left.cfi_range.localeCompare(right.cfi_range);
    if (cfiSort !== 0) {
      return cfiSort;
    }
    return left.created_at.localeCompare(right.created_at);
  });
}

function resolveSelectionMenuAnchor(
  container: HTMLDivElement | null,
  contents: any,
): { x: number; y: number } {
  if (!container) {
    return { x: 200, y: 80 };
  }

  const iframeRect = container.querySelector("iframe")?.getBoundingClientRect();
  const containerRect = container.getBoundingClientRect();

  const selection = contents?.window?.getSelection?.();
  if (selection && selection.rangeCount > 0) {
    const range = selection.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    if (Number.isFinite(rect.left) && Number.isFinite(rect.top)) {
      const x = (iframeRect?.left ?? containerRect.left) + rect.left + rect.width / 2 - containerRect.left;
      const y = (iframeRect?.top ?? containerRect.top) + rect.top - containerRect.top - 12;
      return clampOverlayCoordinates(x, y, container);
    }
  }

  return { x: containerRect.width / 2, y: 72 };
}

function resolveTooltipAnchor(
  container: HTMLDivElement | null,
  pointerEvent: any,
): { x: number; y: number } {
  if (!container) {
    return { x: 220, y: 88 };
  }

  const containerRect = container.getBoundingClientRect();
  if (typeof pointerEvent?.clientX === "number" && typeof pointerEvent?.clientY === "number") {
    return clampOverlayCoordinates(
      pointerEvent.clientX - containerRect.left,
      pointerEvent.clientY - containerRect.top - 8,
      container,
    );
  }

  return { x: containerRect.width / 2, y: 84 };
}

function annotationPreview(annotation: BookAnnotation): string {
  if (annotation.type === "bookmark") {
    return "Bookmark";
  }

  if (annotation.note && annotation.note.trim().length > 0) {
    return annotation.note;
  }

  if (annotation.highlighted_text && annotation.highlighted_text.trim().length > 0) {
    return annotation.highlighted_text;
  }

  return annotation.cfi_range;
}

/**
 * EpubReader renders a paginated epub.js reading surface with annotation
 * support, a floating toolbar, and reader settings.
 *
 * @param book           - Book metadata (id, title, authors).
 * @param format         - Format string (e.g. "epub"), used for the stream URL.
 * @param streamUrl      - Optional override stream URL; defaults to
 *                         `/api/v1/books/:id/formats/:format/stream`.
 * @param initialProgress - CFI + percentage from last saved session.
 * @param onProgressChange - Callback invoked on every epub.js "relocated"
 *                          event; parent is responsible for debouncing and
 *                          persisting to the server.
 */
export function EpubReader({
  book,
  format,
  streamUrl,
  initialProgress,
  onProgressChange,
}: ReaderComponentProps) {
  const user = useAuthStore((state) => state.user);
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const toolbarRef = useRef<HTMLElement | null>(null);
  const renditionRef = useRef<EpubRendition | null>(null);
  const annotationsRef = useRef<BookAnnotation[]>([]);
  const [engineUnavailable, setEngineUnavailable] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [tocOpen, setTocOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [tocTab, setTocTab] = useState<"chapters" | "annotations">("chapters");
  const [progress, setProgress] = useState(initialProgress?.percentage ?? 0);
  const [tocItems, setTocItems] = useState<TocItem[]>([]);
  const [annotations, setAnnotations] = useState<BookAnnotation[]>([]);
  const [selectionMenu, setSelectionMenu] = useState<SelectionMenuState | null>(null);
  const [annotationMenu, setAnnotationMenu] = useState<AnnotationMenuState | null>(null);
  const [annotationMutationPending, setAnnotationMutationPending] = useState(false);
  const { toolbarVisible, showToolbar } = useReaderToolbar();

  const [settings, setSettings] = useState<EpubSettings>(() => readSettings(user?.id ?? null));
  const settingsRef = useRef(settings);

  // Keep a ref in sync so the epub.js "markClicked" event handler can read
  // the latest annotations without closing over a stale closure.
  useEffect(() => {
    annotationsRef.current = annotations;
  }, [annotations]);

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

  const resolvedStreamUrl = useMemo(
    () => streamUrl ?? apiClient.streamUrl(book.id, format),
    [book.id, format, streamUrl],
  );

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
      ".annotation-yellow": {
        background: "rgba(255, 235, 59, 0.4)",
      },
      ".annotation-green": {
        background: "rgba(76, 175, 80, 0.3)",
      },
      ".annotation-blue": {
        background: "rgba(33, 150, 243, 0.3)",
      },
      ".annotation-pink": {
        background: "rgba(233, 30, 99, 0.3)",
      },
    });

    rendition.themes?.fontSize?.(`${nextSettings.fontSize}px`);
  }, []);

  const renderAnnotationHighlight = useCallback((rendition: EpubRendition, annotation: BookAnnotation) => {
    if (annotation.type === "bookmark") {
      return;
    }

    rendition.annotations?.add?.(
      "highlight",
      annotation.cfi_range,
      {
        id: annotation.id,
        color: annotation.color,
        note: annotation.note,
        type: annotation.type,
      },
      null,
      `annotation-${annotation.color}`,
    );
  }, []);

  const removeAnnotationHighlight = useCallback((rendition: EpubRendition, annotation: BookAnnotation) => {
    if (annotation.type === "bookmark") {
      return;
    }

    rendition.annotations?.remove?.(annotation.cfi_range, "highlight");
  }, []);

  const upsertAnnotation = useCallback((annotation: BookAnnotation) => {
    setAnnotations((previous) => {
      const next = previous.some((entry) => entry.id === annotation.id)
        ? previous.map((entry) => (entry.id === annotation.id ? annotation : entry))
        : [...previous, annotation];
      return sortAnnotations(next);
    });
  }, []);

  const removeAnnotation = useCallback((annotationId: string) => {
    setAnnotations((previous) => previous.filter((entry) => entry.id !== annotationId));
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

        const epubBook = createBook(resolvedStreamUrl) as EpubBook;
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

        rendition.on("relocated", (location: any) => {
          const nextPercentage = clampProgress(Number((location?.start?.percentage ?? 0) * 100));
          const nextCfi = (location?.start?.cfi as string | undefined) ?? null;
          setProgress(nextPercentage);
          onProgressChange({ percentage: nextPercentage, cfi: nextCfi, page: null });
        });

        rendition.on("selected", (cfiRange: string, contents: any) => {
          if (typeof cfiRange !== "string" || cfiRange.trim().length === 0) {
            return;
          }

          const selectedText = String(contents?.window?.getSelection?.()?.toString?.() ?? "").trim();
          if (!selectedText) {
            return;
          }

          const anchor = resolveSelectionMenuAnchor(containerRef.current, contents);
          setAnnotationMenu(null);
          setSelectionMenu({
            cfiRange,
            highlightedText: selectedText,
            x: anchor.x,
            y: anchor.y,
            noteOpen: false,
            noteText: "",
          });
        });

        rendition.on("markClicked", (cfiRange: string, data: any, _contents: any, pointerEvent: any) => {
          const annotationId = typeof data?.id === "string" ? data.id : null;
          const targetAnnotation = annotationId
            ? annotationsRef.current.find((entry) => entry.id === annotationId)
            : annotationsRef.current.find((entry) => entry.cfi_range === cfiRange);

          if (!targetAnnotation) {
            return;
          }

          const anchor = resolveTooltipAnchor(containerRef.current, pointerEvent);
          setSelectionMenu(null);
          setAnnotationMenu({
            annotationId: targetAnnotation.id,
            x: anchor.x,
            y: anchor.y,
            editingNote: false,
            noteDraft: targetAnnotation.note ?? "",
          });
        });

        const startCfi = initialProgress?.cfi ?? undefined;
        await rendition.display(startCfi);

        let nextToc: TocItem[] = [];
        try {
          const navigation = await epubBook.loaded?.navigation;
          nextToc = (navigation?.toc ?? []).map((entry, index) => ({
            id: entry.id ?? `${index}`,
            label: entry.label ?? `Chapter ${index + 1}`,
            href: entry.href ?? "",
          }));
        } catch {
          nextToc = [];
        }

        let nextAnnotations: BookAnnotation[] = [];
        try {
          nextAnnotations = sortAnnotations(await apiClient.listBookAnnotations(book.id));
        } catch {
          nextAnnotations = [];
        }

        if (!cancelled) {
          setTocItems(nextToc);
          setAnnotations(nextAnnotations);
          for (const annotation of nextAnnotations) {
            renderAnnotationHighlight(rendition, annotation);
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
  }, [
    applyReaderTheme,
    book.id,
    initialProgress?.cfi,
    onProgressChange,
    renderAnnotationHighlight,
    resolvedStreamUrl,
  ]);

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

  function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) {
      return false;
    }

    return (
      target.isContentEditable ||
      target.tagName === "INPUT" ||
      target.tagName === "TEXTAREA" ||
      target.tagName === "SELECT" ||
      Boolean(target.closest("[contenteditable='true']"))
    );
  }

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "?") {
        if (!isEditableTarget(event.target)) {
          event.preventDefault();
          setHelpOpen((current) => !current);
        }
        return;
      }

      if (event.key === "Escape") {
        if (helpOpen) {
          setHelpOpen(false);
          return;
        }
        if (settingsOpen) {
          setSettingsOpen(false);
          return;
        }
        if (tocOpen) {
          setTocOpen(false);
          return;
        }
        window.location.assign(`/books/${encodeURIComponent(book.id)}`);
        return;
      }

      if (settingsOpen || tocOpen) {
        return;
      }

      if (toolbarRef.current?.contains(document.activeElement)) {
        return;
      }

      if (isEditableTarget(event.target)) {
        return;
      }

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
  }, [book.id, goNext, goPrevious, helpOpen, settingsOpen, tocOpen]);

  const annotationsByChapter = useMemo(() => {
    const chapterOrder = new Map<string, number>();
    tocItems.forEach((item, index) => {
      chapterOrder.set(item.label, index);
    });

    const grouped = new Map<string, BookAnnotation[]>();
    for (const annotation of annotations) {
      const chapter = chapterLabelForAnnotation(annotation.cfi_range, tocItems);
      const existing = grouped.get(chapter) ?? [];
      existing.push(annotation);
      grouped.set(chapter, existing);
    }

    return Array.from(grouped.entries())
      .sort(([left], [right]) => {
        const leftOrder = chapterOrder.get(left);
        const rightOrder = chapterOrder.get(right);
        if (leftOrder !== undefined && rightOrder !== undefined) {
          return leftOrder - rightOrder;
        }
        if (leftOrder !== undefined) {
          return -1;
        }
        if (rightOrder !== undefined) {
          return 1;
        }
        return left.localeCompare(right);
      })
      .map(([chapter, items]) => ({
        chapter,
        items: sortAnnotations(items),
      }));
  }, [annotations, tocItems]);

  const selectedAnnotation = useMemo(() => {
    if (!annotationMenu) {
      return null;
    }

    return annotations.find((entry) => entry.id === annotationMenu.annotationId) ?? null;
  }, [annotationMenu, annotations]);

  const createAnnotationFromSelection = useCallback(
    async (payload: {
      type: "highlight" | "note" | "bookmark";
      cfi_range: string;
      highlighted_text?: string | null;
      note?: string | null;
      color?: AnnotationColor;
    }) => {
      setAnnotationMutationPending(true);
      try {
        const created = await apiClient.createBookAnnotation(book.id, payload);
        if (renditionRef.current) {
          renderAnnotationHighlight(renditionRef.current, created);
        }
        upsertAnnotation(created);
      } finally {
        setAnnotationMutationPending(false);
      }
    },
    [book.id, renderAnnotationHighlight, upsertAnnotation],
  );

  const handleSelectionColorClick = useCallback(
    async (color: AnnotationColor) => {
      if (!selectionMenu) {
        return;
      }

      await createAnnotationFromSelection({
        type: "highlight",
        cfi_range: selectionMenu.cfiRange,
        highlighted_text: selectionMenu.highlightedText,
        note: null,
        color,
      });
      setSelectionMenu(null);
    },
    [createAnnotationFromSelection, selectionMenu],
  );

  const handleSelectionNoteSubmit = useCallback(async () => {
    if (!selectionMenu) {
      return;
    }

    const note = selectionMenu.noteText.trim();
    if (!note) {
      return;
    }

    await createAnnotationFromSelection({
      type: "note",
      cfi_range: selectionMenu.cfiRange,
      highlighted_text: selectionMenu.highlightedText,
      note,
      color: "yellow",
    });
    setSelectionMenu(null);
  }, [createAnnotationFromSelection, selectionMenu]);

  const handleSelectionBookmark = useCallback(async () => {
    if (!selectionMenu) {
      return;
    }

    await createAnnotationFromSelection({
      type: "bookmark",
      cfi_range: selectionMenu.cfiRange,
      highlighted_text: null,
      note: null,
      color: "yellow",
    });
    setSelectionMenu(null);
  }, [createAnnotationFromSelection, selectionMenu]);

  const handleAnnotationColorPatch = useCallback(
    async (annotation: BookAnnotation, color: AnnotationColor) => {
      setAnnotationMutationPending(true);
      try {
        const updated = await apiClient.patchBookAnnotation(book.id, annotation.id, { color });
        if (renditionRef.current) {
          removeAnnotationHighlight(renditionRef.current, annotation);
          renderAnnotationHighlight(renditionRef.current, updated);
        }
        upsertAnnotation(updated);
      } finally {
        setAnnotationMutationPending(false);
      }
    },
    [book.id, removeAnnotationHighlight, renderAnnotationHighlight, upsertAnnotation],
  );

  const handleAnnotationNotePatch = useCallback(
    async (annotation: BookAnnotation, note: string) => {
      setAnnotationMutationPending(true);
      try {
        const updated = await apiClient.patchBookAnnotation(book.id, annotation.id, {
          note: note.trim() || null,
        });
        if (renditionRef.current) {
          removeAnnotationHighlight(renditionRef.current, annotation);
          renderAnnotationHighlight(renditionRef.current, updated);
        }
        upsertAnnotation(updated);
        setAnnotationMenu((previous) => {
          if (!previous) {
            return previous;
          }
          return {
            ...previous,
            editingNote: false,
            noteDraft: updated.note ?? "",
          };
        });
      } finally {
        setAnnotationMutationPending(false);
      }
    },
    [book.id, removeAnnotationHighlight, renderAnnotationHighlight, upsertAnnotation],
  );

  const handleAnnotationDelete = useCallback(
    async (annotation: BookAnnotation) => {
      setAnnotationMutationPending(true);
      try {
        await apiClient.deleteBookAnnotation(book.id, annotation.id);
        if (renditionRef.current) {
          removeAnnotationHighlight(renditionRef.current, annotation);
        }
        removeAnnotation(annotation.id);
        setAnnotationMenu(null);
      } finally {
        setAnnotationMutationPending(false);
      }
    },
    [book.id, removeAnnotation, removeAnnotationHighlight],
  );

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

      {selectionMenu ? (
        <div
          className="absolute z-30 min-w-[240px] rounded border border-zinc-700 bg-zinc-900/95 p-3 text-xs text-zinc-100 shadow-xl"
          style={{
            left: selectionMenu.x,
            top: selectionMenu.y,
            transform: "translate(-50%, -100%)",
          }}
        >
          <div className="mb-2 flex items-center gap-2">
            {ANNOTATION_COLORS.map((color) => (
              <button
                key={color}
                type="button"
                aria-label={`Create ${color} highlight`}
                disabled={annotationMutationPending}
                onClick={() => void handleSelectionColorClick(color)}
                className="h-5 w-5 rounded-full border border-zinc-300/60"
                style={{
                  backgroundColor:
                    color === "yellow"
                      ? "rgba(255, 235, 59, 0.85)"
                      : color === "green"
                        ? "rgba(76, 175, 80, 0.8)"
                        : color === "blue"
                          ? "rgba(33, 150, 243, 0.8)"
                          : "rgba(233, 30, 99, 0.8)",
                }}
              />
            ))}
            <button
              type="button"
              disabled={annotationMutationPending}
              className="rounded border border-zinc-600 px-2 py-1 text-zinc-200"
              onClick={() =>
                setSelectionMenu((previous) =>
                  previous
                    ? {
                        ...previous,
                        noteOpen: !previous.noteOpen,
                      }
                    : previous,
                )
              }
            >
              Note
            </button>
            <button
              type="button"
              disabled={annotationMutationPending}
              className="rounded border border-zinc-600 px-2 py-1 text-zinc-200"
              onClick={() => void handleSelectionBookmark()}
            >
              Bookmark
            </button>
            <button
              type="button"
              className="rounded border border-zinc-700 px-2 py-1 text-zinc-300"
              onClick={() => setSelectionMenu(null)}
            >
              X
            </button>
          </div>

          {selectionMenu.noteOpen ? (
            <form
              className="space-y-2"
              onSubmit={(event) => {
                event.preventDefault();
                void handleSelectionNoteSubmit();
              }}
            >
              <input
                value={selectionMenu.noteText}
                onChange={(event) =>
                  setSelectionMenu((previous) =>
                    previous
                      ? {
                          ...previous,
                          noteText: event.target.value,
                        }
                      : previous,
                  )
                }
                className="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs text-zinc-100"
                placeholder="Add a note"
                maxLength={500}
              />
              <button
                type="submit"
                disabled={annotationMutationPending || selectionMenu.noteText.trim().length === 0}
                className="rounded border border-zinc-500 px-2 py-1 text-zinc-100"
              >
                Save note
              </button>
            </form>
          ) : null}
        </div>
      ) : null}

      {annotationMenu && selectedAnnotation ? (
        <div
          className="absolute z-30 w-[260px] rounded border border-zinc-700 bg-zinc-900/95 p-3 text-xs text-zinc-100 shadow-xl"
          style={{
            left: annotationMenu.x,
            top: annotationMenu.y,
            transform: "translate(-50%, -100%)",
          }}
        >
          <div className="mb-2 flex items-center justify-between">
            <p className="font-semibold uppercase tracking-wide text-zinc-300">{selectedAnnotation.type}</p>
            <button
              type="button"
              className="rounded border border-zinc-700 px-2 py-1 text-zinc-300"
              onClick={() => setAnnotationMenu(null)}
            >
              X
            </button>
          </div>

          {selectedAnnotation.note ? (
            <p className="mb-2 rounded bg-zinc-800 px-2 py-1 text-zinc-200">{selectedAnnotation.note}</p>
          ) : null}

          <div className="mb-2 flex items-center gap-2">
            {ANNOTATION_COLORS.map((color) => (
              <button
                key={color}
                type="button"
                aria-label={`Set ${color} annotation color`}
                disabled={annotationMutationPending}
                onClick={() => void handleAnnotationColorPatch(selectedAnnotation, color)}
                className={`h-5 w-5 rounded-full border ${
                  selectedAnnotation.color === color ? "border-white" : "border-zinc-500"
                }`}
                style={{
                  backgroundColor:
                    color === "yellow"
                      ? "rgba(255, 235, 59, 0.85)"
                      : color === "green"
                        ? "rgba(76, 175, 80, 0.8)"
                        : color === "blue"
                          ? "rgba(33, 150, 243, 0.8)"
                          : "rgba(233, 30, 99, 0.8)",
                }}
              />
            ))}
          </div>

          {selectedAnnotation.type === "note" ? (
            annotationMenu.editingNote ? (
              <form
                className="mb-2 space-y-2"
                onSubmit={(event) => {
                  event.preventDefault();
                  void handleAnnotationNotePatch(selectedAnnotation, annotationMenu.noteDraft);
                }}
              >
                <input
                  value={annotationMenu.noteDraft}
                  maxLength={500}
                  onChange={(event) =>
                    setAnnotationMenu((previous) =>
                      previous
                        ? {
                            ...previous,
                            noteDraft: event.target.value,
                          }
                        : previous,
                    )
                  }
                  className="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs text-zinc-100"
                  placeholder="Edit note"
                />
                <div className="flex gap-2">
                  <button
                    type="submit"
                    disabled={annotationMutationPending}
                    className="rounded border border-zinc-500 px-2 py-1 text-zinc-100"
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    className="rounded border border-zinc-700 px-2 py-1 text-zinc-300"
                    onClick={() =>
                      setAnnotationMenu((previous) =>
                        previous
                          ? {
                              ...previous,
                              editingNote: false,
                              noteDraft: selectedAnnotation.note ?? "",
                            }
                          : previous,
                      )
                    }
                  >
                    Cancel
                  </button>
                </div>
              </form>
            ) : (
              <button
                type="button"
                disabled={annotationMutationPending}
                className="mb-2 rounded border border-zinc-600 px-2 py-1 text-zinc-200"
                onClick={() =>
                  setAnnotationMenu((previous) =>
                    previous
                      ? {
                          ...previous,
                          editingNote: true,
                        }
                      : previous,
                  )
                }
              >
                Edit note
              </button>
            )
          ) : null}

          <button
            type="button"
            disabled={annotationMutationPending}
            className="rounded border border-red-500/70 px-2 py-1 text-red-300"
            onClick={() => void handleAnnotationDelete(selectedAnnotation)}
          >
            Delete
          </button>
        </div>
      ) : null}

      <header
        ref={toolbarRef}
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
        <progress
          className="h-1 w-full overflow-hidden rounded bg-zinc-700 [&::-webkit-progress-bar]:bg-zinc-700 [&::-webkit-progress-value]:bg-teal-500 [&::-moz-progress-bar]:bg-teal-500"
          value={progress}
          max={100}
          aria-label={`Reading progress: ${progressLabel}`}
        />
        <p data-testid="reader-progress-label" className="mt-1 text-right text-xs text-zinc-300">
          {progressLabel}
        </p>
      </div>

      <section
        aria-hidden={!helpOpen}
        className={`absolute bottom-16 right-4 z-30 max-w-xs rounded-xl border border-zinc-700 bg-zinc-950/95 p-4 text-xs text-zinc-200 shadow-2xl ${
          helpOpen ? "not-sr-only" : "sr-only"
        }`}
      >
        <p className="font-semibold text-zinc-100">Keyboard shortcuts</p>
        <ul className="mt-2 space-y-1 text-zinc-300">
          <li>Left / Right: change page</li>
          <li>Esc: close panels or exit reader</li>
          <li>? : toggle this help</li>
        </ul>
      </section>

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

          <div className="space-y-4 p-5 text-sm">
            <div className="inline-flex rounded border border-zinc-700 p-1">
              <button
                type="button"
                className={`rounded px-3 py-1 text-xs ${
                  tocTab === "chapters" ? "bg-zinc-700 text-white" : "text-zinc-300"
                }`}
                onClick={() => setTocTab("chapters")}
              >
                Chapters
              </button>
              <button
                type="button"
                className={`rounded px-3 py-1 text-xs ${
                  tocTab === "annotations" ? "bg-zinc-700 text-white" : "text-zinc-300"
                }`}
                onClick={() => setTocTab("annotations")}
              >
                Annotations
              </button>
            </div>

            {tocTab === "chapters" ? (
              tocItems.length > 0 ? (
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
              )
            ) : annotationsByChapter.length > 0 ? (
              <div className="space-y-4">
                {annotationsByChapter.map((group) => (
                  <div key={group.chapter}>
                    <p className="mb-2 text-xs uppercase tracking-wide text-zinc-400">{group.chapter}</p>
                    <ul className="space-y-2">
                      {group.items.map((annotation) => (
                        <li key={annotation.id}>
                          <button
                            type="button"
                            className="w-full rounded border border-zinc-800 bg-zinc-900 px-2 py-2 text-left text-zinc-200 hover:border-zinc-600"
                            onClick={() => {
                              if (renditionRef.current) {
                                void renditionRef.current.display(annotation.cfi_range);
                              }
                              setTocOpen(false);
                            }}
                          >
                            <p className="truncate text-xs text-zinc-100">{annotationPreview(annotation)}</p>
                            <p className="mt-1 text-[10px] uppercase text-zinc-400">
                              {annotation.type} · {annotation.color}
                            </p>
                          </button>
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-zinc-400">No annotations yet.</p>
            )}
          </div>
        </SheetContent>
      </Sheet>
    </div>
  );
}
