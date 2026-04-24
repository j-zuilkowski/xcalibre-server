import { useEffect, useMemo, useState } from "react";
import {
  Alert,
  FlatList,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
  type ListRenderItem,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { Image } from "expo-image";
import * as FileSystem from "expo-file-system";
import { Stack, useRouter } from "expo-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Swipeable } from "react-native-gesture-handler";
import { useTranslation } from "react-i18next";
import { useApi } from "../lib/api";
import { db } from "../lib/db";
import {
  cancelDownload,
  deleteDownload,
  formatBytes,
  getDownloadSummary,
  listDownloadedBooks,
  downloadBook,
  type DownloadQueueItem,
  type DownloadedBookRow,
  useDownloadQueue,
} from "../lib/downloads";

function downloadProgressLabel(progress: number): string {
  return `${Math.round(Math.max(0, Math.min(1, progress)) * 100)}%`;
}

function groupDownloadedBooks(items: DownloadedBookRow[]): Array<{ format: string; items: DownloadedBookRow[] }> {
  const groups = new Map<string, DownloadedBookRow[]>();

  for (const item of items) {
    const key = item.format.toUpperCase();
    const current = groups.get(key) ?? [];
    current.push(item);
    groups.set(key, current);
  }

  const preferredOrder = ["EPUB", "MOBI", "PDF"];

  return Array.from(groups.entries())
    .map(([format, groupItems]) => ({
      format,
      items: groupItems.sort((left, right) => left.title.localeCompare(right.title)),
    }))
    .sort((left, right) => {
      const leftIndex = preferredOrder.indexOf(left.format);
      const rightIndex = preferredOrder.indexOf(right.format);

      if (leftIndex !== rightIndex) {
        if (leftIndex === -1) return 1;
        if (rightIndex === -1) return -1;
        return leftIndex - rightIndex;
      }

      return left.format.localeCompare(right.format);
    });
}

function DownloadFormatBadge({ format }: { format: string }) {
  return (
    <View style={styles.formatBadge}>
      <Text style={styles.formatBadgeText}>{format}</Text>
    </View>
  );
}

function ProgressBar({ progress }: { progress: number }) {
  const clamped = Math.max(0, Math.min(1, progress));
  return (
    <View style={styles.progressTrack}>
      <View style={[styles.progressFill, { width: `${clamped * 100}%` }]} />
    </View>
  );
}

function DownloadRowCover({
  coverUrl,
  hasCover,
  title,
}: {
  coverUrl: string | null;
  hasCover: boolean;
  title: string;
}) {
  const [imageFailed, setImageFailed] = useState(false);
  const resolvedCoverUrl = coverUrl;

  if (!hasCover || imageFailed || !resolvedCoverUrl) {
    return (
      <View style={styles.coverPlaceholder}>
        <Text style={styles.coverPlaceholderText}>{title.trim().charAt(0).toUpperCase() || "?"}</Text>
      </View>
    );
  }

  return (
    <Image
      source={{ uri: resolvedCoverUrl }}
      cachePolicy="memory-disk"
      contentFit="cover"
      style={styles.coverImage}
      onError={() => setImageFailed(true)}
    />
  );
}

function ActiveDownloadCard({
  item,
  onCancel,
}: {
  item: DownloadQueueItem;
  onCancel: (item: DownloadQueueItem) => void;
}) {
  return (
    <View style={styles.sectionCard}>
      <View style={styles.downloadRow}>
        <View style={styles.coverFrame}>
          <DownloadRowCover coverUrl={item.coverUrl} hasCover={item.hasCover} title={item.title} />
        </View>

        <View style={styles.rowBody}>
          <View style={styles.rowHeader}>
            <Text numberOfLines={2} style={styles.rowTitle}>
              {item.title}
            </Text>
            <DownloadFormatBadge format={item.format} />
          </View>

          <ProgressBar progress={item.progress} />
          <Text style={styles.progressText}>
            {downloadProgressLabel(item.progress)}
            {item.totalBytesExpected > 0 ? ` · ${formatBytes(item.totalBytesWritten)} / ${formatBytes(item.totalBytesExpected)}` : ""}
          </Text>

          <Pressable
            style={styles.secondaryButton}
            onPress={() => {
              onCancel(item);
            }}
          >
            <Text style={styles.secondaryButtonText}>Cancel</Text>
          </Pressable>
        </View>
      </View>
    </View>
  );
}

function FailedDownloadCard({
  item,
  onRetry,
}: {
  item: DownloadQueueItem;
  onRetry: (item: DownloadQueueItem) => void;
}) {
  return (
    <View style={styles.sectionCard}>
      <View style={styles.downloadRow}>
        <View style={styles.coverFrame}>
          <DownloadRowCover coverUrl={item.coverUrl} hasCover={item.hasCover} title={item.title} />
        </View>

        <View style={styles.rowBody}>
          <View style={styles.rowHeader}>
            <Text numberOfLines={2} style={styles.rowTitle}>
              {item.title}
            </Text>
            <DownloadFormatBadge format={item.format} />
          </View>

          <Text style={styles.errorText} numberOfLines={3}>
            {item.errorMessage ?? "Download failed."}
          </Text>

          <Pressable
            style={styles.primaryButton}
            onPress={() => {
              onRetry(item);
            }}
          >
            <Text style={styles.primaryButtonText}>Retry</Text>
          </Pressable>
        </View>
      </View>
    </View>
  );
}

function DownloadedRow({
  item,
  onDelete,
}: {
  item: DownloadedBookRow;
  onDelete: (item: DownloadedBookRow) => void;
}) {
  const router = useRouter();
  const client = useApi();

  const navigateToReader = () => {
    router.push({
      pathname: "/reader/[id]",
      params: {
        id: item.bookId,
        format: item.format,
      },
    });
  };

  const renderRightActions = () => (
    <Pressable
      style={styles.deleteAction}
      onPress={() => {
        onDelete(item);
      }}
    >
      <Ionicons name="trash-outline" color="#fee2e2" size={20} />
      <Text style={styles.deleteActionText}>Delete</Text>
    </Pressable>
  );

  return (
    <Swipeable renderRightActions={renderRightActions}>
      <Pressable style={styles.sectionCard} onPress={navigateToReader}>
        <View style={styles.downloadRow}>
          <View style={styles.coverFrame}>
            <DownloadRowCover
              coverUrl={item.coverUrl ?? client.coverUrl(item.bookId)}
              hasCover={item.hasCover}
              title={item.title}
            />
          </View>

          <View style={styles.rowBody}>
            <View style={styles.rowHeader}>
              <Text numberOfLines={2} style={styles.rowTitle}>
                {item.title}
              </Text>
              <DownloadFormatBadge format={item.format} />
            </View>
            <Text style={styles.downloadMeta}>{formatBytes(item.sizeBytes)}</Text>
          </View>
        </View>
      </Pressable>
    </Swipeable>
  );
}

export default function DownloadsScreen() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const client = useApi();
  const queue = useDownloadQueue();

  const downloadSummaryQuery = useQuery({
    queryKey: ["downloads", "summary"],
    queryFn: async () => {
      const database = await db;
      return await getDownloadSummary(database);
    },
  });

  const downloadsQuery = useQuery({
    queryKey: ["downloads", "items"],
    queryFn: async () => {
      const database = await db;
      return await listDownloadedBooks(database);
    },
  });

  const storageQuery = useQuery({
    queryKey: ["downloads", "free-space"],
    queryFn: async () => {
      try {
        return await FileSystem.getFreeDiskStorageAsync();
      } catch {
        return 0;
      }
    },
    staleTime: 30_000,
  });

  const queueSignature = useMemo(
    () => queue.map((item) => `${item.key}:${item.status}`).join("|"),
    [queue],
  );

  useEffect(() => {
    void queryClient.invalidateQueries({ queryKey: ["downloads"] });
  }, [queryClient, queueSignature]);

  const activeDownloads = useMemo(
    () => queue.filter((item) => item.status === "downloading"),
    [queue],
  );

  const failedDownloads = useMemo(
    () => queue.filter((item) => item.status === "failed"),
    [queue],
  );

  const completedDownloads = downloadsQuery.data ?? [];
  const completedGroups = useMemo(
    () => groupDownloadedBooks(completedDownloads),
    [completedDownloads],
  );

  const usedBytes = downloadSummaryQuery.data?.usedBytes ?? 0;
  const freeBytes = storageQuery.data ?? 0;
  const totalBytes = usedBytes + freeBytes;
  const fillRatio = totalBytes > 0 ? Math.max(0, Math.min(1, usedBytes / totalBytes)) : 0;
  const downloadsEmpty =
    activeDownloads.length === 0 && failedDownloads.length === 0 && completedDownloads.length === 0;

  const cancelItem = async (item: DownloadQueueItem): Promise<void> => {
    await cancelDownload(item.bookId, item.format);
  };

  const retryItem = async (item: DownloadQueueItem): Promise<void> => {
    const database = await db;
    await downloadBook(client, database, item.bookId, item.format, {
      title: item.title,
      coverUrl: item.coverUrl,
      hasCover: item.hasCover,
      sizeBytes: item.sizeBytes ?? undefined,
    });
  };

  const removeItem = async (item: DownloadedBookRow): Promise<void> => {
    Alert.alert("Delete download", "This will remove the file from your device.", [
      { text: t("common.cancel"), style: "cancel" },
      {
        text: t("common.delete"),
        style: "destructive",
        onPress: () => {
          void (async () => {
            const database = await db;
            await deleteDownload(database, item.bookId, item.format);
            await queryClient.invalidateQueries({ queryKey: ["downloads"] });
          })();
        },
      },
    ]);
  };

  return (
    <ScrollView style={styles.screen} contentContainerStyle={styles.content}>
      <Stack.Screen
        options={{
          title: t("downloads.page_title"),
        }}
      />

      <View style={styles.header}>
        <Text style={styles.kicker}>{t("downloads.page_title")}</Text>
        <Text style={styles.title}>{t("downloads.page_title")}</Text>
      </View>

      <View style={styles.summaryCard}>
        <View style={styles.summaryRow}>
          <Text style={styles.summaryLabel}>Used: {formatBytes(usedBytes)}</Text>
          <Text style={styles.summaryLabel}>Available: {formatBytes(freeBytes)}</Text>
        </View>
        <View style={styles.summaryTrack}>
          <View style={[styles.summaryFill, { width: `${fillRatio * 100}%` }]} />
        </View>
      </View>

      {activeDownloads.length > 0 ? (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>In Progress</Text>
          <FlatList
            scrollEnabled={false}
            data={activeDownloads}
            keyExtractor={(item) => item.key}
            renderItem={({ item }) => <ActiveDownloadCard item={item} onCancel={cancelItem} />}
          />
        </View>
      ) : null}

      {completedGroups.length > 0 ? (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Downloaded</Text>
          {completedGroups.map((group) => (
            <View key={group.format} style={styles.groupBlock}>
              <View style={styles.groupHeader}>
                <Text style={styles.groupTitle}>{group.format}</Text>
                <Text style={styles.groupCount}>{group.items.length}</Text>
              </View>
              <FlatList
                scrollEnabled={false}
                data={group.items}
                keyExtractor={(item) => `${item.bookId}:${item.format}`}
                renderItem={({ item }) => <DownloadedRow item={item} onDelete={removeItem} />}
              />
            </View>
          ))}
        </View>
      ) : null}

      {failedDownloads.length > 0 ? (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Failed</Text>
          <FlatList
            scrollEnabled={false}
            data={failedDownloads}
            keyExtractor={(item) => item.key}
            renderItem={({ item }) => <FailedDownloadCard item={item} onRetry={retryItem} />}
          />
        </View>
      ) : null}

      {downloadsEmpty ? (
        <View style={styles.emptyState}>
          <Ionicons name="download-outline" color="#5eead4" size={32} />
          <Text style={styles.emptyTitle}>No downloads yet</Text>
          <Text style={styles.emptySubtitle}>
            Tap ↓ on any book to download for offline reading.
          </Text>
        </View>
      ) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    backgroundColor: "#0f172a",
  },
  content: {
    flexGrow: 1,
    padding: 16,
    gap: 14,
  },
  header: {
    gap: 6,
    marginTop: 8,
  },
  kicker: {
    color: "#5eead4",
    fontSize: 12,
    fontWeight: "700",
    letterSpacing: 1.2,
    textTransform: "uppercase",
  },
  title: {
    color: "#f8fafc",
    fontSize: 28,
    fontWeight: "700",
  },
  summaryCard: {
    borderRadius: 16,
    backgroundColor: "#111827",
    borderWidth: 1,
    borderColor: "rgba(20, 184, 166, 0.28)",
    padding: 14,
    gap: 10,
  },
  summaryRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    gap: 12,
  },
  summaryLabel: {
    color: "#cbd5e1",
    fontSize: 12,
    fontWeight: "700",
  },
  summaryTrack: {
    height: 10,
    borderRadius: 999,
    backgroundColor: "rgba(15, 23, 42, 0.9)",
    overflow: "hidden",
  },
  summaryFill: {
    height: "100%",
    borderRadius: 999,
    backgroundColor: "#14b8a6",
  },
  section: {
    gap: 10,
  },
  sectionTitle: {
    color: "#f8fafc",
    fontSize: 18,
    fontWeight: "700",
  },
  sectionCard: {
    borderRadius: 16,
    backgroundColor: "#111827",
    borderWidth: 1,
    borderColor: "rgba(148, 163, 184, 0.2)",
    padding: 14,
    marginBottom: 12,
  },
  downloadRow: {
    flexDirection: "row",
    gap: 12,
  },
  coverFrame: {
    width: 68,
    aspectRatio: 2 / 3,
    borderRadius: 10,
    overflow: "hidden",
    backgroundColor: "#1f2937",
  },
  coverImage: {
    width: "100%",
    height: "100%",
  },
  coverPlaceholder: {
    width: "100%",
    height: "100%",
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#1f2937",
  },
  coverPlaceholderText: {
    color: "#94a3b8",
    fontSize: 22,
    fontWeight: "700",
  },
  rowBody: {
    flex: 1,
    gap: 10,
  },
  rowHeader: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "flex-start",
    gap: 12,
  },
  rowTitle: {
    flex: 1,
    color: "#f8fafc",
    fontSize: 16,
    fontWeight: "700",
  },
  formatBadge: {
    borderRadius: 999,
    borderWidth: 1,
    borderColor: "rgba(20, 184, 166, 0.3)",
    backgroundColor: "rgba(20, 184, 166, 0.12)",
    paddingHorizontal: 10,
    paddingVertical: 4,
  },
  formatBadgeText: {
    color: "#5eead4",
    fontSize: 11,
    fontWeight: "800",
  },
  progressTrack: {
    height: 8,
    borderRadius: 999,
    backgroundColor: "rgba(30, 41, 59, 0.95)",
    overflow: "hidden",
  },
  progressFill: {
    height: "100%",
    borderRadius: 999,
    backgroundColor: "#14b8a6",
  },
  progressText: {
    color: "#94a3b8",
    fontSize: 12,
  },
  secondaryButton: {
    alignSelf: "flex-start",
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "rgba(148, 163, 184, 0.28)",
    backgroundColor: "rgba(15, 23, 42, 0.85)",
    paddingHorizontal: 14,
    paddingVertical: 10,
  },
  secondaryButtonText: {
    color: "#e2e8f0",
    fontSize: 13,
    fontWeight: "700",
  },
  primaryButton: {
    alignSelf: "flex-start",
    borderRadius: 12,
    backgroundColor: "#0f766e",
    paddingHorizontal: 14,
    paddingVertical: 10,
  },
  primaryButtonText: {
    color: "#ffffff",
    fontSize: 13,
    fontWeight: "700",
  },
  errorText: {
    color: "#fecaca",
    fontSize: 12,
    lineHeight: 18,
  },
  downloadMeta: {
    color: "#cbd5e1",
    fontSize: 12,
  },
  groupBlock: {
    gap: 8,
  },
  groupHeader: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
  },
  groupTitle: {
    color: "#e2e8f0",
    fontSize: 14,
    fontWeight: "800",
    letterSpacing: 0.6,
  },
  groupCount: {
    color: "#94a3b8",
    fontSize: 12,
    fontWeight: "700",
  },
  deleteAction: {
    width: 88,
    marginBottom: 12,
    borderRadius: 16,
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    backgroundColor: "#7f1d1d",
  },
  deleteActionText: {
    color: "#fee2e2",
    fontSize: 12,
    fontWeight: "700",
  },
  emptyState: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: 8,
    paddingVertical: 60,
  },
  emptyTitle: {
    color: "#f8fafc",
    fontSize: 18,
    fontWeight: "700",
  },
  emptySubtitle: {
    color: "#94a3b8",
    fontSize: 13,
    textAlign: "center",
  },
});
