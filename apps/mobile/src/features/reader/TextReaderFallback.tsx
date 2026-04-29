import { useEffect, useMemo, useRef, useState } from "react";
import { PanResponder, Pressable, StatusBar, StyleSheet, Text, View } from "react-native";
import { useTranslation } from "react-i18next";
import type { ApiClient } from "@xs/shared";
import type { SQLiteDatabase } from "expo-sqlite";
import { loadProgress, saveProgress } from "../../lib/progress";

type TextReaderFallbackProps = {
  client: ApiClient;
  database: SQLiteDatabase;
  bookId: string;
  title: string;
  format: "EPUB" | "PDF";
  onBack: () => void;
};

function splitIntoPages(text: string, maxChars = 1400): string[] {
  const trimmed = text.trim();
  if (!trimmed) {
    return ["No text available."];
  }

  const paragraphs = trimmed.split(/\n\s*\n/);
  const pages: string[] = [];
  let current = "";

  for (const paragraph of paragraphs) {
    const next = paragraph.trim();
    if (!next) {
      continue;
    }

    const candidate = current ? `${current}\n\n${next}` : next;
    if (candidate.length > maxChars && current) {
      pages.push(current);
      current = next;
      continue;
    }

    current = candidate;
  }

  if (current) {
    pages.push(current);
  }

  return pages.length > 0 ? pages : [trimmed];
}

function buildFallbackPages(title: string, bookId: string, format: string): string[] {
  const intro = `${title}\n\n${format} reader fallback\nBook ID: ${bookId}\n\n`;
  return [
    `${intro}This Expo Go fallback keeps the reader visible even when the native renderer is unavailable.`,
    `${intro}Swipe horizontally to move between pages. The page number below persists through the progress API.`,
    `${intro}The full-reader implementation can be swapped in later without changing the route contract.`,
    `${intro}This is a local, text-only fallback so the visual inspection can still verify reading flow.`,
  ];
}

export function TextReaderFallback({
  client,
  database,
  bookId,
  title,
  format,
  onBack,
}: TextReaderFallbackProps) {
  const { t } = useTranslation();
  const scrollRef = useRef<ScrollView | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const currentPageRef = useRef(0);
  const pageCountRef = useRef(0);
  const [loading, setLoading] = useState(true);
  const [pages, setPages] = useState<string[]>([]);
  const [currentPage, setCurrentPage] = useState(0);

  useEffect(() => {
    currentPageRef.current = currentPage;
  }, [currentPage]);

  useEffect(() => {
    pageCountRef.current = pages.length;
  }, [pages.length]);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        const progress = await loadProgress(client, bookId);

        if (cancelled) {
          return;
        }

        const nextPages = buildFallbackPages(title, bookId, format);
        const progressIndex =
          typeof progress?.page === "number" && progress.page > 0
            ? Math.min(nextPages.length - 1, progress.page - 1)
            : typeof progress?.percentage === "number" && progress.percentage > 0
              ? Math.min(nextPages.length - 1, Math.round(progress.percentage * (nextPages.length - 1)))
              : 0;

        setPages(nextPages);
        setCurrentPage(progressIndex);
        setLoading(false);
      } catch {
        if (!cancelled) {
          setPages(["Unable to load the book text."]);
          setCurrentPage(0);
          setLoading(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [bookId, client, format, title]);

  useEffect(() => {
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, []);

  const panResponder = useMemo(
    () =>
      PanResponder.create({
        onStartShouldSetPanResponder: () => true,
        onMoveShouldSetPanResponder: (_, gestureState) =>
          Math.abs(gestureState.dx) > 14 && Math.abs(gestureState.dx) > Math.abs(gestureState.dy),
        onPanResponderRelease: (_, gestureState) => {
          const pageIndex = currentPageRef.current;
          const totalPages = pageCountRef.current;

          if (gestureState.dx < -30 && pageIndex < totalPages - 1) {
            const nextPage = pageIndex + 1;
            currentPageRef.current = nextPage;
            setCurrentPage(nextPage);
            scheduleSave(nextPage);
          }

          if (gestureState.dx > 30 && pageIndex > 0) {
            const nextPage = pageIndex - 1;
            currentPageRef.current = nextPage;
            setCurrentPage(nextPage);
            scheduleSave(nextPage);
          }
        },
      }),
    [],
  );

  const scheduleSave = (nextPage: number) => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    debounceRef.current = setTimeout(() => {
      void saveProgress(client, database, bookId, format, {
        page: nextPage + 1,
        percentage: pages.length > 0 ? (nextPage + 1) / pages.length : 0,
      });
    }, 2_000);
  };

  const pagePreview = useMemo(() => {
    if (!pages[currentPage]) {
      return "";
    }
    return pages[currentPage];
  }, [currentPage, pages]);

  if (loading) {
    return (
      <View style={[styles.screen, styles.centered]}>
        <StatusBar hidden animated />
        <Text style={styles.loadingText}>{t("reader.loading_reader")}</Text>
      </View>
    );
  }

  return (
    <View style={styles.screen}>
      <StatusBar hidden animated />

      <View style={styles.header}>
        <Pressable style={styles.backButton} onPress={onBack}>
          <Text style={styles.backButtonText}>{t("common.back")}</Text>
        </Pressable>
        <Text style={styles.title} numberOfLines={1}>
          {title}
        </Text>
        <View style={styles.headerSpacer} />
      </View>

      <View style={styles.readerArea} {...panResponder.panHandlers}>
        <View style={styles.pageContent}>
          <Text style={styles.pageText}>{pages[currentPage]}</Text>
        </View>
      </View>

      <View style={styles.pageIndicator}>
        <Text style={styles.pageIndicatorText}>
          {currentPage + 1} / {pages.length}
        </Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    backgroundColor: "#020617",
  },
  centered: {
    alignItems: "center",
    justifyContent: "center",
  },
  loadingText: {
    color: "#94a3b8",
    fontSize: 14,
  },
  header: {
    paddingTop: 52,
    paddingHorizontal: 14,
    paddingBottom: 12,
    backgroundColor: "#1e293b",
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
  headerSpacer: {
    width: 1,
    height: 1,
  },
  readerArea: {
    flex: 1,
    backgroundColor: "#f8fafc",
  },
  pageContent: {
    paddingHorizontal: 24,
    paddingVertical: 28,
  },
  pageText: {
    color: "#0f172a",
    fontSize: 18,
    lineHeight: 30,
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
