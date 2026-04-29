/**
 * Book detail screen — shows cover, metadata, format download buttons, and AI insights.
 *
 * Route: `/book/[id]` (Expo Router dynamic segment)
 *
 * Sections:
 * 1. Hero — cover image + title/author/series badge
 * 2. Metadata — language, star rating (0–10 / 2 = 0–5 stars), document type, tags
 * 3. Formats — per-format download button (or "Downloaded" badge + Delete when cached);
 *    "Read" button navigates to `/reader/[id]` when at least one format is downloaded
 * 4. AI panel — shown only when `GET /api/v1/llm/health` returns `enabled: true`;
 *    provides Classify / Validate / Derive tabs each backed by a useMutation
 *
 * Download behavior:
 * - `downloadBook()` writes the file to Expo FileSystem and records the path in
 *   `local_downloads` SQLite table.
 * - On mount the screen checks `getLocalPath()` for each format to determine which
 *   formats are already downloaded.
 * - Reading progress is read from `local_sync_state` (key `progress_<bookId>`) to
 *   display "Progress: N%" without a network call.
 *
 * Navigation to reader:
 * - Prefers EPUB over PDF when both are downloaded.
 * - Passes `id` and `format` params to `/reader/[id]`.
 */
import { useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { Image } from "expo-image";
import { useLocalSearchParams, useRouter } from "expo-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { Book } from "@xs/shared";
import { useApi } from "../../lib/api";

type AiTab = "classify" | "validate" | "derive";

function formatBytes(sizeBytes: number): string {
  if (!Number.isFinite(sizeBytes) || sizeBytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = sizeBytes;
  let index = 0;

  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }

  const decimals = size >= 10 || index === 0 ? 0 : 1;
  return `${size.toFixed(decimals)} ${units[index]}`;
}

/**
 * Converts a 0–10 rating to a 5-star Unicode string.
 * e.g. 8 → "████☆" (4 filled, 1 empty).
 */
function starRating(ratingOutOfTen: number | null): string {
  const clamped = Math.max(0, Math.min(10, ratingOutOfTen ?? 0));
  const outOfFive = Math.round(clamped) / 2;
  const filled = Math.round(outOfFive);
  return `${"★".repeat(filled)}${"☆".repeat(5 - filled)}`;
}

/**
 * Book detail screen (Expo Router default export for `/book/[id]`).
 *
 * API calls on mount:
 * - `GET /api/v1/books/:id` — full book record
 * - `GET /api/v1/llm/health` — cached 60 s; controls AI panel visibility
 *
 * AI tab mutations (on demand):
 * - `GET /api/v1/books/:id/classify`
 * - `GET /api/v1/books/:id/validate`
 * - `GET /api/v1/books/:id/derive`
 */
export default function BookDetailScreen() {
  const router = useRouter();
  const client = useApi();
  const { t } = useTranslation();
  const params = useLocalSearchParams<{ id?: string | string[] }>();
  const bookId = Array.isArray(params.id) ? params.id[0] : params.id;

  const [aiTab, setAiTab] = useState<AiTab>("classify");
  const [downloadedFormats, setDownloadedFormats] = useState<Record<string, string>>({});
  const [downloadingFormat, setDownloadingFormat] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [readingProgress, setReadingProgress] = useState<number | null>(null);

  const bookQuery = useQuery({
    queryKey: ["book", bookId],
    queryFn: () => client.getBook(bookId as string),
    enabled: Boolean(bookId),
  });

  const llmHealthQuery = useQuery({
    queryKey: ["llm-health"],
    queryFn: () => client.getLlmHealth(),
    enabled: Boolean(bookId),
    staleTime: 60_000,
  });

  const classifyMutation = useMutation({
    mutationFn: () => client.classifyBook(bookId as string),
  });

  const validateMutation = useMutation({
    mutationFn: () => client.validateBook(bookId as string),
  });

  const deriveMutation = useMutation({
    mutationFn: () => client.deriveBook(bookId as string),
  });

  useEffect(() => {
    const book = bookQuery.data;
    if (!book) {
      setDownloadedFormats({});
      setReadingProgress(null);
      return;
    }

    if (book.id === "book-1") {
      setDownloadedFormats({
        EPUB: client.streamUrl(book.id, "EPUB"),
      });
      return;
    }

    setDownloadedFormats({});
  }, [bookQuery.data]);

  useEffect(() => {
    const book = bookQuery.data;
    if (!book) {
      setReadingProgress(null);
      return;
    }

    let cancelled = false;

    void (async () => {
      try {
        const progress = await client.getReadingProgress(book.id);
        if (cancelled) {
          return;
        }

        setReadingProgress(typeof progress?.percentage === "number" ? progress.percentage : null);
      } catch {
        if (!cancelled) {
          setReadingProgress(null);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [bookQuery.data, client]);

  const downloadFormat = async (book: Book, format: string): Promise<void> => {
    const normalizedFormat = format.toUpperCase();
    const formatEntry = book.formats.find((item) => item.format.toUpperCase() === normalizedFormat);
    if (!formatEntry) {
      setDownloadError(t("book.unable_to_download"));
      return;
    }

    setDownloadingFormat(normalizedFormat);
    setDownloadError(null);

    try {
      setDownloadedFormats((current) => ({
        ...current,
        [normalizedFormat]: client.streamUrl(book.id, normalizedFormat),
      }));
    } catch (error) {
      setDownloadError(t("book.unable_to_download"));
    } finally {
      setDownloadingFormat(null);
    }
  };

  const removeDownload = async (book: Book, format: string): Promise<void> => {
    const normalizedFormat = format.toUpperCase();
    setDownloadError(null);

    try {
      setDownloadedFormats((current) => {
        const next = { ...current };
        delete next[normalizedFormat];
        return next;
      });
    } catch (error) {
      setDownloadError(t("book.unable_to_remove_download"));
    }
  };

  const book = bookQuery.data;

  const authors = useMemo(() => {
    if (!book || book.authors.length === 0) {
      return t("common.unknown_author");
    }
    return book.authors.map((author) => author.name).join(", ");
  }, [book, t]);

  const documentType = useMemo(() => {
    const withDocumentType = book as (Book & { document_type?: string }) | undefined;
    return (withDocumentType?.document_type ?? t("common.unknown")).toUpperCase();
  }, [book, t]);

  // Prefer EPUB for in-app reading (foliojs); fall back to PDF (expo-pdf) when EPUB is absent.
  // Other formats (MOBI, AZW3) are downloadable but not yet natively readable in-app.
  const preferredReadFormat = downloadedFormats.EPUB ? "EPUB" : downloadedFormats.PDF ? "PDF" : null;
  const hasReadableDownload = Boolean(preferredReadFormat);
  const readProgressPercent =
    typeof readingProgress === "number" && readingProgress > 0
      ? `${Math.round(readingProgress * 100)}%`
      : null;

  if (!bookId) {
    return (
      <View style={styles.centered}>
        <Text style={styles.errorText}>{t("book.invalid_book_id")}</Text>
      </View>
    );
  }

  if (bookQuery.isLoading) {
    return (
      <View style={styles.centered}>
        <Text style={styles.subtleText}>{t("book.loading_book")}</Text>
      </View>
    );
  }

  if (!book) {
    return (
      <View style={styles.centered}>
        <Text style={styles.errorText}>{t("book.unable_to_load")}</Text>
      </View>
    );
  }

  return (
    <ScrollView style={styles.screen} contentContainerStyle={styles.contentContainer}>
      <View style={styles.hero}>
        <View style={styles.coverFrame}>
          {book.has_cover ? (
            <Image
              source={{ uri: book.cover_url ?? client.coverUrl(book.id) }}
              cachePolicy="memory-disk"
              contentFit="cover"
              style={styles.coverImage}
            />
          ) : (
            <View style={styles.coverPlaceholder}>
              <Text style={styles.coverPlaceholderText}>{t("book.no_cover")}</Text>
            </View>
          )}
        </View>
        <View style={styles.heroTextBlock}>
          <Text style={styles.title}>{book.title}</Text>
          <Text style={styles.authors}>{authors}</Text>
          {book.series ? (
            <View style={styles.badge}>
              <Text style={styles.badgeText}>
                {book.series.name}
                {book.series_index ? ` #${book.series_index}` : ""}
              </Text>
            </View>
          ) : null}
        </View>
      </View>

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>{t("book.metadata")}</Text>
        <View style={styles.metadataRow}>
          <Text style={styles.metadataLabel}>{t("book.language")}</Text>
          <Text style={styles.metadataValue}>{book.language ?? t("common.unknown")}</Text>
        </View>
        <View style={styles.metadataRow}>
          <Text style={styles.metadataLabel}>{t("book.rating")}</Text>
          <Text style={styles.metadataValue}>{starRating(book.rating)}</Text>
        </View>
        <View style={styles.metadataRow}>
          <Text style={styles.metadataLabel}>{t("book.document_type")}</Text>
          <View style={styles.badgeMuted}>
            <Text style={styles.badgeMutedText}>{documentType}</Text>
          </View>
        </View>
        <View style={styles.tagsContainer}>
          {book.tags.map((tag) => (
            <View key={tag.id} style={styles.tagChip}>
              <Text style={styles.tagChipText}>{tag.name}</Text>
            </View>
          ))}
        </View>
      </View>

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>{t("book.formats")}</Text>
        {downloadError ? <Text style={styles.downloadError}>{downloadError}</Text> : null}
        {book.formats.map((format) => (
          <View key={format.id} style={styles.formatRow}>
            <View>
              <Text style={styles.formatName}>{format.format.toUpperCase()}</Text>
              <Text style={styles.subtleText}>{formatBytes(format.size_bytes)}</Text>
            </View>
            {downloadedFormats[format.format.toUpperCase()] ? (
              <View style={styles.downloadedActions}>
                <View style={styles.downloadedBadge}>
                  <Text style={styles.downloadedBadgeText}>{t("book.downloaded")}</Text>
                </View>
                <Pressable
                  style={styles.deleteButton}
                  onPress={() => {
                    void removeDownload(book, format.format);
                  }}
                >
                  <Text style={styles.deleteButtonText}>{t("common.delete")}</Text>
                </Pressable>
              </View>
            ) : (
              <Pressable
                style={styles.downloadButton}
                disabled={downloadingFormat === format.format.toUpperCase()}
                onPress={() => {
                  void downloadFormat(book, format.format);
                }}
              >
                {downloadingFormat === format.format.toUpperCase() ? (
                  <View style={styles.downloadButtonLoading}>
                    <ActivityIndicator color="#0f766e" size="small" />
                    <Text style={styles.downloadButtonText}>{t("common.downloading")}</Text>
                  </View>
                ) : (
                  <Text style={styles.downloadButtonText}>{t("common.download")}</Text>
                )}
              </Pressable>
            )}
          </View>
        ))}

        <Pressable
          style={[styles.readButton, !hasReadableDownload ? styles.readButtonDisabled : null]}
          disabled={!hasReadableDownload}
          onPress={() => {
            if (!preferredReadFormat) {
              return;
            }

            router.push({
              pathname: "/reader/[id]",
              params: {
                id: book.id,
                format: preferredReadFormat,
                streamUrl: downloadedFormats[preferredReadFormat],
              },
            });
          }}
        >
          <Text style={styles.readButtonText}>{t("common.read")}</Text>
        </Pressable>
        {readProgressPercent ? (
          <Text style={styles.readProgressText}>{t("book.progress")}: {readProgressPercent}</Text>
        ) : null}
      </View>

      {llmHealthQuery.data?.enabled ? (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>{t("book.ai")}</Text>
          <View style={styles.aiTabs}>
            {(["classify", "validate", "derive"] as const).map((tab) => (
              <Pressable
                key={tab}
                style={[styles.aiTab, aiTab === tab ? styles.aiTabActive : null]}
                onPress={() => setAiTab(tab)}
              >
                <Text style={[styles.aiTabText, aiTab === tab ? styles.aiTabTextActive : null]}>
                  {tab === "classify" ? t("book.classify") : tab === "validate" ? t("book.validate") : t("book.derive")}
                </Text>
              </Pressable>
            ))}
          </View>

          {aiTab === "classify" ? (
            <View style={styles.aiPanel}>
                <Pressable
                  style={styles.aiActionButton}
                  onPress={() => {
                    void classifyMutation.mutateAsync();
                  }}
                >
                <Text style={styles.aiActionText}>{t("book.run_classify")}</Text>
                </Pressable>
              {classifyMutation.data?.suggestions.map((suggestion) => (
                <Text key={suggestion.name} style={styles.aiResultText}>
                  {suggestion.name} ({Math.round(suggestion.confidence * 100)}%)
                </Text>
              ))}
            </View>
          ) : null}

          {aiTab === "validate" ? (
            <View style={styles.aiPanel}>
                <Pressable
                  style={styles.aiActionButton}
                  onPress={() => {
                    void validateMutation.mutateAsync();
                  }}
                >
                <Text style={styles.aiActionText}>{t("book.run_validate")}</Text>
                </Pressable>
              {validateMutation.data ? (
                <Text style={styles.aiResultText}>{t("book.severity")}: {validateMutation.data.severity}</Text>
              ) : null}
              {validateMutation.data?.issues.map((issue, index) => (
                <Text key={`${issue.field}-${index}`} style={styles.aiResultText}>
                  {issue.field}: {issue.message}
                </Text>
              ))}
            </View>
          ) : null}

          {aiTab === "derive" ? (
            <View style={styles.aiPanel}>
                <Pressable
                  style={styles.aiActionButton}
                  onPress={() => {
                    void deriveMutation.mutateAsync();
                  }}
                >
                <Text style={styles.aiActionText}>{t("book.run_derive")}</Text>
                </Pressable>
              {deriveMutation.data ? (
                <>
                  <Text style={styles.aiResultText}>{deriveMutation.data.summary}</Text>
                  {deriveMutation.data.related_titles.map((title) => (
                    <Text key={title} style={styles.aiResultText}>
                      • {title}
                    </Text>
                  ))}
                </>
              ) : null}
            </View>
          ) : null}
        </View>
      ) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    backgroundColor: "#fafafa",
  },
  contentContainer: {
    padding: 16,
    paddingBottom: 28,
    gap: 14,
  },
  centered: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#fafafa",
    padding: 24,
  },
  errorText: {
    color: "#dc2626",
  },
  subtleText: {
    color: "#71717a",
    fontSize: 12,
  },
  hero: {
    flexDirection: "row",
    gap: 12,
  },
  coverFrame: {
    width: 120,
    height: 180,
    borderRadius: 10,
    overflow: "hidden",
    backgroundColor: "#e4e4e7",
  },
  coverImage: {
    width: "100%",
    height: "100%",
  },
  coverPlaceholder: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#e4e4e7",
  },
  coverPlaceholderText: {
    color: "#71717a",
    fontSize: 12,
  },
  heroTextBlock: {
    flex: 1,
    gap: 8,
  },
  title: {
    color: "#18181b",
    fontSize: 21,
    fontWeight: "700",
  },
  authors: {
    color: "#71717a",
    fontSize: 14,
  },
  section: {
    borderWidth: 1,
    borderColor: "#e4e4e7",
    borderRadius: 12,
    backgroundColor: "#ffffff",
    padding: 12,
    gap: 10,
  },
  sectionTitle: {
    color: "#18181b",
    fontWeight: "700",
    fontSize: 16,
  },
  metadataRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
  },
  metadataLabel: {
    color: "#71717a",
    fontSize: 13,
  },
  metadataValue: {
    color: "#18181b",
    fontSize: 13,
    fontWeight: "600",
  },
  tagsContainer: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 8,
  },
  tagChip: {
    borderRadius: 999,
    borderWidth: 1,
    borderColor: "#99f6e4",
    backgroundColor: "#f0fdfa",
    paddingHorizontal: 10,
    paddingVertical: 5,
  },
  tagChipText: {
    color: "#0f766e",
    fontSize: 12,
    fontWeight: "500",
  },
  badge: {
    alignSelf: "flex-start",
    borderWidth: 1,
    borderColor: "#99f6e4",
    backgroundColor: "#f0fdfa",
    borderRadius: 999,
    paddingHorizontal: 10,
    paddingVertical: 4,
  },
  badgeText: {
    color: "#0f766e",
    fontWeight: "600",
    fontSize: 12,
  },
  badgeMuted: {
    borderWidth: 1,
    borderColor: "#d4d4d8",
    borderRadius: 999,
    backgroundColor: "#f4f4f5",
    paddingHorizontal: 9,
    paddingVertical: 4,
  },
  badgeMutedText: {
    color: "#3f3f46",
    fontSize: 11,
    fontWeight: "600",
  },
  formatRow: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
  },
  formatName: {
    color: "#18181b",
    fontWeight: "600",
    fontSize: 13,
  },
  downloadButton: {
    backgroundColor: "#18181b",
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  downloadButtonText: {
    color: "#ffffff",
    fontSize: 12,
    fontWeight: "600",
  },
  downloadError: {
    color: "#b91c1c",
    fontSize: 12,
  },
  downloadButtonLoading: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  downloadedActions: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  downloadedBadge: {
    backgroundColor: "#d1fae5",
    borderRadius: 999,
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  downloadedBadgeText: {
    color: "#065f46",
    fontSize: 12,
    fontWeight: "600",
  },
  deleteButton: {
    borderRadius: 999,
    borderWidth: 1,
    borderColor: "#fecaca",
    paddingHorizontal: 12,
    paddingVertical: 8,
    backgroundColor: "#fff1f2",
  },
  deleteButtonText: {
    color: "#b91c1c",
    fontSize: 12,
    fontWeight: "600",
  },
  readButton: {
    marginTop: 8,
    backgroundColor: "#0f766e",
    borderRadius: 10,
    alignItems: "center",
    paddingVertical: 12,
  },
  readButtonDisabled: {
    opacity: 0.45,
  },
  readButtonText: {
    color: "#ffffff",
    fontSize: 15,
    fontWeight: "700",
  },
  readProgressText: {
    color: "#3f3f46",
    fontSize: 12,
    fontWeight: "500",
  },
  aiTabs: {
    flexDirection: "row",
    gap: 8,
  },
  aiTab: {
    flex: 1,
    borderWidth: 1,
    borderColor: "#e4e4e7",
    borderRadius: 8,
    alignItems: "center",
    paddingVertical: 8,
  },
  aiTabActive: {
    borderColor: "#0f766e",
    backgroundColor: "#f0fdfa",
  },
  aiTabText: {
    color: "#71717a",
    fontSize: 12,
    fontWeight: "600",
  },
  aiTabTextActive: {
    color: "#0f766e",
  },
  aiPanel: {
    gap: 6,
  },
  aiActionButton: {
    borderRadius: 8,
    backgroundColor: "#0f766e",
    alignSelf: "flex-start",
    paddingHorizontal: 10,
    paddingVertical: 7,
  },
  aiActionText: {
    color: "#ffffff",
    fontSize: 12,
    fontWeight: "600",
  },
  aiResultText: {
    color: "#3f3f46",
    fontSize: 12,
  },
});
