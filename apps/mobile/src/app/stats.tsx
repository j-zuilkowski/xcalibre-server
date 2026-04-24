import React from "react";
import { ScrollView, StyleSheet, Text, View } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { Stack } from "expo-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { UserStats } from "@autolibre/shared";
import { useApi } from "../lib/api";

function formatPercent(value: number, t: (key: string) => string): string {
  return `${value.toFixed(1)} ${t("stats.pp")}`;
}

export default function StatsScreen() {
  const { t } = useTranslation();
  const client = useApi();

  const statsQuery = useQuery<UserStats>({
    queryKey: ["user-stats"],
    queryFn: () => client.getUserStats(),
  });

  const stats = statsQuery.data;
  const formatEntries = stats
    ? Object.entries(stats.formats_read).sort((left, right) => {
        const countDelta = right[1] - left[1];
        if (countDelta !== 0) {
          return countDelta;
        }
        return left[0].localeCompare(right[0]);
      })
    : [];

  return (
    <ScrollView style={styles.screen} contentContainerStyle={styles.content}>
      <Stack.Screen
        options={{
          title: t("stats.page_title"),
        }}
      />

      <View style={styles.header}>
        <Text style={styles.kicker}>{t("nav.reading_stats")}</Text>
        <Text style={styles.title}>{t("stats.page_title")}</Text>
      </View>

      {statsQuery.isLoading ? (
        <View style={styles.messageCard}>
          <Text style={styles.messageText}>{t("common.loading")}</Text>
        </View>
      ) : statsQuery.isError || !stats ? (
        <View style={styles.messageCardError}>
          <Text style={styles.messageTextError}>{t("stats.unable_to_load")}</Text>
        </View>
      ) : (
        <>
          <View style={styles.grid}>
            <View style={styles.statCard}>
              <Ionicons name="book-outline" size={18} color="#14b8a6" />
              <Text style={styles.statValue}>{stats.total_books_read}</Text>
              <Text style={styles.statLabel}>{t("stats.books_read")}</Text>
            </View>
            <View style={styles.statCard}>
              <Ionicons name="calendar-outline" size={18} color="#14b8a6" />
              <Text style={styles.statValue}>{stats.books_read_this_year}</Text>
              <Text style={styles.statLabel}>{t("stats.this_year")}</Text>
            </View>
            <View style={styles.statCard}>
              <Ionicons name="flame-outline" size={18} color="#14b8a6" />
              <Text style={styles.statValue}>{t("stats.days", { value: stats.reading_streak_days })}</Text>
              <Text style={styles.statLabel}>{t("stats.streak")}</Text>
            </View>
            <View style={styles.statCard}>
              <Ionicons name="time-outline" size={18} color="#14b8a6" />
              <Text style={styles.statValue}>{stats.books_in_progress}</Text>
              <Text style={styles.statLabel}>{t("stats.in_progress")}</Text>
            </View>
          </View>

          <View style={styles.summaryCard}>
            <Text style={styles.summaryText}>
              {t("stats.total_sessions")}: {stats.total_reading_sessions}
            </Text>
            <Text style={styles.summaryText}>
              {t("stats.average_progress")}: {formatPercent(stats.average_progress_per_session, t)}
            </Text>
          </View>

          <View style={styles.sectionCard}>
            <Text style={styles.sectionTitle}>{t("stats.top_authors")}</Text>
            {stats.top_authors.slice(0, 3).length > 0 ? (
              stats.top_authors.slice(0, 3).map((author, index) => (
                <View key={author.name} style={styles.listItem}>
                  <View style={styles.rankBubble}>
                    <Text style={styles.rankText}>{index + 1}</Text>
                  </View>
                  <Text numberOfLines={1} style={styles.listName}>
                    {author.name}
                  </Text>
                  <Text style={styles.listCount}>{author.count}</Text>
                </View>
              ))
            ) : (
              <Text style={styles.emptyText}>{t("common.none")}</Text>
            )}
          </View>

          <View style={styles.sectionCard}>
            <Text style={styles.sectionTitle}>{t("stats.top_tags")}</Text>
            {stats.top_tags.slice(0, 3).length > 0 ? (
              stats.top_tags.slice(0, 3).map((tag, index) => (
                <View key={tag.name} style={styles.listItem}>
                  <View style={styles.rankBubble}>
                    <Text style={styles.rankText}>{index + 1}</Text>
                  </View>
                  <Text numberOfLines={1} style={styles.listName}>
                    {tag.name}
                  </Text>
                  <Text style={styles.listCount}>{tag.count}</Text>
                </View>
              ))
            ) : (
              <Text style={styles.emptyText}>{t("common.none")}</Text>
            )}
          </View>

          <View style={styles.sectionCard}>
            <Text style={styles.sectionTitle}>{t("stats.formats_breakdown")}</Text>
            <Text style={styles.formatsText}>
              {formatEntries.length > 0
                ? formatEntries.map(([format, count]) => `${format.toUpperCase()}: ${count}`).join(", ")
                : t("common.none")}
            </Text>
          </View>
        </>
      )}
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
    marginBottom: 2,
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
  grid: {
    flexDirection: "row",
    flexWrap: "wrap",
    justifyContent: "space-between",
    gap: 12,
  },
  statCard: {
    width: "48%",
    borderRadius: 16,
    backgroundColor: "#111827",
    borderWidth: 1,
    borderColor: "rgba(20, 184, 166, 0.28)",
    padding: 14,
    gap: 6,
    alignItems: "center",
  },
  statValue: {
    color: "#f8fafc",
    fontSize: 22,
    fontWeight: "700",
    marginTop: 2,
    textAlign: "center",
  },
  statLabel: {
    color: "#94a3b8",
    fontSize: 12,
    fontWeight: "600",
    textAlign: "center",
  },
  summaryCard: {
    borderRadius: 16,
    backgroundColor: "#111827",
    borderWidth: 1,
    borderColor: "rgba(148, 163, 184, 0.2)",
    padding: 14,
    gap: 6,
  },
  summaryText: {
    color: "#cbd5e1",
    fontSize: 13,
    fontWeight: "600",
  },
  sectionCard: {
    borderRadius: 16,
    backgroundColor: "#111827",
    borderWidth: 1,
    borderColor: "rgba(148, 163, 184, 0.2)",
    padding: 14,
    gap: 10,
  },
  sectionTitle: {
    color: "#f8fafc",
    fontSize: 16,
    fontWeight: "700",
  },
  listItem: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  rankBubble: {
    width: 26,
    height: 26,
    borderRadius: 13,
    backgroundColor: "rgba(20, 184, 166, 0.16)",
    alignItems: "center",
    justifyContent: "center",
  },
  rankText: {
    color: "#5eead4",
    fontSize: 12,
    fontWeight: "700",
  },
  listName: {
    flex: 1,
    color: "#e2e8f0",
    fontSize: 14,
    fontWeight: "600",
  },
  listCount: {
    color: "#94a3b8",
    fontSize: 13,
    fontWeight: "700",
  },
  emptyText: {
    color: "#94a3b8",
    fontSize: 13,
  },
  formatsText: {
    color: "#e2e8f0",
    fontSize: 14,
    lineHeight: 20,
  },
  messageCard: {
    borderRadius: 16,
    backgroundColor: "#111827",
    borderWidth: 1,
    borderColor: "rgba(148, 163, 184, 0.2)",
    padding: 14,
  },
  messageCardError: {
    borderRadius: 16,
    backgroundColor: "rgba(127, 29, 29, 0.35)",
    borderWidth: 1,
    borderColor: "#7f1d1d",
    padding: 14,
  },
  messageText: {
    color: "#cbd5e1",
    fontSize: 14,
  },
  messageTextError: {
    color: "#fecaca",
    fontSize: 14,
    fontWeight: "600",
  },
});
