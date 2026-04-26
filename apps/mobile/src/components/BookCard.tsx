import { useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { Image } from "expo-image";
import { useRouter } from "expo-router";
import { useTranslation } from "react-i18next";
import type { BookSummary } from "@xs/shared";
import { useApi } from "../lib/api";

type BookCardProps = {
  book: BookSummary;
  downloaded?: boolean;
};

export function BookCard({ book, downloaded = false }: BookCardProps) {
  const router = useRouter();
  const client = useApi();
  const { t } = useTranslation();
  const [imageFailed, setImageFailed] = useState(false);

  const primaryAuthor = book.authors[0]?.name ?? t("common.unknown_author");
  const hasCover = book.has_cover && !imageFailed;
  const coverUri = book.cover_url ?? (book.has_cover ? client.coverUrl(book.id) : null);

  return (
    <Pressable
      testID={`book-card-${book.id}`}
      style={styles.card}
      onPress={() => {
        router.push(`/book/${encodeURIComponent(book.id)}`);
      }}
    >
      <View style={styles.coverFrame}>
        {hasCover && coverUri ? (
          <Image
            source={{ uri: coverUri }}
            cachePolicy="memory-disk"
            contentFit="cover"
            style={styles.coverImage}
            onError={() => setImageFailed(true)}
          />
        ) : (
          <View style={styles.coverPlaceholder}>
            <Text style={styles.coverPlaceholderText}>
              {book.title.trim().charAt(0).toUpperCase() || "?"}
            </Text>
          </View>
        )}
        {downloaded ? (
          <View style={styles.downloadedBadge}>
            <Text style={styles.downloadedBadgeText}>Downloaded</Text>
          </View>
        ) : null}
      </View>
      <Text numberOfLines={2} style={styles.title}>
        {book.title}
      </Text>
      <Text numberOfLines={1} style={styles.author}>
        {primaryAuthor}
      </Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  card: {
    flex: 1,
    marginBottom: 14,
  },
  coverFrame: {
    width: "100%",
    aspectRatio: 2 / 3,
    borderRadius: 10,
    overflow: "hidden",
    backgroundColor: "#e4e4e7",
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
    backgroundColor: "#e4e4e7",
  },
  coverPlaceholderText: {
    color: "#71717a",
    fontSize: 38,
    fontWeight: "700",
  },
  downloadedBadge: {
    position: "absolute",
    right: 8,
    top: 8,
    borderRadius: 999,
    backgroundColor: "#0f766e",
    paddingHorizontal: 10,
    paddingVertical: 4,
  },
  downloadedBadgeText: {
    color: "#f8fafc",
    fontSize: 10,
    fontWeight: "700",
    textTransform: "uppercase",
  },
  title: {
    marginTop: 8,
    color: "#18181b",
    fontSize: 14,
    fontWeight: "600",
  },
  author: {
    marginTop: 2,
    color: "#71717a",
    fontSize: 12,
  },
});
