import { Pressable, StyleSheet, Text, View } from "react-native";
import { useRouter } from "expo-router";
import { clearTokens } from "../../lib/auth";

export default function ProfileTabScreen() {
  const router = useRouter();

  return (
    <View style={styles.container}>
      <Text style={styles.title}>Profile</Text>
      <Pressable
        style={styles.signOutButton}
        onPress={() => {
          void clearTokens().then(() => {
            router.replace("/login");
          });
        }}
      >
        <Text style={styles.signOutText}>Sign Out</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#fafafa",
    gap: 12,
  },
  title: {
    fontSize: 22,
    fontWeight: "700",
    color: "#18181b",
  },
  signOutButton: {
    backgroundColor: "#0f766e",
    borderRadius: 10,
    paddingHorizontal: 16,
    paddingVertical: 10,
  },
  signOutText: {
    color: "#ffffff",
    fontWeight: "600",
  },
});
