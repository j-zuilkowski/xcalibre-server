import React from "react";
import { Text, View } from "react-native";

const PLACEHOLDER_COLORS = [
  "#27272a",
  "#3f3f46",
  "#52525b",
  "#1f2937",
  "#134e4a",
  "#0f766e",
  "#155e75",
  "#374151",
];

export function hashTitleToColorIndex(title: string): number {
  let hash = 0;
  for (let index = 0; index < title.length; index += 1) {
    hash = (hash * 31 + title.charCodeAt(index)) | 0;
  }
  return Math.abs(hash) % PLACEHOLDER_COLORS.length;
}

type CoverPlaceholderProps = {
  title: string;
};

export function CoverPlaceholder({ title }: CoverPlaceholderProps) {
  const trimmed = title.trim();
  const firstLetter = (trimmed[0] ?? "?").toUpperCase();
  const colorIndex = hashTitleToColorIndex(trimmed || "?");

  return (
    <View
      testID="cover-placeholder"
      accessibilityLabel={`${title} placeholder cover`}
      style={{
        alignItems: "center",
        aspectRatio: 2 / 3,
        backgroundColor: PLACEHOLDER_COLORS[colorIndex],
        borderRadius: 12,
        justifyContent: "center",
        overflow: "hidden",
        width: "100%",
      }}
    >
      <Text
        style={{
          color: "#f4f4f5",
          fontSize: 38,
          fontWeight: "700",
        }}
      >
        {firstLetter}
      </Text>
    </View>
  );
}
