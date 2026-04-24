import { useEffect, useMemo, useState } from "react";
import { Alert, ActivityIndicator, FlatList, Pressable, StyleSheet, Text, View } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { Stack, useLocalSearchParams } from "expo-router";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { Book, BookSummary, Shelf } from "@autolibre/shared";
import { useApi } from "../../lib/api";
import { db } from "../../lib/db";
import {
  downloadBook,
  formatBytes,
  getLocalPath,
  getPreferredDownloadFormat,
  resolvePreferredDownloadFormat,
} from "../../lib/downloads";
import { BookCard } from "../../components/BookCard";

const SHELF_PAGE_SIZE = 30;

function ShelfHeaderIcon() {
  return (
    <View style={styles.headerIcon}>
      <Ionicons name="albums-outline" color="#5eead4" size={18} />
    </View>
  );
}

async function loadAllShelfBooks(client: ReturnType<typeof useApi>, shelfId: string): Promise<BookSummary[]> {
  const books: BookSummary[] = [];
  let page = 1;

  while (true) {
    const response = await client.listShelfBooks(shelfId, {
      page,
      page_size: 100,
    });

    books.push(...response.items);

    if (books.length >= response.total || response.items.length === 0) {
      break;
    }

    page += 1;
  }

  return books;
}

export default function ShelfDetailScreen() {
  const { t } = useTranslation();
  const client = useApi();
  const params = useLocalSearchParams<{ id?: string | string[] }>();
  const shelfId = Array.isArray(params.id) ? params.id[0] : params.id;
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  const [batchBusy, setBatchBusy] = useState(false);

  const shelvesQuery = useQuery({
    queryKey: ["shelves"],
    queryFn: () => client.listShelves(),
  });

  const shelfBooksQuery = useInfiniteQuery({
    queryKey: ["shelf-books", shelfId],
    initialPageParam: 1,
    enabled: Boolean(shelfId),
    queryFn: async ({ pageParam }) => {
      return await client.listShelfBooks(shelfId as string, {
        page: Number(pageParam),
        page_size: SHELF_PAGE_SIZE,
      });
    },
    getNextPageParam: (lastPage, allPages) => {
      const loadedCount = allPages.reduce((count, page) => count + page.items.length, 0);
      return loadedCount < lastPage.total ? allPages.length + 1 : undefined;
    },
  });

  useEffect(() => {
    if (!toastMessage) {
      return;
    }

    const timeout = setTimeout(() => {
      setToastMessage(null);
    }, 2200);

    return () => {
      clearTimeout(timeout);
    };
  }, [toastMessage]);

  const shelf = useMemo(() => {
    return shelvesQuery.data?.find((item: Shelf) => item.id === shelfId) ?? null;
  }, [shelfId, shelvesQuery.data]);

  const books = useMemo(
    () => shelfBooksQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [shelfBooksQuery.data],
  );

  const downloadAll = async (): Promise<void> => {
    if (!shelfId) {
      return;
    }

    const database = await db;
    const preferredFormat = await getPreferredDownloadFormat();
    const shelfBooks = await loadAllShelfBooks(client, shelfId);
    const selectedDownloads: Array<{
      book: Book;
      format: { format: string; size_bytes: number };
    }> = [];

    for (const book of shelfBooks) {
      const details = await client.getBook(book.id);
      const chosenFormat = resolvePreferredDownloadFormat(details.formats, preferredFormat);
      if (!chosenFormat) {
        continue;
      }

      const alreadyDownloaded = await getLocalPath(database, details.id, chosenFormat.format);
      if (alreadyDownloaded) {
        continue;
      }

      selectedDownloads.push({
        book: details,
        format: chosenFormat,
      });
    }

    if (selectedDownloads.length === 0) {
      Alert.alert("Download all", "All books in this shelf are already downloaded.");
      return;
    }

    const estimatedSizeBytes = selectedDownloads.reduce(
      (total, item) => total + item.format.size_bytes,
      0,
    );

    Alert.alert(
      "Download all",
      `Download ${selectedDownloads.length} books? This may use up to ${formatBytes(estimatedSizeBytes)}.`,
      [
        { text: t("common.cancel"), style: "cancel" },
        {
          text: "Download all",
          onPress: () => {
            void (async () => {
              setBatchBusy(true);
              try {
                for (const item of selectedDownloads) {
                  await downloadBook(client, database, item.book.id, item.format.format, {
                    title: item.book.title,
                    coverUrl: item.book.cover_url ?? client.coverUrl(item.book.id),
                    hasCover: item.book.has_cover,
                    sizeBytes: item.format.size_bytes,
                    skipStorageWarning: true,
                  });
                }

                setToastMessage(`Download started for ${selectedDownloads.length} books`);
              } catch {
                Alert.alert("Download all", "Unable to start one or more downloads.");
              } finally {
                setBatchBusy(false);
              }
            })();
          },
        },
      ],
    );
  };

  if (!shelfId) {
    return (
      <View style={styles.centered}>
        <Text style={styles.errorText}>{t("shelves.unable_to_load")}</Text>
      </View>
    );
  }

  return (
    <View style={styles.screen}>
      <Stack.Screen
        options={{
          title: shelf?.name ?? t("shelves.page_title"),
          headerRight:
            books.length > 0
              ? () => (
                  <Pressable
                    style={[styles.headerButton, batchBusy ? styles.headerButtonDisabled : null]}
                    onPress={() => {
                      void downloadAll();
                    }}
                    disabled={batchBusy}
                  >
                    <Text style={styles.headerButtonText}>
                      {batchBusy ? t("common.running") : "⬇ Download all"}
                    </Text>
                  </Pressable>
                )
              : undefined,
        }}
      />

      <FlatList
        testID="shelf-books"
        data={books}
        keyExtractor={(item) => item.id}
        numColumns={2}
        columnWrapperStyle={styles.columnWrapper}
        contentContainerStyle={books.length === 0 ? styles.emptyContent : styles.listContent}
        renderItem={({ item }) => <BookCard book={item} />}
        onEndReached={() => {
          if (shelfBooksQuery.hasNextPage && !shelfBooksQuery.isFetchingNextPage) {
            void shelfBooksQuery.fetchNextPage();
          }
        }}
        onEndReachedThreshold={0.7}
        ListFooterComponent={
          shelfBooksQuery.isFetchingNextPage ? (
            <View style={styles.footer}>
              <ActivityIndicator color="#14b8a6" size="small" />
            </View>
          ) : null
        }
        ListEmptyComponent={
          <View style={styles.emptyState}>
            <ShelfHeaderIcon />
            <Text style={styles.emptyTitle}>{t("shelves.no_books_yet")}</Text>
          </View>
        }
      />

      {toastMessage ? (
        <View style={styles.toast}>
          <Text style={styles.toastText}>{toastMessage}</Text>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    backgroundColor: "#0f172a",
  },
  centered: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#0f172a",
  },
  errorText: {
    color: "#fecaca",
    fontSize: 15,
    fontWeight: "600",
  },
  headerIcon: {
    width: 34,
    height: 34,
    borderRadius: 17,
    backgroundColor: "rgba(94, 234, 212, 0.12)",
    alignItems: "center",
    justifyContent: "center",
  },
  headerButton: {
    borderRadius: 999,
    backgroundColor: "#14b8a6",
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  headerButtonDisabled: {
    opacity: 0.7,
  },
  headerButtonText: {
    color: "#031a17",
    fontSize: 12,
    fontWeight: "800",
  },
  columnWrapper: {
    gap: 12,
  },
  listContent: {
    padding: 16,
    paddingBottom: 32,
  },
  emptyContent: {
    flexGrow: 1,
    padding: 16,
  },
  footer: {
    paddingVertical: 20,
  },
  emptyState: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: 10,
    paddingVertical: 60,
  },
  emptyTitle: {
    color: "#f8fafc",
    fontSize: 18,
    fontWeight: "700",
  },
  toast: {
    position: "absolute",
    left: 16,
    right: 16,
    bottom: 20,
    borderRadius: 14,
    backgroundColor: "rgba(15, 23, 42, 0.96)",
    borderWidth: 1,
    borderColor: "rgba(94, 234, 212, 0.28)",
    paddingHorizontal: 14,
    paddingVertical: 12,
  },
  toastText: {
    color: "#e2e8f0",
    fontSize: 13,
    fontWeight: "600",
    textAlign: "center",
  },
});
