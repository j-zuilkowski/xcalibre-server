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
  for (let i = 0; i < title.length; i += 1) {
    hash = (hash * 31 + title.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) % PLACEHOLDER_COLORS.length;
}

type CoverPlaceholderProps = {
  title: string;
  className?: string;
};

export function CoverPlaceholder({ title, className }: CoverPlaceholderProps) {
  const trimmed = title.trim();
  const firstLetter = (trimmed[0] ?? "?").toUpperCase();
  const colorIndex = hashTitleToColorIndex(trimmed || "?");
  const color = PLACEHOLDER_COLORS[colorIndex];

  return (
    <div
      data-testid="cover-placeholder"
      data-color-index={colorIndex}
      role="img"
      className={`aspect-[2/3] w-full overflow-hidden rounded-lg ${className ?? ""}`.trim()}
      style={{ backgroundColor: color }}
      aria-label={`${title} placeholder cover`}
    >
      <div className="flex h-full w-full items-center justify-center text-6xl font-serif text-zinc-100">
        {firstLetter}
      </div>
    </div>
  );
}
