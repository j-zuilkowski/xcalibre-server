import { useQuery } from "@tanstack/react-query";
import { Stack, useLocalSearchParams, useRouter } from "expo-router";
import { useMemo } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import type { ApiClient } from "@xs/shared";
import { EpubReaderScreen } from "../../features/reader/EpubReaderScreen";
import { PdfReaderScreen } from "../../features/reader/PdfReaderScreen";
import { useApi } from "../../lib/api";

type ReaderFormat = "EPUB" | "PDF";

const readerDatabase = {
  execAsync: async () => undefined,
  runAsync: async () => undefined,
} as never;

export default function ReaderScreen() {
  const router = useRouter();
  const client = useApi() as ApiClient;
  const params = useLocalSearchParams<{
    id?: string | string[];
    format?: string | string[];
    streamUrl?: string | string[];
  }>();
  const bookId = Array.isArray(params.id) ? params.id[0] : params.id;
  const rawFormat = Array.isArray(params.format) ? params.format[0] : params.format;
  const rawStreamUrl = Array.isArray(params.streamUrl) ? params.streamUrl[0] : params.streamUrl;
  const format = rawFormat?.toUpperCase() as ReaderFormat | undefined;

  const bookQuery = useQuery({
    queryKey: ["reader-book", bookId],
    queryFn: () => client.getBook(bookId as string),
    enabled: Boolean(bookId),
  });

  const title = useMemo(() => bookQuery.data?.title ?? "Reader", [bookQuery.data?.title]);

  if (!bookId || !format || !rawStreamUrl) {
    return (
      <View style={styles.screen}>
        <Stack.Screen options={{ title: "Reader" }} />
        <Text style={styles.message}>Book not downloaded</Text>
        <Pressable style={styles.backButton} onPress={() => router.back()}>
          <Text style={styles.backButtonText}>Back</Text>
        </Pressable>
      </View>
    );
  }

  if (bookQuery.isLoading) {
    return (
      <View style={styles.screen}>
        <Stack.Screen options={{ title: "Reader" }} />
        <Text style={styles.message}>Loading reader...</Text>
      </View>
    );
  }

  if (format === "PDF") {
    return (
      <PdfReaderScreen
        client={client}
        database={readerDatabase}
        bookId={bookId}
        title={title}
        filePath={rawStreamUrl}
        onBack={() => router.back()}
      />
    );
  }

  return (
    <EpubReaderScreen
      client={client}
      database={readerDatabase}
      bookId={bookId}
      title={title}
      format="EPUB"
      filePath={rawStreamUrl}
      streamUrl={rawStreamUrl}
      onBack={() => router.back()}
    />
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#0f172a",
    padding: 24,
  },
  message: {
    color: "#e2e8f0",
    fontSize: 18,
    fontWeight: "600",
    textAlign: "center",
  },
  backButton: {
    marginTop: 20,
    borderRadius: 999,
    backgroundColor: "#0f766e",
    paddingHorizontal: 18,
    paddingVertical: 10,
  },
  backButtonText: {
    color: "#f8fafc",
    fontWeight: "700",
  },
});
