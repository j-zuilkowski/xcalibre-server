import { useEffect, useMemo, useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { Stack, useLocalSearchParams, useRouter } from "expo-router";
import { useQuery } from "@tanstack/react-query";
import type { SQLiteDatabase } from "expo-sqlite";
import { useApi } from "../../lib/api";
import { db } from "../../lib/db";
import { getLocalPath } from "../../lib/downloads";
import { EpubReaderScreen } from "../../features/reader/EpubReaderScreen";
import { PdfReaderScreen } from "../../features/reader/PdfReaderScreen";

type ReaderFormat = "EPUB" | "PDF";

function normalizeFormat(format: string | undefined): ReaderFormat | null {
  if (!format) {
    return null;
  }

  const normalized = format.toUpperCase();
  if (normalized === "EPUB" || normalized === "PDF") {
    return normalized;
  }

  return null;
}

export default function ReaderEntryScreen() {
  const router = useRouter();
  const client = useApi();
  const params = useLocalSearchParams<{ id?: string | string[]; format?: string | string[] }>();
  const bookId = Array.isArray(params.id) ? params.id[0] : params.id;
  const formatParam = Array.isArray(params.format) ? params.format[0] : params.format;
  const format = normalizeFormat(formatParam);

  const [loading, setLoading] = useState(true);
  const [localPath, setLocalPath] = useState<string | null>(null);
  const [database, setDatabase] = useState<SQLiteDatabase | null>(null);

  const bookQuery = useQuery({
    queryKey: ["reader", "book", bookId],
    queryFn: () => client.getBook(bookId as string),
    enabled: Boolean(bookId),
    staleTime: 60_000,
  });

  useEffect(() => {
    if (!bookId || !format) {
      setLoading(false);
      setLocalPath(null);
      return;
    }

    let cancelled = false;

    void (async () => {
      const resolvedDatabase = await db;
      const path = await getLocalPath(resolvedDatabase, bookId, format);

      if (cancelled) {
        return;
      }

      setDatabase(resolvedDatabase);
      setLocalPath(path);
      setLoading(false);
    })();

    return () => {
      cancelled = true;
    };
  }, [bookId, format]);

  const title = useMemo(() => {
    if (bookQuery.data?.title) {
      return bookQuery.data.title;
    }
    return "Reader";
  }, [bookQuery.data?.title]);

  if (!bookId || !format) {
    return (
      <View style={styles.centered}>
        <Stack.Screen options={{ headerShown: false }} />
        <Text style={styles.errorText}>Invalid reader request.</Text>
        <Pressable style={styles.backButton} onPress={() => router.back()}>
          <Text style={styles.backButtonText}>Back</Text>
        </Pressable>
      </View>
    );
  }

  if (loading || !database) {
    return (
      <View style={styles.centered}>
        <Stack.Screen options={{ headerShown: false }} />
        <Text style={styles.loadingText}>Opening reader…</Text>
      </View>
    );
  }

  if (!localPath) {
    return (
      <View style={styles.centered}>
        <Stack.Screen options={{ headerShown: false }} />
        <Text style={styles.errorText}>Book not downloaded</Text>
        <Pressable style={styles.backButton} onPress={() => router.back()}>
          <Text style={styles.backButtonText}>Back</Text>
        </Pressable>
      </View>
    );
  }

  return (
    <View style={styles.readerScreen}>
      <Stack.Screen options={{ headerShown: false }} />
      {format === "EPUB" ? (
        <EpubReaderScreen
          client={client}
          database={database}
          bookId={bookId}
          title={title}
          filePath={localPath}
          onBack={() => router.back()}
        />
      ) : null}
      {format === "PDF" ? (
        <PdfReaderScreen
          client={client}
          database={database}
          bookId={bookId}
          title={title}
          filePath={localPath}
          onBack={() => router.back()}
        />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  readerScreen: {
    flex: 1,
    backgroundColor: "#020617",
  },
  centered: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 24,
    backgroundColor: "#020617",
    gap: 14,
  },
  loadingText: {
    color: "#e2e8f0",
    fontSize: 15,
  },
  errorText: {
    color: "#fecaca",
    fontSize: 15,
    fontWeight: "600",
  },
  backButton: {
    borderRadius: 999,
    borderWidth: 1,
    borderColor: "rgba(226, 232, 240, 0.45)",
    paddingHorizontal: 14,
    paddingVertical: 8,
    backgroundColor: "rgba(2, 6, 23, 0.65)",
  },
  backButtonText: {
    color: "#f8fafc",
    fontSize: 13,
    fontWeight: "600",
  },
});
