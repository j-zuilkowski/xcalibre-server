import React, { useEffect, useState } from "react";
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from "react-native";
import Constants from "expo-constants";
import { useQuery } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { clearTokens } from "../../lib/auth";
import { getApiBaseUrl, useApi, setApiBaseUrl } from "../../lib/api";

export default function ProfileTabScreen() {
  const router = useRouter();
  const client = useApi();
  const [serverUrl, setServerUrl] = useState("");
  const [saving, setSaving] = useState(false);

  const meQuery = useQuery({
    queryKey: ["me"],
    queryFn: () => client.getMe(),
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
      <Text style={styles.title}>Profile</Text>

      <View style={styles.card}>
        <Text style={styles.cardLabel}>Current user</Text>
        {meQuery.data ? (
          <View style={styles.userBlock}>
            <Text testID="profile-username" style={styles.username}>
              {meQuery.data.username}
            </Text>
            <Text style={styles.userMeta}>{meQuery.data.email}</Text>
          </View>
        ) : (
          <Text style={styles.userMeta}>Loading user…</Text>
        )}
      </View>

      <View style={styles.card}>
        <Text style={styles.cardLabel}>Server URL</Text>
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
          <Text style={styles.primaryButtonText}>{saving ? "Saving…" : "Save"}</Text>
        </Pressable>
      </View>

      <View style={styles.card}>
        <Text style={styles.cardLabel}>App version</Text>
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
        <Text style={styles.signOutText}>Sign Out</Text>
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
