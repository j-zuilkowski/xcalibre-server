/**
 * PDF reader screen component.
 *
 * Renders a PDF file using `react-native-pdf` (resolved lazily via
 * `resolvePdfRenderer()` so the rest of the app does not hard-depend on the
 * native module). Shows a graceful fallback when the module is unavailable.
 *
 * Reading progress:
 * - On mount, the last saved page number is loaded via `loadProgress()`.
 * - On each page change, `queueProgressSave()` debounces the save (2 s) and calls
 *   `saveProgress()` which PATCHes `PATCH /api/v1/books/:id/progress` with the page
 *   number and a percentage (page / totalPages).
 *
 * CFI vs page note:
 * - PDF progress is tracked by physical page number (integer, 1-based).
 * - EPUB/MOBI positions use CFI strings — handled by EpubReaderScreen.
 *
 * UI:
 * - Horizontal paging mode (`enablePaging + horizontal`).
 * - Overlay header (title + Back) and bottom page indicator are hidden/shown
 *   by tapping the center tap zone.
 */
import { useEffect, useRef, useState } from "react";
import { Pressable, StatusBar, StyleSheet, Text, View } from "react-native";
import { useTranslation } from "react-i18next";
import type { ApiClient as CalibreClient } from "@xs/shared";
import type { SQLiteDatabase } from "expo-sqlite";
import { loadProgress, saveProgress } from "../../lib/progress";

type PdfReaderScreenProps = {
  client: CalibreClient;
  database: SQLiteDatabase;
  bookId: string;
  title: string;
  filePath: string;
  onBack: () => void;
};

type PdfRendererProps = {
  source: { uri: string };
  style: Record<string, unknown>;
  page?: number;
  enablePaging?: boolean;
  horizontal?: boolean;
  onLoadComplete?: (pageCount: number, currentPage?: number) => void;
  onPageChanged?: (page: number, pageCount: number) => void;
};

/**
 * Dynamically resolves the `react-native-pdf` default export at module load time.
 * Returns null when the native module is not available (e.g. in Expo Go).
 */
function resolvePdfRenderer(): ((props: PdfRendererProps) => JSX.Element) | null {
  try {
    const module = require("react-native-pdf") as { default?: (props: PdfRendererProps) => JSX.Element };
    return module.default ?? null;
  } catch {
    return null;
  }
}

const PdfRenderer = resolvePdfRenderer();

function clampPercentage(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(1, value));
}

/**
 * PDF reader screen component.
 *
 * @param client - API client used for reading progress sync.
 * @param database - Expo SQLite database handle for local progress caching.
 * @param bookId - UUID of the book being read.
 * @param title - Book title shown in the overlay header.
 * @param filePath - Absolute local file path to the downloaded PDF file.
 * @param onBack - Called when the user taps the Back button in the overlay header.
 */
export function PdfReaderScreen({
  client,
  database,
  bookId,
  title,
  filePath,
  onBack,
}: PdfReaderScreenProps) {
  const { t } = useTranslation();
  const [overlayVisible, setOverlayVisible] = useState(true);
  const [initialPage, setInitialPage] = useState(1);
  const [currentPage, setCurrentPage] = useState(1);
  const [pageCount, setPageCount] = useState(0);
  const [loadingProgress, setLoadingProgress] = useState(true);
  const [progressLabelVisible, setProgressLabelVisible] = useState(true);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<{ page: number; percentage: number } | null>(null);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const progress = await loadProgress(client, bookId);
      if (cancelled) {
        return;
      }

      if (typeof progress?.page === "number" && progress.page > 0) {
        setInitialPage(progress.page);
        setCurrentPage(progress.page);
      }
      setLoadingProgress(false);
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

  /**
   * Debounces progress saves (2 s) to avoid saving on every swipe.
   * Calculates the percentage as `page / total` clamped to [0, 1].
   */
  const queueProgressSave = (page: number, total: number) => {
    pendingRef.current = {
      page,
      percentage: clampPercentage(total > 0 ? page / total : 0),
    };

    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    debounceRef.current = setTimeout(() => {
      const pending = pendingRef.current;
      if (!pending) {
        return;
      }

      void saveProgress(client, database, bookId, "PDF", {
        page: pending.page,
        percentage: pending.percentage,
      });
    }, 2_000);
  };

  const handlePageChanged = (page: number, total: number) => {
    setCurrentPage(page);
    setPageCount(total);
    queueProgressSave(page, total);
  };

  const toggleCenterOverlay = () => {
    const next = !overlayVisible;
    setOverlayVisible(next);
    setProgressLabelVisible(next);
  };

  return (
    <View style={styles.screen}>
      <StatusBar hidden animated />

      {loadingProgress ? (
        <View style={styles.loadingState}>
          <Text style={styles.loadingText}>{t("reader.loading_reader")}</Text>
        </View>
      ) : PdfRenderer ? (
        <PdfRenderer
          source={{ uri: filePath }}
          style={styles.pdf}
          page={initialPage}
          enablePaging
          horizontal
          onLoadComplete={(pages, page) => {
            setPageCount(pages);
            if (typeof page === "number" && page > 0) {
              setCurrentPage(page);
            }
          }}
          onPageChanged={handlePageChanged}
        />
      ) : (
        <View style={styles.loadingState}>
          <Text style={styles.loadingText}>{t("reader.pdf_renderer_unavailable")}</Text>
        </View>
      )}

      <Pressable style={styles.centerTapZone} onPress={toggleCenterOverlay} />

      {overlayVisible ? (
        <View style={styles.topOverlay}>
          <Pressable style={styles.backButton} onPress={onBack}>
            <Text style={styles.backButtonText}>{t("common.back")}</Text>
          </Pressable>
          <Text style={styles.title} numberOfLines={1}>
            {title}
          </Text>
        </View>
      ) : null}

      {progressLabelVisible && pageCount > 0 ? (
        <View style={styles.pageIndicator}>
          <Text style={styles.pageIndicatorText}>
            {currentPage} / {pageCount}
          </Text>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    backgroundColor: "#020617",
  },
  pdf: {
    flex: 1,
    backgroundColor: "#020617",
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
  topOverlay: {
    position: "absolute",
    top: 0,
    left: 0,
    right: 0,
    paddingTop: 52,
    paddingHorizontal: 14,
    paddingBottom: 12,
    backgroundColor: "rgba(2, 6, 23, 0.75)",
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  backButton: {
    borderRadius: 999,
    borderWidth: 1,
    borderColor: "rgba(226, 232, 240, 0.45)",
    paddingHorizontal: 12,
    paddingVertical: 7,
  },
  backButtonText: {
    color: "#f8fafc",
    fontWeight: "600",
    fontSize: 12,
  },
  title: {
    flex: 1,
    color: "#f8fafc",
    fontSize: 15,
    fontWeight: "600",
  },
  pageIndicator: {
    position: "absolute",
    bottom: 32,
    alignSelf: "center",
    borderRadius: 999,
    backgroundColor: "rgba(2, 6, 23, 0.75)",
    paddingHorizontal: 14,
    paddingVertical: 8,
  },
  pageIndicatorText: {
    color: "#f8fafc",
    fontSize: 12,
    fontWeight: "700",
  },
});
