import { StyleSheet, Text, View } from "react-native";

export default function SearchTabScreen() {
  return (
    <View style={styles.container}>
      <Text style={styles.title}>Search</Text>
      <Text style={styles.subtitle}>Semantic + keyword search is coming in a later stage.</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#fafafa",
    padding: 24,
  },
  title: {
    fontSize: 22,
    fontWeight: "700",
    color: "#18181b",
    marginBottom: 8,
  },
  subtitle: {
    textAlign: "center",
    color: "#71717a",
    fontSize: 14,
  },
});
