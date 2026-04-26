/**
 * Profile tab — user account information, reading stats summary, and settings.
 *
 * Displays:
 * - Current user name and email (from `GET /api/v1/auth/me`)
 * - Reading stats summary row (total read, streak, in-progress) from `GET /api/v1/users/me/stats`
 * - Downloads storage row (file count + used bytes from local SQLite)
 * - Server URL field — persisted to Expo SecureStore via `setApiBaseUrl()`
 * - App version from `expo-constants`
 * - Sign-out button
 *
 * The server URL is read from SecureStore on mount so the field reflects
 * any previously saved value. Changes take effect on the next API call.
 *
 * Tapping the stats card navigates to `/stats`.
 * Tapping the downloads row navigates to `/downloads`.
 * Sign-out clears both tokens from SecureStore and navigates to `/login`.
 */
import React, { useEffect, useState } from "react";
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from "react-native";
import Constants from "expo-constants";
import { useQuery } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useTranslation } from "react-i18next";
import { Ionicons } from "@expo/vector-icons";
import type { UserStats } from "@xs/shared";
import { clearTokens } from "../../lib/auth";
import { getApiBaseUrl, useApi, setApiBaseUrl } from "../../lib/api";
import { db } from "../../lib/db";
import { formatBytes, getDownloadSummary } from "../../lib/downloads";

/**
 * Profile tab screen (Expo Router default export for `/(tabs)/profile`).
 *
 * API calls:
 * - `GET /api/v1/auth/me` — fetches current user on mount
 * - `GET /api/v1/users/me/stats` — fetches reading stats on mount
 * - `getDownloadSummary(db)` — queries the local SQLite `local_downloads` table
 */
export default function ProfileTabScreen() {
  const router = useRouter();
  const client = useApi();
  const { t } = useTranslation();
  const [serverUrl, setServerUrl] = useState("");
  const [saving, setSaving] = useState(false);

  const meQuery = useQuery({
    queryKey: ["me"],
    queryFn: () => client.getMe(),
  });

  const statsQuery = useQuery<UserStats>({
    queryKey: ["user-stats"],
    queryFn: () => client.getUserStats(),
  });

  const downloadSummaryQuery = useQuery({
    queryKey: ["downloads", "summary"],
    queryFn: async () => {
      const database = await db;
      return await getDownloadSummary(database);
    },
  });

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const currentUrl = await getApiBaseUrl();
      if (!cancelled) {
        setServerUrl(currentUrl);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  const version = Constants.expoConfig?.version ?? "unknown";

  const saveServerUrl = async () => {
    setSaving(true);
    try {
      await setApiBaseUrl(serverUrl);
    } finally {
      setSaving(false);
    }
  };

  return (
    <ScrollView style={styles.screen} contentContainerStyle={styles.content}>
      <Text style={styles.title}>{t("profile.page_title")}</Text>

      <View style={styles.card}>
        <Text style={styles.cardLabel}>{t("profile.current_user")}</Text>
        {meQuery.data ? (
          <View style={styles.userBlock}>
            <Text testID="profile-username" style={styles.username}>
              {meQuery.data.username}
            </Text>
            <Text style={styles.userMeta}>{meQuery.data.email}</Text>
          </View>
        ) : (
          <Text style={styles.userMeta}>{t("profile.loading_user")}</Text>
        )}
      </View>

      <Pressable
        testID="profile-stats-card"
        style={({ pressed }) => [styles.statsCard, pressed ? styles.statsCardPressed : null]}
        onPress={() => {
          router.push("/stats");
        }}
      >
        <Text style={styles.cardLabel}>{t("nav.reading_stats")}</Text>
        <View style={styles.statsSummaryRow}>
          <View style={styles.statsSummaryItem}>
            <Ionicons name="book-outline" color="#14b8a6" size={18} />
            <Text style={styles.statsSummaryValue}>{statsQuery.data?.total_books_read ?? 0}</Text>
            <Text style={styles.statsSummaryLabel}>{t("stats.books_read")}</Text>
          </View>
          <View style={styles.statsSummaryItem}>
            <Ionicons name="flame-outline" color="#14b8a6" size={18} />
            <Text style={styles.statsSummaryValue}>
              {t("stats.days", { value: statsQuery.data?.reading_streak_days ?? 0 })}
            </Text>
            <Text style={styles.statsSummaryLabel}>{t("stats.streak")}</Text>
          </View>
          <View style={styles.statsSummaryItem}>
            <Ionicons name="time-outline" color="#14b8a6" size={18} />
            <Text style={styles.statsSummaryValue}>{statsQuery.data?.books_in_progress ?? 0}</Text>
            <Text style={styles.statsSummaryLabel}>{t("stats.in_progress")}</Text>
          </View>
        </View>
      </Pressable>

      <Pressable
        testID="profile-downloads-row"
        style={({ pressed }) => [styles.navCard, pressed ? styles.navCardPressed : null]}
        onPress={() => {
          router.push("/downloads");
        }}
      >
        <View style={styles.navCardIcon}>
          <Ionicons name="download-outline" color="#5eead4" size={18} />
        </View>
        <View style={styles.navCardBody}>
          <Text style={styles.navCardTitle}>{t("nav.downloads")}</Text>
          <Text style={styles.navCardSubtitle}>
            {downloadSummaryQuery.data
              ? `${downloadSummaryQuery.data.fileCount} files · ${formatBytes(downloadSummaryQuery.data.usedBytes)}`
              : t("common.loading")}
          </Text>
        </View>
        <Ionicons name="chevron-forward" color="#64748b" size={18} />
      </Pressable>

      <View style={styles.card}>
        <Text style={styles.cardLabel}>{t("profile.server_url")}</Text>
        <TextInput
          testID="server-url-input"
          value={serverUrl}
          onChangeText={setServerUrl}
          autoCapitalize="none"
          autoCorrect={false}
          placeholder="http://localhost:8080"
          placeholderTextColor="#a1a1aa"
          style={styles.input}
        />
        <Pressable
          testID="server-url-save"
          style={[styles.primaryButton, saving ? styles.primaryButtonDisabled : null]}
          onPress={() => {
            void saveServerUrl();
          }}
          disabled={saving}
        >
          <Text style={styles.primaryButtonText}>{saving ? t("common.saving") : t("common.save")}</Text>
        </Pressable>
      </View>

      <View style={styles.card}>
        <Text style={styles.cardLabel}>{t("profile.app_version")}</Text>
        <Text style={styles.versionText}>{version}</Text>
      </View>

      <Pressable
        testID="sign-out"
        style={styles.signOutButton}
        onPress={() => {
          void clearTokens().then(() => {
            router.replace("/login");
          });
        }}
      >
        <Text style={styles.signOutText}>{t("common.sign_out")}</Text>
      </Pressable>
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
  title: {
    color: "#f8fafc",
    fontSize: 28,
    fontWeight: "700",
    marginTop: 8,
  },
  card: {
    borderRadius: 16,
    backgroundColor: "#111827",
    borderWidth: 1,
    borderColor: "rgba(148, 163, 184, 0.2)",
    padding: 14,
    gap: 10,
  },
  statsCard: {
    borderRadius: 16,
    backgroundColor: "#0f172a",
    borderWidth: 1,
    borderColor: "rgba(20, 184, 166, 0.35)",
    padding: 14,
    gap: 10,
  },
  statsCardPressed: {
    opacity: 0.82,
  },
  navCard: {
    borderRadius: 16,
    backgroundColor: "#111827",
    borderWidth: 1,
    borderColor: "rgba(148, 163, 184, 0.2)",
    padding: 14,
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
  },
  navCardPressed: {
    opacity: 0.82,
  },
  navCardIcon: {
    width: 34,
    height: 34,
    borderRadius: 17,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "rgba(94, 234, 212, 0.12)",
  },
  navCardBody: {
    flex: 1,
    gap: 2,
  },
  navCardTitle: {
    color: "#f8fafc",
    fontSize: 15,
    fontWeight: "700",
  },
  navCardSubtitle: {
    color: "#94a3b8",
    fontSize: 12,
  },
  cardLabel: {
    color: "#cbd5e1",
    fontSize: 12,
    textTransform: "uppercase",
    letterSpacing: 0.6,
    fontWeight: "700",
  },
  userBlock: {
    gap: 4,
  },
  username: {
    color: "#f8fafc",
    fontSize: 18,
    fontWeight: "700",
  },
  userMeta: {
    color: "#94a3b8",
    fontSize: 13,
  },
  statsSummaryRow: {
    flexDirection: "row",
    gap: 10,
  },
  statsSummaryItem: {
    flex: 1,
    borderRadius: 14,
    backgroundColor: "rgba(15, 23, 42, 0.85)",
    borderWidth: 1,
    borderColor: "rgba(20, 184, 166, 0.18)",
    paddingVertical: 12,
    paddingHorizontal: 10,
    alignItems: "center",
    gap: 4,
  },
  statsSummaryValue: {
    color: "#f8fafc",
    fontSize: 18,
    fontWeight: "700",
  },
  statsSummaryLabel: {
    color: "#94a3b8",
    fontSize: 11,
    textAlign: "center",
  },
  input: {
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#334155",
    backgroundColor: "#020617",
    color: "#f8fafc",
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 14,
  },
  primaryButton: {
    borderRadius: 12,
    backgroundColor: "#0f766e",
    paddingVertical: 11,
    alignItems: "center",
  },
  primaryButtonDisabled: {
    opacity: 0.65,
  },
  primaryButtonText: {
    color: "#ffffff",
    fontSize: 14,
    fontWeight: "700",
  },
  versionText: {
    color: "#e2e8f0",
    fontSize: 14,
    fontWeight: "600",
  },
  signOutButton: {
    marginTop: 8,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#ef4444",
    backgroundColor: "rgba(127, 29, 29, 0.3)",
    paddingVertical: 11,
    alignItems: "center",
  },
  signOutText: {
    color: "#fecaca",
    fontWeight: "700",
    fontSize: 14,
  },
});
