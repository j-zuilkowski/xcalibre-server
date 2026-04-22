import { useEffect, useRef, useState } from "react";
import { Animated, Pressable, StatusBar, StyleSheet, Text, View } from "react-native";
import * as SecureStore from "expo-secure-store";
import type { ApiClient as CalibreClient } from "@autolibre/shared";
import type { SQLiteDatabase } from "expo-sqlite";
import { loadProgress, saveProgress } from "../../lib/progress";

const FONT_KEY = "reader_font";
const NIGHT_KEY = "reader_night";

type EpubReaderScreenProps = {
  client: CalibreClient;
  database: SQLiteDatabase;
  bookId: string;
  title: string;
  filePath: string;
  onBack: () => void;
};

type EpubLocationPayload = {
  cfi?: string;
  percentage?: number;
};

function clampPercentage(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }

  if (value > 1) {
    return Math.max(0, Math.min(1, value / 100));
  }

  return Math.max(0, Math.min(1, value));
}

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

export function EpubReaderScreen({
  client,
  database,
  bookId,
  title,
  filePath,
  onBack,
}: EpubReaderScreenProps) {
  const [headerVisible, setHeaderVisible] = useState(true);
  const [headerOpacity] = useState(new Animated.Value(1));
  const [initialCfi, setInitialCfi] = useState<string | undefined>(undefined);
  const [loadingProgress, setLoadingProgress] = useState(true);
  const [fontFamily, setFontFamily] = useState<"Inter" | "Literata">("Inter");
  const [nightMode, setNightMode] = useState(false);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<{ cfi?: string; percentage: number } | null>(null);

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

      void saveProgress(client, database, bookId, "EPUB", {
        cfi: pending.cfi,
        percentage: pending.percentage,
      });
    }, 2_000);
  };

  const handleLocationChange = (payload: unknown) => {
    const location = extractLocation(payload);
    if (!location || typeof location.percentage !== "number") {
      return;
    }

    queueProgressSave({
      cfi: location.cfi,
      percentage: location.percentage,
    });
  };

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

  return (
    <View style={[styles.screen, nightMode ? styles.screenNight : null]}>
      <StatusBar hidden animated />

      {loadingProgress ? (
        <View style={styles.loadingState}>
          <Text style={styles.loadingText}>Loading reader…</Text>
        </View>
      ) : EpubRenderer ? (
        <EpubRenderer
          key={`epub-${bookId}-${initialCfi ?? "start"}-${fontFamily}-${nightMode ? "night" : "day"}`}
          src={filePath}
          source={filePath}
          path={filePath}
          filePath={filePath}
          bookPath={filePath}
          cfi={initialCfi}
          initialCfi={initialCfi}
          initialLocation={initialCfi}
          style={styles.reader}
          theme={nightMode ? "dark" : "light"}
          colorMode={nightMode ? "night" : "day"}
          font={fontFamily}
          fontFamily={fontFamily}
          swipeEnabled
          onLocationChange={handleLocationChange}
          onRelocated={handleLocationChange}
          onPageChange={handleLocationChange}
        />
      ) : (
        <View style={styles.loadingState}>
          <Text style={styles.loadingText}>EPUB renderer not available.</Text>
        </View>
      )}

      <Pressable style={styles.centerTapZone} onPress={toggleHeader} />

      <Animated.View
        style={[styles.header, { opacity: headerOpacity }]}
        pointerEvents={headerVisible ? "auto" : "none"}
      >
        <Pressable style={styles.headerButton} onPress={onBack}>
          <Text style={styles.headerButtonText}>Back</Text>
        </Pressable>
        <Text style={styles.headerTitle} numberOfLines={1}>
          {title}
        </Text>
        <View style={styles.headerActions}>
          <Pressable style={styles.headerActionButton} onPress={toggleFont}>
            <Text style={styles.headerActionText}>{fontFamily === "Inter" ? "Literata" : "Inter"}</Text>
          </Pressable>
          <Pressable style={styles.headerActionButton} onPress={toggleNight}>
            <Text style={styles.headerActionText}>{nightMode ? "Day" : "Night"}</Text>
          </Pressable>
        </View>
      </Animated.View>
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
});
