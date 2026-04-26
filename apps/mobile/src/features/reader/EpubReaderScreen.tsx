/**
 * EPUB reader screen component.
 *
 * Renders an EPUB or MOBI file using the `react-native-foliojs` renderer (if
 * available). The renderer is resolved lazily at module load time via
 * `resolveEpubRenderer()` so the rest of the app does not hard-depend on the
 * native module — a graceful fallback message is shown when unavailable.
 *
 * Reading progress:
 * - On mount, the last saved CFI is loaded via `loadProgress()` (checks the
 *   server `GET /api/v1/books/:id/progress`, falls back to the local SQLite cache).
 * - On location change events from the renderer, `queueProgressSave()` debounces
 *   progress writes (2 s) and calls `saveProgress()` which:
 *   1. PATCHes the server via `PATCH /api/v1/books/:id/progress`.
 *   2. Writes the percentage to `local_sync_state` (key `progress_<bookId>`) in SQLite.
 *
 * Reader preferences:
 * - Font family ("Inter" / "Literata") persisted to Expo SecureStore under `"reader_font"`.
 * - Night mode (dark/light) persisted under `"reader_night"` as "1" / "0".
 * - Preferences are read on mount and applied to the renderer key (which forces a
 *   full renderer remount on preference change).
 *
 * Annotations:
 * - Loaded from `GET /api/v1/books/:id/annotations` on mount.
 * - Displayed as highlight overlays by passing a `rendererHighlights` array to the renderer.
 * - Creation, color/note edits, and deletion use optimistic UI:
 *   a temporary annotation is applied immediately; the server mutation
 *   either replaces it with the real ID or rolls back.
 * - The annotation sheet is a bottom-sheet Modal with three modes:
 *   "selection" (new annotation from text selection), "annotation" (edit existing),
 *   "list" (browse all annotations, tap to navigate).
 *
 * CFI vs page note:
 * - EPUB/MOBI positions are represented as CFI strings (e.g.
 *   `"epubcfi(/6/4[chapter-1]!/4/2/1:0)"`).
 * - PDF page numbers are handled separately in PdfReaderScreen.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Animated,
  Modal,
  Pressable,
  ScrollView,
  StatusBar,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";
import * as SecureStore from "expo-secure-store";
import { useTranslation } from "react-i18next";
import type {
  AnnotationColor,
  ApiClient as CalibreClient,
  BookAnnotation,
  CreateBookAnnotationRequest,
} from "@xs/shared";
import type { SQLiteDatabase } from "expo-sqlite";
import { loadProgress, saveProgress } from "../../lib/progress";
import {
  ANNOTATION_COLORS,
  annotationColorPatch,
  annotationIconName,
  annotationNotePatch,
  annotationPreviewText,
  createOptimisticAnnotation,
  removeAnnotation as removeAnnotationFromList,
  sortAnnotations,
  updateAnnotationColor,
  updateAnnotationNote,
  upsertAnnotation as upsertAnnotationInList,
} from "./annotations";

// Expo SecureStore keys for persisting reader preferences across app sessions.
// Values are stored in iOS Keychain / Android Keystore — encrypted at rest.
const FONT_KEY = "reader_font";   // "Inter" | "Literata"
const NIGHT_KEY = "reader_night"; // "1" (night mode) | "0" (day mode)

type EpubReaderScreenProps = {
  client: CalibreClient;
  database: SQLiteDatabase;
  bookId: string;
  title: string;
  format: "EPUB" | "MOBI" | "AZW3";
  filePath: string;
  streamUrl?: string;
  onBack: () => void;
};

type EpubLocationPayload = {
  cfi?: string;
  percentage?: number;
};

type AnnotationSheetMode = "selection" | "annotation" | "list" | null;

type SelectionDraft = {
  cfiRange: string;
  highlightedText: string;
  color: AnnotationColor;
  noteText: string;
  noteOpen: boolean;
};

/**
 * Clamps a progress percentage to [0, 1].
 * Values > 1 are interpreted as 0–100 scale and divided by 100.
 */
function clampPercentage(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }

  if (value > 1) {
    return Math.max(0, Math.min(1, value / 100));
  }

  return Math.max(0, Math.min(1, value));
}

/**
 * Normalises the location-change event payload emitted by the EPUB renderer.
 * Different versions of foliojs use different field names for the CFI and
 * percentage — this helper probes multiple candidate fields defensively.
 */
function extractLocation(payload: unknown): EpubLocationPayload | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }

  const source = payload as Record<string, unknown>;
  const nested =
    source.nativeEvent && typeof source.nativeEvent === "object"
      ? (source.nativeEvent as Record<string, unknown>)
      : source;

  const cfiCandidate =
    typeof nested.cfi === "string"
      ? nested.cfi
      : typeof nested.location === "string"
        ? nested.location
        : typeof nested.startCfi === "string"
          ? nested.startCfi
          : undefined;

  const percentageCandidate =
    typeof nested.percentage === "number"
      ? nested.percentage
      : typeof nested.progress === "number"
        ? nested.progress
        : typeof nested.position === "number"
          ? nested.position
          : undefined;

  if (!cfiCandidate && typeof percentageCandidate !== "number") {
    return null;
  }

  return {
    cfi: cfiCandidate,
    percentage: typeof percentageCandidate === "number" ? clampPercentage(percentageCandidate) : undefined,
  };
}

/**
 * Extracts the CFI range and highlighted text from a text-selection event.
 * Probes multiple candidate field names for cross-version compatibility
 * with foliojs-port.
 */
function extractSelection(payload: unknown): { cfiRange: string; highlightedText: string } | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }

  const source = payload as Record<string, unknown>;
  const nested =
    source.nativeEvent && typeof source.nativeEvent === "object"
      ? (source.nativeEvent as Record<string, unknown>)
      : source;

  const cfiRange =
    typeof nested.cfiRange === "string"
      ? nested.cfiRange
      : typeof nested.cfi_range === "string"
        ? nested.cfi_range
        : typeof nested.cfi === "string"
          ? nested.cfi
          : typeof nested.range === "string"
            ? nested.range
            : undefined;

  const highlightedText =
    typeof nested.highlightedText === "string"
      ? nested.highlightedText
      : typeof nested.highlighted_text === "string"
        ? nested.highlighted_text
        : typeof nested.text === "string"
          ? nested.text
          : typeof nested.value === "string"
            ? nested.value
            : undefined;

  if (!cfiRange || !highlightedText || highlightedText.trim().length === 0) {
    return null;
  }

  return {
    cfiRange,
    highlightedText: highlightedText.trim(),
  };
}

/**
 * Extracts a {@link BookAnnotation}-shaped object from a highlight-tap event
 * emitted by the renderer. Used to map the renderer event back to our own
 * annotation list for editing.
 */
function extractAnnotation(payload: unknown): BookAnnotation | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }

  const source = payload as Record<string, unknown>;
  const nested =
    source.annotation && typeof source.annotation === "object"
      ? (source.annotation as Record<string, unknown>)
      : source;

  const id = typeof nested.id === "string" ? nested.id : undefined;
  const cfiRange =
    typeof nested.cfi_range === "string"
      ? nested.cfi_range
      : typeof nested.cfiRange === "string"
        ? nested.cfiRange
        : undefined;

  if (!id && !cfiRange) {
    return null;
  }

  return {
    id: id ?? "annotation",
    user_id: typeof nested.user_id === "string" ? nested.user_id : "unknown",
    book_id: typeof nested.book_id === "string" ? nested.book_id : "unknown",
    type:
      nested.type === "note" || nested.type === "bookmark" || nested.type === "highlight"
        ? nested.type
        : "highlight",
    cfi_range: cfiRange ?? "",
    highlighted_text: typeof nested.highlighted_text === "string" ? nested.highlighted_text : null,
    note: typeof nested.note === "string" ? nested.note : null,
    color:
      nested.color === "green" || nested.color === "blue" || nested.color === "pink"
        ? nested.color
        : "yellow",
    created_at: typeof nested.created_at === "string" ? nested.created_at : new Date().toISOString(),
    updated_at: typeof nested.updated_at === "string" ? nested.updated_at : new Date().toISOString(),
  };
}

/**
 * Dynamically resolves the foliojs EPUB renderer component at module load time.
 * Returns null if the native module is not available (e.g. in Expo Go or on web).
 * Probes multiple export names for compatibility across library versions.
 */
function resolveEpubRenderer():
  | ((props: Record<string, unknown>) => JSX.Element)
  | null {
  try {
    const folio = require("react-native-foliojs") as Record<string, unknown>;
    const FolioComponent =
      (folio.FolioReaderView as ((props: Record<string, unknown>) => JSX.Element) | undefined) ??
      (folio.Reader as ((props: Record<string, unknown>) => JSX.Element) | undefined) ??
      (folio.default as ((props: Record<string, unknown>) => JSX.Element) | undefined);
    return FolioComponent ?? null;
  } catch {
    return null;
  }
}

const EpubRenderer = resolveEpubRenderer();

/**
 * EPUB reader screen component.
 *
 * @param client - API client used for reading progress sync and annotation CRUD.
 * @param database - Expo SQLite database handle for local progress caching.
 * @param bookId - UUID of the book being read.
 * @param title - Book title shown in the overlay header.
 * @param format - File format: "EPUB", "MOBI", or "AZW3".
 * @param filePath - Absolute local file path to the downloaded book file.
 * @param streamUrl - Optional server stream URL used when no local file is present.
 * @param onBack - Called when the user taps the Back button in the overlay header.
 */
export function EpubReaderScreen({
  client,
  database,
  bookId,
  title,
  format,
  filePath,
  streamUrl,
  onBack,
}: EpubReaderScreenProps) {
  const { t } = useTranslation();
  const [headerVisible, setHeaderVisible] = useState(true);
  const [headerOpacity] = useState(new Animated.Value(1));
  const [initialCfi, setInitialCfi] = useState<string | undefined>(undefined);
  const [loadingProgress, setLoadingProgress] = useState(true);
  const [fontFamily, setFontFamily] = useState<"Inter" | "Literata">("Inter");
  const [nightMode, setNightMode] = useState(false);
  const [annotations, setAnnotations] = useState<BookAnnotation[]>([]);
  const [sheetMode, setSheetMode] = useState<AnnotationSheetMode>(null);
  const [selectionDraft, setSelectionDraft] = useState<SelectionDraft | null>(null);
  const [activeAnnotationId, setActiveAnnotationId] = useState<string | null>(null);
  const [annotationNoteDraft, setAnnotationNoteDraft] = useState("");
  const [annotationMutationPending, setAnnotationMutationPending] = useState(false);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<{ cfi?: string; percentage: number } | null>(null);
  const annotationsRef = useRef<BookAnnotation[]>([]);
  const currentLocationRef = useRef<string | null>(null);
  // Prefer local file path for offline reading; fall back to stream URL for online-only access.
  const sourcePath = streamUrl ?? filePath;

  useEffect(() => {
    annotationsRef.current = annotations;
  }, [annotations]);

  useEffect(() => {
    void (async () => {
      const [storedFont, storedNight] = await Promise.all([
        SecureStore.getItemAsync(FONT_KEY),
        SecureStore.getItemAsync(NIGHT_KEY),
      ]);

      if (storedFont === "Literata") {
        setFontFamily("Literata");
      }

      setNightMode(storedNight === "1");
    })();
  }, []);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const progress = await loadProgress(client, bookId);
      if (cancelled) {
        return;
      }

      if (progress?.cfi) {
        setInitialCfi(progress.cfi);
      }
      setLoadingProgress(false);
    })();

    return () => {
      cancelled = true;
    };
  }, [bookId, client]);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        const remoteAnnotations = await client.listBookAnnotations(bookId);
        if (!cancelled) {
          const next = sortAnnotations(remoteAnnotations);
          annotationsRef.current = next;
          setAnnotations(next);
        }
      } catch {
        if (!cancelled) {
          annotationsRef.current = [];
          setAnnotations([]);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [bookId, client]);

  useEffect(() => {
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, []);

  const toggleHeader = () => {
    const nextVisible = !headerVisible;
    setHeaderVisible(nextVisible);

    Animated.timing(headerOpacity, {
      toValue: nextVisible ? 1 : 0,
      duration: 300,
      useNativeDriver: true,
    }).start();
  };

  /**
   * Debounces reading progress saves to avoid spamming the server on every page turn.
   * Stores the latest pending progress in a ref so the final debounce flush always
   * writes the most recent position even if multiple events arrive in quick succession.
   * Writes are flushed 2 seconds after the last location-change event.
   */
  const queueProgressSave = (next: { cfi?: string; percentage: number }) => {
    pendingRef.current = next;

    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    debounceRef.current = setTimeout(() => {
      const pending = pendingRef.current;
      if (!pending) {
        return;
      }

      void saveProgress(client, database, bookId, format, {
        cfi: pending.cfi,
        percentage: pending.percentage,
      });
    }, 2_000);
  };

  const handleLocationChange = useCallback(
    (payload: unknown) => {
      const location = extractLocation(payload);
      if (!location || typeof location.percentage !== "number") {
        return;
      }

      if (location.cfi) {
        currentLocationRef.current = location.cfi;
      }

      queueProgressSave({
        cfi: location.cfi,
        percentage: location.percentage,
      });
    },
    [client, database, format, bookId],
  );

  const toggleFont = () => {
    const next = fontFamily === "Inter" ? "Literata" : "Inter";
    setFontFamily(next);
    void SecureStore.setItemAsync(FONT_KEY, next);
  };

  const toggleNight = () => {
    const next = !nightMode;
    setNightMode(next);
    void SecureStore.setItemAsync(NIGHT_KEY, next ? "1" : "0");
  };

  const syncAnnotations = useCallback((next: BookAnnotation[]) => {
    const sorted = sortAnnotations(next);
    annotationsRef.current = sorted;
    setAnnotations(sorted);
    return sorted;
  }, []);

  const upsertAnnotationState = useCallback(
    (annotation: BookAnnotation) => {
      syncAnnotations(upsertAnnotationInList(annotationsRef.current, annotation));
    },
    [syncAnnotations],
  );

  const removeAnnotationState = useCallback(
    (annotationId: string) => {
      syncAnnotations(removeAnnotationFromList(annotationsRef.current, annotationId));
    },
    [syncAnnotations],
  );

  const closeAnnotationSheet = useCallback(() => {
    setSheetMode(null);
    setSelectionDraft(null);
    setActiveAnnotationId(null);
    setAnnotationNoteDraft("");
  }, []);

  const openSelectionSheet = useCallback((draft: { cfiRange: string; highlightedText: string }) => {
    setActiveAnnotationId(null);
    setAnnotationNoteDraft("");
    setSelectionDraft({
      cfiRange: draft.cfiRange,
      highlightedText: draft.highlightedText,
      color: "yellow",
      noteText: "",
      noteOpen: false,
    });
    setSheetMode("selection");
  }, []);

  const openAnnotationSheet = useCallback((annotation: BookAnnotation) => {
    setSelectionDraft(null);
    setActiveAnnotationId(annotation.id);
    setAnnotationNoteDraft(annotation.note ?? "");
    setSheetMode("annotation");
  }, []);

  const openListSheet = useCallback(() => {
    setSelectionDraft(null);
    setActiveAnnotationId(null);
    setAnnotationNoteDraft("");
    setSheetMode("list");
  }, []);

  const selectedAnnotation = useMemo(() => {
    if (!activeAnnotationId) {
      return null;
    }

    return annotations.find((annotation) => annotation.id === activeAnnotationId) ?? null;
  }, [activeAnnotationId, annotations]);

  const createAnnotation = useCallback(
    async (request: CreateBookAnnotationRequest) => {
      const optimistic = createOptimisticAnnotation(bookId, request);
      setAnnotationMutationPending(true);
      upsertAnnotationState(optimistic);

      try {
        const created = await client.createBookAnnotation(bookId, request);
        removeAnnotationState(optimistic.id);
        upsertAnnotationState(created);
        return created;
      } catch (error) {
        removeAnnotationState(optimistic.id);
        throw error;
      } finally {
        setAnnotationMutationPending(false);
      }
    },
    [bookId, client, removeAnnotationState, upsertAnnotationState],
  );

  const handleSelectionColorChange = useCallback((color: AnnotationColor) => {
    setSelectionDraft((previous) =>
      previous
        ? {
            ...previous,
            color,
          }
        : previous,
    );
  }, []);

  const handleSelectionHighlight = useCallback(async () => {
    if (!selectionDraft) {
      return;
    }

    try {
      await createAnnotation({
        type: "highlight",
        cfi_range: selectionDraft.cfiRange,
        highlighted_text: selectionDraft.highlightedText,
        note: null,
        color: selectionDraft.color,
      });
      closeAnnotationSheet();
    } catch {
      return;
    }
  }, [closeAnnotationSheet, createAnnotation, selectionDraft]);

  const handleSelectionNoteSave = useCallback(async () => {
    if (!selectionDraft) {
      return;
    }

    const note = selectionDraft.noteText.trim();
    if (!note) {
      return;
    }

    try {
      await createAnnotation({
        type: "note",
        cfi_range: selectionDraft.cfiRange,
        highlighted_text: selectionDraft.highlightedText,
        note,
        color: selectionDraft.color,
      });
      closeAnnotationSheet();
    } catch {
      return;
    }
  }, [closeAnnotationSheet, createAnnotation, selectionDraft]);

  const handleSelectionBookmark = useCallback(async () => {
    if (!selectionDraft) {
      return;
    }

    try {
      await createAnnotation({
        type: "bookmark",
        cfi_range: currentLocationRef.current ?? selectionDraft.cfiRange,
        highlighted_text: null,
        note: null,
        color: selectionDraft.color,
      });
      closeAnnotationSheet();
    } catch {
      return;
    }
  }, [closeAnnotationSheet, createAnnotation, selectionDraft]);

  const handleAnnotationColorChange = useCallback(
    async (color: AnnotationColor) => {
      if (!selectedAnnotation) {
        return;
      }

      const previous = selectedAnnotation;
      const optimistic = updateAnnotationColor(previous, color);

      setAnnotationMutationPending(true);
      upsertAnnotationState(optimistic);

      try {
        const updated = await client.patchBookAnnotation(bookId, previous.id, annotationColorPatch(color));
        upsertAnnotationState(updated);
      } catch {
        upsertAnnotationState(previous);
      } finally {
        setAnnotationMutationPending(false);
      }
    },
    [bookId, client, selectedAnnotation, upsertAnnotationState],
  );

  const handleAnnotationNoteSave = useCallback(async () => {
    if (!selectedAnnotation) {
      return;
    }

    const previous = selectedAnnotation;
    const optimistic = updateAnnotationNote(previous, annotationNoteDraft);

    setAnnotationMutationPending(true);
    upsertAnnotationState(optimistic);

    try {
      const updated = await client.patchBookAnnotation(
        bookId,
        previous.id,
        annotationNotePatch(annotationNoteDraft),
      );
      upsertAnnotationState(updated);
      setAnnotationNoteDraft(updated.note ?? "");
    } catch {
      upsertAnnotationState(previous);
      setAnnotationNoteDraft(previous.note ?? "");
    } finally {
      setAnnotationMutationPending(false);
    }
  }, [annotationNoteDraft, bookId, client, selectedAnnotation, upsertAnnotationState]);

  const handleAnnotationDelete = useCallback(async () => {
    if (!selectedAnnotation) {
      return;
    }

    const previous = selectedAnnotation;
    setAnnotationMutationPending(true);
    removeAnnotationState(previous.id);

    try {
      await client.deleteBookAnnotation(bookId, previous.id);
      closeAnnotationSheet();
    } catch {
      upsertAnnotationState(previous);
    } finally {
      setAnnotationMutationPending(false);
    }
  }, [bookId, client, closeAnnotationSheet, removeAnnotationState, selectedAnnotation, upsertAnnotationState]);

  const handleSelectionTextSelected = useCallback(
    (payload: unknown) => {
      const selection = extractSelection(payload);
      if (!selection) {
        return;
      }

      openSelectionSheet(selection);
    },
    [openSelectionSheet],
  );

  const handleRendererHighlight = useCallback(
    (payload: unknown) => {
      const annotation = extractAnnotation(payload);
      if (!annotation) {
        return;
      }

      const existing =
        annotationsRef.current.find((entry) => entry.id === annotation.id) ??
        annotationsRef.current.find((entry) => entry.cfi_range === annotation.cfi_range);

      if (!existing) {
        return;
      }

      openAnnotationSheet(existing);
    },
    [openAnnotationSheet],
  );

  const handleAnnotationNavigate = useCallback(
    (annotation: BookAnnotation) => {
      setInitialCfi(annotation.cfi_range);
      closeAnnotationSheet();
    },
    [closeAnnotationSheet],
  );

  const annotationSheetTitle =
    sheetMode === "selection" ? "Create annotation" : sheetMode === "annotation" ? "Edit annotation" : "Annotations";

  // Transform annotations into the shape expected by the renderer.
  // Bookmarks have no visual highlight in the text, so they are excluded.
  // Both `cfiRange` (camelCase) and `cfi` (snake_case) are provided for
  // cross-version compatibility with different foliojs builds.
  const rendererHighlights = useMemo(
    () =>
      annotations
        .filter((annotation) => annotation.type !== "bookmark")
        .map((annotation) => ({
          id: annotation.id,
          cfiRange: annotation.cfi_range,
          cfi: annotation.cfi_range,
          color: annotation.color,
          note: annotation.note,
          type: annotation.type,
        })),
    [annotations],
  );

  return (
    <View style={[styles.screen, nightMode ? styles.screenNight : null]}>
      <StatusBar hidden animated />

      {loadingProgress ? (
        <View style={styles.loadingState}>
          <Text style={styles.loadingText}>{t("reader.loading_reader")}</Text>
        </View>
      ) : EpubRenderer ? (
        <EpubRenderer
          key={`epub-${bookId}-${format}-${initialCfi ?? "start"}-${fontFamily}-${nightMode ? "night" : "day"}`}
          src={sourcePath}
          source={sourcePath}
          path={sourcePath}
          filePath={sourcePath}
          bookPath={sourcePath}
          cfi={initialCfi}
          initialCfi={initialCfi}
          initialLocation={initialCfi}
          style={styles.reader}
          theme={nightMode ? "dark" : "light"}
          colorMode={nightMode ? "night" : "day"}
          font={fontFamily}
          fontFamily={fontFamily}
          annotations={rendererHighlights}
          highlights={rendererHighlights}
          swipeEnabled
          onLocationChange={handleLocationChange}
          onRelocated={handleLocationChange}
          onPageChange={handleLocationChange}
          onTextSelected={handleSelectionTextSelected}
          onHighlight={handleRendererHighlight}
        />
      ) : (
        <View style={styles.loadingState}>
          <Text style={styles.loadingText}>{t("reader.epub_renderer_unavailable")}</Text>
        </View>
      )}

      <Pressable style={styles.centerTapZone} onPress={toggleHeader} />

      <Animated.View
        style={[styles.header, { opacity: headerOpacity }]}
        pointerEvents={headerVisible ? "auto" : "none"}
      >
        <Pressable style={styles.headerButton} onPress={onBack}>
          <Text style={styles.headerButtonText}>{t("common.back")}</Text>
        </Pressable>
        <Text style={styles.headerTitle} numberOfLines={1}>
          {title}
        </Text>
        <View style={styles.headerActions}>
          <Pressable style={styles.headerActionButton} onPress={toggleFont}>
            <Text style={styles.headerActionText}>{fontFamily === "Inter" ? "Literata" : "Inter"}</Text>
          </Pressable>
          <Pressable style={styles.headerActionButton} onPress={toggleNight}>
            <Text style={styles.headerActionText}>{nightMode ? t("reader.day_mode") : t("reader.night_mode")}</Text>
          </Pressable>
          <Pressable style={styles.headerActionButton} onPress={openListSheet}>
            <Text style={styles.headerActionText}>Annotations</Text>
          </Pressable>
        </View>
      </Animated.View>

      <Modal visible={sheetMode !== null} transparent animationType="slide" onRequestClose={closeAnnotationSheet}>
        <View className="flex-1 justify-end bg-black/60">
          <Pressable className="absolute inset-0" onPress={closeAnnotationSheet} />
          <View className="rounded-t-[28px] border-t border-zinc-800 bg-zinc-950 px-5 pb-8 pt-3">
            <View className="mx-auto mb-3 h-1.5 w-12 rounded-full bg-zinc-700" />
            <View className="flex-row items-center justify-between">
              <Text className="text-lg font-semibold text-zinc-50">{annotationSheetTitle}</Text>
              <Pressable className="rounded-full border border-zinc-700 px-3 py-1" onPress={closeAnnotationSheet}>
                <Text className="text-xs font-semibold text-zinc-200">Close</Text>
              </Pressable>
            </View>

            {sheetMode === "selection" && selectionDraft ? (
              <View className="mt-4 gap-4">
                <View className="rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3">
                  <Text className="text-xs uppercase tracking-[0.2em] text-zinc-500">Selected text</Text>
                  <Text className="mt-2 text-sm leading-6 text-zinc-100">{selectionDraft.highlightedText}</Text>
                </View>

                <View>
                  <Text className="mb-2 text-xs font-semibold uppercase tracking-wider text-zinc-400">Color</Text>
                  <View className="flex-row gap-3">
                    {ANNOTATION_COLORS.map((color) => (
                      <Pressable
                        key={color}
                        accessibilityRole="button"
                        accessibilityLabel={`${color} annotation color`}
                        disabled={annotationMutationPending}
                        onPress={() => handleSelectionColorChange(color)}
                        className="h-9 w-9 items-center justify-center rounded-full border"
                        style={{
                          borderColor:
                            selectionDraft.color === color ? "#f8fafc" : "rgba(82, 82, 91, 0.8)",
                          backgroundColor:
                            color === "yellow"
                              ? "rgba(255, 235, 59, 0.9)"
                              : color === "green"
                                ? "rgba(76, 175, 80, 0.85)"
                                : color === "blue"
                                  ? "rgba(33, 150, 243, 0.85)"
                                  : "rgba(233, 30, 99, 0.85)",
                        }}
                      />
                    ))}
                  </View>
                </View>

                {!selectionDraft.noteOpen ? (
                  <View className="flex-row gap-3">
                    <Pressable
                      className="flex-1 rounded-2xl bg-teal-500 px-4 py-3"
                      disabled={annotationMutationPending}
                      onPress={() =>
                        setSelectionDraft((previous) =>
                          previous ? { ...previous, noteOpen: true } : previous,
                        )
                      }
                    >
                      <Text className="text-center text-sm font-semibold text-zinc-950">Add note</Text>
                    </Pressable>
                    <Pressable
                      className="flex-1 rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3"
                      disabled={annotationMutationPending}
                      onPress={() => void handleSelectionHighlight()}
                    >
                      <Text className="text-center text-sm font-semibold text-zinc-100">Highlight</Text>
                    </Pressable>
                  </View>
                ) : (
                  <View className="gap-3">
                    <TextInput
                      value={selectionDraft.noteText}
                      onChangeText={(value) =>
                        setSelectionDraft((previous) => (previous ? { ...previous, noteText: value } : previous))
                      }
                      placeholder="Write a note"
                      placeholderTextColor="#64748b"
                      multiline
                      className="min-h-[112px] rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3 text-zinc-50"
                      editable={!annotationMutationPending}
                    />
                    <View className="flex-row gap-3">
                      <Pressable
                        className="flex-1 rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3"
                        disabled={annotationMutationPending}
                        onPress={() =>
                          setSelectionDraft((previous) =>
                            previous ? { ...previous, noteOpen: false, noteText: "" } : previous,
                          )
                        }
                      >
                        <Text className="text-center text-sm font-semibold text-zinc-100">Cancel</Text>
                      </Pressable>
                      <Pressable
                        className="flex-1 rounded-2xl bg-teal-500 px-4 py-3"
                        disabled={annotationMutationPending || selectionDraft.noteText.trim().length === 0}
                        onPress={() => void handleSelectionNoteSave()}
                      >
                        <Text className="text-center text-sm font-semibold text-zinc-950">Save note</Text>
                      </Pressable>
                    </View>
                  </View>
                )}

                <Pressable
                  className="rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3"
                  disabled={annotationMutationPending}
                  onPress={() => void handleSelectionBookmark()}
                >
                  <Text className="text-center text-sm font-semibold text-zinc-100">Bookmark</Text>
                </Pressable>
              </View>
            ) : null}

            {sheetMode === "annotation" && selectedAnnotation ? (
              <View className="mt-4 gap-4">
                <View className="rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3">
                  <Text className="text-xs uppercase tracking-[0.2em] text-zinc-500">Preview</Text>
                  <Text className="mt-2 text-sm leading-6 text-zinc-100">
                    {annotationPreviewText(selectedAnnotation)}
                  </Text>
                  <Text className="mt-2 text-xs text-zinc-500">{selectedAnnotation.cfi_range}</Text>
                </View>

                <View>
                  <Text className="mb-2 text-xs font-semibold uppercase tracking-wider text-zinc-400">Color</Text>
                  <View className="flex-row gap-3">
                    {ANNOTATION_COLORS.map((color) => (
                      <Pressable
                        key={color}
                        accessibilityRole="button"
                        accessibilityLabel={`${color} annotation color`}
                        disabled={annotationMutationPending}
                        onPress={() => void handleAnnotationColorChange(color)}
                        className="h-9 w-9 items-center justify-center rounded-full border"
                        style={{
                          borderColor:
                            selectedAnnotation.color === color ? "#f8fafc" : "rgba(82, 82, 91, 0.8)",
                          backgroundColor:
                            color === "yellow"
                              ? "rgba(255, 235, 59, 0.9)"
                              : color === "green"
                                ? "rgba(76, 175, 80, 0.85)"
                                : color === "blue"
                                  ? "rgba(33, 150, 243, 0.85)"
                                  : "rgba(233, 30, 99, 0.85)",
                        }}
                      />
                    ))}
                  </View>
                </View>

                <View className="gap-3">
                  <Text className="text-xs font-semibold uppercase tracking-wider text-zinc-400">Note</Text>
                  <TextInput
                    value={annotationNoteDraft}
                    onChangeText={setAnnotationNoteDraft}
                    placeholder="Add or edit note"
                    placeholderTextColor="#64748b"
                    multiline
                    className="min-h-[112px] rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3 text-zinc-50"
                    editable={!annotationMutationPending}
                  />
                  <Pressable
                    className="rounded-2xl bg-teal-500 px-4 py-3"
                    disabled={annotationMutationPending}
                    onPress={() => void handleAnnotationNoteSave()}
                  >
                    <Text className="text-center text-sm font-semibold text-zinc-950">Save note</Text>
                  </Pressable>
                </View>

                <Pressable
                  className="rounded-2xl border border-rose-900 bg-rose-950 px-4 py-3"
                  disabled={annotationMutationPending}
                  onPress={() => void handleAnnotationDelete()}
                >
                  <Text className="text-center text-sm font-semibold text-rose-100">Delete annotation</Text>
                </Pressable>
              </View>
            ) : null}

            {sheetMode === "list" ? (
              <View className="mt-4">
                {annotations.length === 0 ? (
                  <View className="rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-6">
                    <Text className="text-center text-sm text-zinc-400">No annotations yet</Text>
                  </View>
                ) : (
                  <ScrollView style={styles.annotationListScroll} contentContainerStyle={styles.annotationListContent}>
                    {annotations.map((annotation) => (
                      <Pressable
                        key={annotation.id}
                        className="flex-row items-center gap-3 rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3"
                        onPress={() => handleAnnotationNavigate(annotation)}
                      >
                        <View
                          className="h-9 w-9 items-center justify-center rounded-full"
                          style={{
                            backgroundColor:
                              annotation.color === "yellow"
                                ? "rgba(255, 235, 59, 0.18)"
                                : annotation.color === "green"
                                  ? "rgba(76, 175, 80, 0.18)"
                                  : annotation.color === "blue"
                                    ? "rgba(33, 150, 243, 0.18)"
                                    : "rgba(233, 30, 99, 0.18)",
                          }}
                        >
                          <Ionicons
                            name={annotationIconName(annotation.type)}
                            size={18}
                            color={
                              annotation.color === "yellow"
                                ? "#facc15"
                                : annotation.color === "green"
                                  ? "#4ade80"
                                  : annotation.color === "blue"
                                    ? "#38bdf8"
                                    : "#f472b6"
                            }
                          />
                        </View>

                        <View className="min-w-0 flex-1 gap-1">
                          <Text className="text-sm font-semibold text-zinc-100" numberOfLines={2}>
                            {annotationPreviewText(annotation)}
                          </Text>
                          <Text className="text-xs text-zinc-500" numberOfLines={1}>
                            {annotation.cfi_range}
                          </Text>
                        </View>

                        <View
                          className="h-4 w-4 rounded-full border border-zinc-700"
                          style={{
                            backgroundColor:
                              annotation.color === "yellow"
                                ? "rgba(255, 235, 59, 0.9)"
                                : annotation.color === "green"
                                  ? "rgba(76, 175, 80, 0.9)"
                                  : annotation.color === "blue"
                                    ? "rgba(33, 150, 243, 0.9)"
                                    : "rgba(233, 30, 99, 0.9)",
                          }}
                        />
                      </Pressable>
                    ))}
                  </ScrollView>
                )}
              </View>
            ) : null}
          </View>
        </View>
      </Modal>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    backgroundColor: "#f8fafc",
  },
  screenNight: {
    backgroundColor: "#020617",
  },
  reader: {
    flex: 1,
  },
  loadingState: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
  },
  loadingText: {
    color: "#94a3b8",
    fontSize: 14,
  },
  centerTapZone: {
    position: "absolute",
    top: "33%",
    left: "30%",
    width: "40%",
    height: "34%",
  },
  header: {
    position: "absolute",
    top: 0,
    left: 0,
    right: 0,
    paddingTop: 52,
    paddingHorizontal: 14,
    paddingBottom: 12,
    backgroundColor: "rgba(2, 6, 23, 0.78)",
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  headerButton: {
    borderRadius: 999,
    borderWidth: 1,
    borderColor: "rgba(226, 232, 240, 0.45)",
    paddingHorizontal: 12,
    paddingVertical: 7,
  },
  headerButtonText: {
    color: "#f8fafc",
    fontWeight: "600",
    fontSize: 12,
  },
  headerTitle: {
    flex: 1,
    color: "#f8fafc",
    fontSize: 15,
    fontWeight: "600",
  },
  headerActions: {
    flexDirection: "row",
    gap: 8,
  },
  headerActionButton: {
    borderRadius: 999,
    borderWidth: 1,
    borderColor: "rgba(226, 232, 240, 0.45)",
    paddingHorizontal: 10,
    paddingVertical: 7,
  },
  headerActionText: {
    color: "#f8fafc",
    fontSize: 11,
    fontWeight: "600",
  },
  annotationListScroll: {
    maxHeight: 420,
  },
  annotationListContent: {
    gap: 12,
    paddingBottom: 8,
  },
});
