import "react-native-gesture-handler";

import { useEffect, useState } from "react";
import { ActivityIndicator, StyleSheet, View } from "react-native";
import { Slot, useRootNavigationState, useRouter } from "expo-router";
import { QueryClientProvider } from "@tanstack/react-query";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { getAccessToken, setAuthExpiredHandler } from "../lib/auth";
import { initializeApi } from "../lib/api";
import { runMigrations } from "../lib/db";
import { queryClient } from "../lib/query-client";
import { initializeI18n } from "../i18n";

export default function RootLayout() {
  const router = useRouter();
  const rootNavigationState = useRootNavigationState();
  const [ready, setReady] = useState(false);

  useEffect(() => {
    setAuthExpiredHandler(() => {
      router.replace("/login");
    });

    return () => {
      setAuthExpiredHandler(undefined);
    };
  }, [router]);

  useEffect(() => {
    if (!rootNavigationState?.key) {
      return;
    }

    let cancelled = false;

    void (async () => {
      try {
        await initializeApi();
      } catch {
        // Silent fallback: the app can still render login/library without API hydration.
      }

      try {
        await runMigrations();
      } catch {
        // Silent fallback: Expo Go does not always expose the SQLite methods used in dev.
      }

      try {
        await initializeI18n();
      } catch {
        // Silent fallback: the default English strings remain usable if i18n boot fails.
      }

      const accessToken = await getAccessToken().catch(() => null);

      if (cancelled) {
        return;
      }

      router.replace(accessToken ? "/(tabs)/library" : "/login");
      setReady(true);
    })();

    return () => {
      cancelled = true;
    };
  }, [router, rootNavigationState?.key]);

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <QueryClientProvider client={queryClient}>
          <Slot />
          {!ready ? (
            <View
              style={{
                ...StyleSheet.absoluteFillObject,
                alignItems: "center",
                justifyContent: "center",
                backgroundColor: "#fafafa",
              }}
            >
              <ActivityIndicator color="#0f766e" size="large" />
            </View>
          ) : null}
        </QueryClientProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
