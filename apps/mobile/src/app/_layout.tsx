import "react-native-gesture-handler";

import { useEffect, useState } from "react";
import { ActivityIndicator, View } from "react-native";
import { Slot, useRouter } from "expo-router";
import { QueryClientProvider } from "@tanstack/react-query";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { getAccessToken, setAuthExpiredHandler } from "../lib/auth";
import { initializeApi } from "../lib/api";
import { runMigrations } from "../lib/db";
import { queryClient } from "../lib/query-client";

export default function RootLayout() {
  const router = useRouter();
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
    let cancelled = false;

    void (async () => {
      await initializeApi();
      await runMigrations();
      const accessToken = await getAccessToken();

      if (cancelled) {
        return;
      }

      router.replace(accessToken ? "/(tabs)/library" : "/login");
      setReady(true);
    })();

    return () => {
      cancelled = true;
    };
  }, [router]);

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <QueryClientProvider client={queryClient}>
          {ready ? (
            <Slot />
          ) : (
            <View
              style={{
                flex: 1,
                alignItems: "center",
                justifyContent: "center",
                backgroundColor: "#fafafa",
              }}
            >
              <ActivityIndicator color="#0f766e" size="large" />
            </View>
          )}
        </QueryClientProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
