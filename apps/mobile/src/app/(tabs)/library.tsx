import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  Animated,
  FlatList,
  Pressable,
  StyleSheet,
  Text,
  View,
  type ListRenderItem,
} from "react-native";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import type { BookSummary, PaginatedResponse } from "@autolibre/shared";
import { Ionicons } from "@expo/vector-icons";
import { Stack } from "expo-router";
import { useNetInfo } from "@react-native-community/netinfo";
import { BookCard } from "../../components/BookCard";
import { useApi } from "../../lib/api";
import { listLocalBooks } from "../../lib/db";
import { syncLibrary } from "../../lib/sync";
import { db } from "../../lib/db";

const PAGE_SIZE = 30;

export const LIBRARY_QUERY_KEY = ["books", "library"] as const;

function LoadingSkeleton() {
  const opacity = useRef(new Animated.Value(0.35)).current;

  useEffect(() => {
    const animation = Animated.loop(
      Animated.sequence([
        Animated.timing(opacity, {
          toValue: 0.8,
          duration: 500,
          useNativeDriver: false,
        }),
        Animated.timing(opacity, {
          toValue: 0.35,
          duration: 500,
          useNativeDriver: false,
        }),
      ]),
    );

    animation.start();
    return () => {
      animation.stop();
    };
  }, [opacity]);

  return (
    <View style={styles.skeletonGrid}>
      {Array.from({ length: 6 }).map((_, index) => (
        <Animated.View key={index} style={[styles.skeletonCard, { opacity }]} />
      ))}
    </View>
  );
}

function EmptyState() {
  return (
    <View style={styles.emptyState} testID="library-empty-state">
      <Ionicons name="library-outline" color="#0f766e" size={32} />
      <Text style={styles.emptyStateTitle}>Your library is empty</Text>
    </View>
  );
}

export default function LibraryScreen() {
  const client = useApi();
  const queryClient = useQueryClient();
  const netInfo = useNetInfo();
  const [isSyncing, setIsSyncing] = useState(false);

  const isOffline = netInfo.isConnected === false || netInfo.isInternetReachable === false;
  const isOnline = !isOffline;

  const booksQuery = useInfiniteQuery({
    queryKey: LIBRARY_QUERY_KEY,
    initialPageParam: 1,
    enabled: isOnline,
    queryFn: async ({ pageParam }) => {
      return client.listBooks({
        page: Number(pageParam),
        page_size: PAGE_SIZE,
      });
    },
    getNextPageParam: (
      lastPage: PaginatedResponse<BookSummary>,
      allPages: Array<PaginatedResponse<BookSummary>>,
    ) => {
      const loadedCount = allPages.reduce((count, page) => count + page.items.length, 0);
      return loadedCount < lastPage.total ? allPages.length + 1 : undefined;
    },
  });

  const localBooksQuery = useQuery({
    queryKey: ["books", "local"] as const,
    enabled: isOffline,
    queryFn: async () => {
      const database = await db;
      return listLocalBooks(database);
    },
  });

  const books = useMemo(
    () =>
      isOffline
        ? localBooksQuery.data ?? []
        : booksQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [booksQuery.data, isOffline, localBooksQuery.data],
  );

  const refreshLibrary = () => {
    if (isOffline) {
      void localBooksQuery.refetch();
      return;
    }

    void queryClient.invalidateQueries({ queryKey: LIBRARY_QUERY_KEY });
  };

  const renderItem: ListRenderItem<BookSummary> = ({ item }) => <BookCard book={item} />;

  useEffect(() => {
    if (isOffline) {
      return;
    }

    let cancelled = false;

    void (async () => {
      setIsSyncing(true);
      try {
        const database = await db;
        await syncLibrary(client, database);
      } catch {
        return;
      } finally {
        if (!cancelled) {
          setIsSyncing(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, isOffline]);

  const isLoading = isOffline ? localBooksQuery.isLoading : booksQuery.isLoading;

  if (isLoading) {
    return (
      <View style={styles.screen}>
        <Stack.Screen
          options={{
            headerRight: () =>
              isSyncing ? <ActivityIndicator color="#0f766e" size="small" /> : null,
          }}
        />
        <LoadingSkeleton />
      </View>
    );
  }

  return (
    <View style={styles.screen}>
      <Stack.Screen
        options={{
          headerRight: () =>
            isSyncing ? <ActivityIndicator color="#0f766e" size="small" /> : null,
        }}
      />
      <FlatList
        testID="library-list"
        data={books}
        renderItem={renderItem}
        keyExtractor={(item) => item.id}
        numColumns={2}
        columnWrapperStyle={styles.columnWrapper}
        contentContainerStyle={books.length === 0 ? styles.emptyContentContainer : styles.listContent}
        onRefresh={refreshLibrary}
        refreshing={
          isOffline
            ? localBooksQuery.isRefetching
            : booksQuery.isRefetching && !booksQuery.isFetchingNextPage
        }
        ListEmptyComponent={<EmptyState />}
        onEndReached={() => {
          if (!isOffline && booksQuery.hasNextPage && !booksQuery.isFetchingNextPage) {
            void booksQuery.fetchNextPage();
          }
        }}
        onEndReachedThreshold={0.7}
        ListFooterComponent={
          !isOffline && booksQuery.isFetchingNextPage ? (
            <Pressable style={styles.fetchingMore} disabled>
              <Text style={styles.fetchingMoreText}>Loading more…</Text>
            </Pressable>
          ) : null
        }
      />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    backgroundColor: "#fafafa",
    paddingHorizontal: 12,
    paddingTop: 12,
  },
  listContent: {
    paddingBottom: 24,
  },
  columnWrapper: {
    gap: 12,
  },
  emptyContentContainer: {
    flexGrow: 1,
    justifyContent: "center",
  },
  emptyState: {
    alignItems: "center",
    justifyContent: "center",
    gap: 8,
  },
  emptyStateTitle: {
    color: "#18181b",
    fontSize: 16,
    fontWeight: "600",
  },
  skeletonGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    justifyContent: "space-between",
    paddingBottom: 18,
  },
  skeletonCard: {
    width: "48%",
    aspectRatio: 2 / 3,
    borderRadius: 10,
    marginBottom: 12,
    backgroundColor: "#e4e4e7",
  },
  fetchingMore: {
    marginTop: 8,
    marginBottom: 16,
    alignItems: "center",
  },
  fetchingMoreText: {
    color: "#71717a",
    fontSize: 12,
  },
});
