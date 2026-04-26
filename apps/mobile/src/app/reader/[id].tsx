import { Pressable, StyleSheet, Text, View } from "react-native";
import { Stack, useLocalSearchParams, useRouter } from "expo-router";

export default function ReaderPlaceholderScreen() {
  const router = useRouter();
  const params = useLocalSearchParams<{ id?: string | string[]; format?: string | string[] }>();
  const bookId = Array.isArray(params.id) ? params.id[0] : params.id;
  const format = Array.isArray(params.format) ? params.format[0] : params.format;

  return (
    <View style={styles.screen}>
      <Stack.Screen options={{ title: "Reader" }} />
      <Text style={styles.message}>Opening reader...</Text>
      {bookId && format ? (
        <Text style={styles.path}>
          {bookId} · {format}
        </Text>
      ) : null}
      <Pressable style={styles.backButton} onPress={() => router.back()}>
        <Text style={styles.backButtonText}>Back</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#0f172a",
    padding: 24,
  },
  message: {
    color: "#e2e8f0",
    fontSize: 18,
    fontWeight: "600",
    textAlign: "center",
  },
  path: {
    marginTop: 12,
    color: "#94a3b8",
    fontSize: 12,
    textAlign: "center",
  },
  backButton: {
    marginTop: 20,
    borderRadius: 999,
    backgroundColor: "#0f766e",
    paddingHorizontal: 18,
    paddingVertical: 10,
  },
  backButtonText: {
    color: "#f8fafc",
    fontWeight: "700",
  },
});
