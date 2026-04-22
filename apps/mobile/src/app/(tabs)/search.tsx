import { StyleSheet, Text, View } from "react-native";
import { useTranslation } from "react-i18next";

export default function SearchTabScreen() {
  const { t } = useTranslation();
  return (
    <View style={styles.container}>
      <Text style={styles.title}>{t("search.page_title")}</Text>
      <Text style={styles.subtitle}>{t("search.coming_later")}</Text>
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
