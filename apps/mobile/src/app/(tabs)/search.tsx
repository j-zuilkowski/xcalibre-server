import React from "react";
import { useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Alert,
  FlatList,
  Modal,
  Pressable,
  Text,
  TextInput,
  View,
  type ListRenderItem,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { Image } from "expo-image";
import { useRouter } from "expo-router";
import { useQuery } from "@tanstack/react-query";
import type { BookSummary, SearchResultItem } from "@autolibre/shared";
import { useApi } from "../../lib/api";
import { useDebounce } from "../../hooks/useDebounce";
import { CoverPlaceholder } from "../../components/CoverPlaceholder";

type SearchTab = "fts" | "semantic";
type SearchOrder = "asc" | "desc";
type SearchSort = "title" | "author" | "created_at" | "rating";
type SearchFormat = "any" | "epub" | "pdf" | "mobi";

type SearchFilters = {
  language: string;
  format: SearchFormat;
  sort: SearchSort;
  order: SearchOrder;
};

const PAGE_SIZE = 20;

const DEFAULT_FILTERS: SearchFilters = {
  language: "",
  format: "any",
  sort: "title",
  order: "asc",
};

function authorLabel(book: BookSummary): string {
  if (book.authors.length === 0) {
    return "Unknown author";
  }

  return book.authors.map((author) => author.name).join(", ");
}

function scoreLabel(score?: number): string | null {
  if (typeof score !== "number") {
    return null;
  }

  const percentage = Math.round(Math.max(0, Math.min(1, score)) * 100);
  return `score: ${percentage}%`;
}

function SearchCard({
  book,
  score,
}: {
  book: BookSummary;
  score?: number;
}) {
  const client = useApi();
  const router = useRouter();
  const [imageFailed, setImageFailed] = useState(false);
  const coverUri = book.has_cover ? client.coverUrl(book.id) : null;
  const scoreText = scoreLabel(score);

  return (
    <Pressable
      testID={`search-result-${book.id}`}
      onPress={() => {
        router.push({ pathname: "/book/[id]", params: { id: book.id } });
      }}
      className="relative flex-1 overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900"
    >
      <View className="overflow-hidden rounded-t-2xl bg-zinc-800">
        {book.has_cover && coverUri && !imageFailed ? (
          <Image
            source={{ uri: coverUri }}
            onError={() => setImageFailed(true)}
            style={{ aspectRatio: 2 / 3, width: "100%" }}
          />
        ) : (
          <CoverPlaceholder title={book.title} />
        )}

        {scoreText ? (
          <View className="absolute right-2 top-2 rounded-full bg-teal-500 px-2.5 py-1">
            <Text className="text-[10px] font-semibold text-zinc-950">{scoreText}</Text>
          </View>
        ) : null}
      </View>

      <View className="gap-0.5 px-3 py-3">
        <Text className="truncate text-sm font-semibold text-zinc-50" numberOfLines={1}>
          {book.title}
        </Text>
        <Text className="truncate text-xs text-zinc-400" numberOfLines={1}>
          {authorLabel(book)}
        </Text>
      </View>
    </Pressable>
  );
}

function SearchPrompt({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <View className="flex-1 items-center justify-center px-6">
      <View className="mb-4 rounded-full bg-zinc-900 p-4">
        <Ionicons name="search-outline" color="#d4d4d8" size={28} />
      </View>
      <Text className="text-xl font-semibold text-zinc-50">{title}</Text>
      {subtitle ? <Text className="mt-2 text-center text-sm text-zinc-400">{subtitle}</Text> : null}
    </View>
  );
}

function SearchLoading() {
  return (
    <View className="flex-1 items-center justify-center">
      <ActivityIndicator color="#14b8a6" size="large" />
    </View>
  );
}

function SearchEmpty() {
  return (
    <View className="flex-1 items-center justify-center px-6">
      <Text className="text-xl font-semibold text-zinc-50">No results</Text>
      <Text className="mt-2 text-center text-sm text-zinc-400">Try a different search term</Text>
    </View>
  );
}

function SearchFiltersSheet({
  open,
  draft,
  onChange,
  onReset,
  onApply,
  onClose,
}: {
  open: boolean;
  draft: SearchFilters;
  onChange: (next: SearchFilters) => void;
  onReset: () => void;
  onApply: () => void;
  onClose: () => void;
}) {
  const setLanguage = (language: string) => onChange({ ...draft, language });
  const setFormat = (format: SearchFormat) => onChange({ ...draft, format });
  const setSort = (sort: SearchSort) => onChange({ ...draft, sort });
  const toggleOrder = () =>
    onChange({ ...draft, order: draft.order === "asc" ? "desc" : "asc" });

  return (
    <Modal animationType="slide" transparent visible={open} onRequestClose={onClose}>
      <View className="flex-1 justify-end">
        <Pressable className="absolute inset-0 bg-black/60" onPress={onClose} />
        <View className="rounded-t-3xl border-t border-zinc-800 bg-zinc-950 px-5 pb-8 pt-3">
          <View className="mx-auto mb-3 h-1.5 w-12 rounded-full bg-zinc-700" />
          <Text className="text-lg font-semibold text-zinc-50">Filters</Text>

          <View className="mt-4 gap-4">
            <View>
              <Text className="mb-2 text-xs font-semibold uppercase tracking-wider text-zinc-400">
                Language
              </Text>
              <TextInput
                value={draft.language}
                onChangeText={setLanguage}
                placeholder="Any language"
                placeholderTextColor="#71717a"
                className="rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3 text-zinc-50"
              />
            </View>

            <View>
              <Text className="mb-2 text-xs font-semibold uppercase tracking-wider text-zinc-400">
                Format
              </Text>
              <View className="flex-row flex-wrap gap-2">
                {(["any", "epub", "pdf", "mobi"] as const).map((format) => {
                  const active = draft.format === format;
                  return (
                    <Pressable
                      key={format}
                      onPress={() => setFormat(format)}
                      className={`rounded-full border px-3 py-2 ${active ? "border-teal-500 bg-teal-500" : "border-zinc-800 bg-zinc-900"}`}
                    >
                      <Text className={`text-sm font-medium ${active ? "text-zinc-950" : "text-zinc-300"}`}>
                        {format === "any" ? "Any" : format.toUpperCase()}
                      </Text>
                    </Pressable>
                  );
                })}
              </View>
            </View>

            <View>
              <Text className="mb-2 text-xs font-semibold uppercase tracking-wider text-zinc-400">
                Sort
              </Text>
              <View className="flex-row flex-wrap gap-2">
                {(
                  [
                    ["title", "Title"],
                    ["author", "Author"],
                    ["created_at", "Date added"],
                    ["rating", "Rating"],
                  ] as const
                ).map(([value, label]) => {
                  const active = draft.sort === value;
                  return (
                    <Pressable
                      key={value}
                      onPress={() => setSort(value)}
                      className={`rounded-full border px-3 py-2 ${active ? "border-teal-500 bg-teal-500" : "border-zinc-800 bg-zinc-900"}`}
                    >
                      <Text className={`text-sm font-medium ${active ? "text-zinc-950" : "text-zinc-300"}`}>
                        {label}
                      </Text>
                    </Pressable>
                  );
                })}
              </View>
            </View>

            <View>
              <Text className="mb-2 text-xs font-semibold uppercase tracking-wider text-zinc-400">
                Order
              </Text>
              <Pressable
                onPress={toggleOrder}
                className="flex-row items-center justify-between rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3"
              >
                <Text className="text-sm text-zinc-300">
                  {draft.order === "asc" ? "A→Z" : "Z→A"}
                </Text>
                <Text className="text-sm text-zinc-500">
                  {draft.order === "asc" ? "Ascending" : "Descending"}
                </Text>
              </Pressable>
            </View>
          </View>

          <View className="mt-6 flex-row gap-3">
            <Pressable
              onPress={onReset}
              className="flex-1 rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3"
            >
              <Text className="text-center text-sm font-semibold text-zinc-100">Reset</Text>
            </Pressable>
            <Pressable
              onPress={onApply}
              className="flex-1 rounded-2xl bg-teal-500 px-4 py-3"
            >
              <Text className="text-center text-sm font-semibold text-zinc-950">Apply</Text>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}

export default function SearchTabScreen() {
  const client = useApi();
  const [query, setQuery] = useState("");
  const debouncedQuery = useDebounce(query, 300);
  const [activeTab, setActiveTab] = useState<SearchTab>("fts");
  const [page, setPage] = useState(1);
  const [searchRefreshToken, setSearchRefreshToken] = useState(0);
  const [semanticEnabled, setSemanticEnabled] = useState(false);
  const [filters, setFilters] = useState<SearchFilters>(DEFAULT_FILTERS);
  const [draftFilters, setDraftFilters] = useState<SearchFilters>(DEFAULT_FILTERS);
  const [filtersOpen, setFiltersOpen] = useState(false);

  const trimmedQuery = debouncedQuery.trim();
  const hasQuery = trimmedQuery.length > 0;
  const isSemantic = activeTab === "semantic";
  const searchPage = isSemantic ? 1 : page;

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        const health = await client.getLlmHealth();
        if (!cancelled) {
          setSemanticEnabled(Boolean(health.enabled));
        }
      } catch {
        if (!cancelled) {
          setSemanticEnabled(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client]);

  useEffect(() => {
    setPage(1);
  }, [trimmedQuery, activeTab, filters.language, filters.format, filters.sort, filters.order]);

  const searchQuery = useQuery({
    queryKey: [
      "search",
      activeTab,
      trimmedQuery,
      searchPage,
      filters.language,
      filters.format,
      filters.sort,
      filters.order,
      searchRefreshToken,
    ],
    enabled: hasQuery,
    queryFn: () =>
      client.search(
        isSemantic
          ? {
              q: trimmedQuery,
              semantic: true,
              page: 1,
              page_size: PAGE_SIZE,
            }
          : {
              q: trimmedQuery,
              language: filters.language || undefined,
              format: filters.format === "any" ? undefined : filters.format,
              sort: filters.sort,
              order: filters.order,
              page: searchPage,
              page_size: PAGE_SIZE,
            },
      ),
  });

  const results = useMemo(() => searchQuery.data?.items ?? [], [searchQuery.data?.items]);
  const total = searchQuery.data?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const isFetchingResults = searchQuery.isLoading || searchQuery.isFetching;

  const openFiltersSheet = () => {
    setDraftFilters(filters);
    setFiltersOpen(true);
  };

  const applyFilters = () => {
    setFilters(draftFilters);
    setFiltersOpen(false);
    setPage(1);
    setSearchRefreshToken((current) => current + 1);
  };

  const resetFilters = () => {
    setDraftFilters(DEFAULT_FILTERS);
    setFilters(DEFAULT_FILTERS);
    setFiltersOpen(false);
    setPage(1);
    setSearchRefreshToken((current) => current + 1);
  };

  const renderItem: ListRenderItem<SearchResultItem> = ({ item }) => (
    <SearchCard book={item} score={item.score} />
  );

  const pagination = !isSemantic && hasQuery ? (
    <View className="mt-4 flex-row items-center justify-between rounded-2xl border border-zinc-800 bg-zinc-900 px-4 py-3">
      <Pressable
        testID="search-pagination-prev"
        disabled={page <= 1}
        onPress={() => setPage((current) => Math.max(1, current - 1))}
        className={`rounded-full px-3 py-2 ${page <= 1 ? "opacity-40" : "bg-zinc-800"}`}
      >
        <Text className="text-sm font-semibold text-zinc-100">Previous</Text>
      </Pressable>

      <Text className="text-sm text-zinc-400">
        {page} / {totalPages}
      </Text>

      <Pressable
        testID="search-pagination-next"
        disabled={page >= totalPages}
        onPress={() => setPage((current) => Math.min(totalPages, current + 1))}
        className={`rounded-full px-3 py-2 ${page >= totalPages ? "opacity-40" : "bg-zinc-800"}`}
      >
        <Text className="text-sm font-semibold text-zinc-100">Next</Text>
      </Pressable>
    </View>
  ) : null;

  return (
    <View className="flex-1 bg-zinc-950 px-4 pt-3">
      <View className="flex-row items-center gap-3">
        <View className="flex-1 flex-row items-center rounded-2xl border border-zinc-800 bg-zinc-800 px-3">
          <Ionicons name="search-outline" color="#a1a1aa" size={18} />
          <TextInput
            testID="search-input"
            value={query}
            onChangeText={setQuery}
            placeholder="Search books"
            placeholderTextColor="#a1a1aa"
            autoCapitalize="none"
            autoCorrect={false}
            returnKeyType="search"
            className="flex-1 px-3 py-3 text-base text-zinc-50"
          />
        </View>

        <Pressable
          testID="search-filters-button"
          onPress={openFiltersSheet}
          className="h-12 w-12 items-center justify-center rounded-2xl border border-zinc-800 bg-zinc-900"
        >
          <Ionicons name="options-outline" color="#e4e4e7" size={18} />
        </Pressable>
      </View>

      <View className="mt-3 flex-row gap-2">
        <Pressable
          testID="search-tab-fts"
          onPress={() => {
            setActiveTab("fts");
            setPage(1);
          }}
          className={`flex-1 rounded-full border px-4 py-2.5 ${
            activeTab === "fts"
              ? "border-teal-500 bg-teal-500"
              : "border-zinc-800 bg-zinc-900"
          }`}
        >
          <Text
            className={`text-center text-sm font-semibold ${
              activeTab === "fts" ? "text-zinc-950" : "text-zinc-200"
            }`}
          >
            Library
          </Text>
        </Pressable>

        <Pressable
          testID="search-tab-semantic"
          onPress={() => {
            if (!semanticEnabled) {
              Alert.alert(
                "Semantic search requires the AI features to be enabled on your server.",
              );
              return;
            }

            setActiveTab("semantic");
            setPage(1);
          }}
          accessibilityState={{ disabled: !semanticEnabled }}
          className={`flex-1 rounded-full border px-4 py-2.5 ${
            activeTab === "semantic"
              ? "border-teal-500 bg-teal-500"
              : "border-zinc-800 bg-zinc-900"
          } ${semanticEnabled ? "" : "opacity-40"}`}
        >
          <Text
            className={`text-center text-sm font-semibold ${
              activeTab === "semantic" ? "text-zinc-950" : "text-zinc-200"
            }`}
          >
            AI Semantic
          </Text>
        </Pressable>
      </View>

      <View className="mt-4 flex-1">
        {!hasQuery ? (
          <SearchPrompt title="Enter a search term" subtitle="Search your library" />
        ) : isFetchingResults ? (
          <SearchLoading />
        ) : (
          <FlatList
            testID="search-results"
            data={results}
            renderItem={renderItem}
            keyExtractor={(item) => item.id}
            numColumns={2}
            columnWrapperStyle={{ gap: 12 }}
            contentContainerStyle={{
              flexGrow: 1,
              gap: 12,
              paddingBottom: 16,
            }}
            ListEmptyComponent={<SearchEmpty />}
            ListFooterComponent={pagination}
          />
        )}
      </View>

      <SearchFiltersSheet
        open={filtersOpen}
        draft={draftFilters}
        onChange={setDraftFilters}
        onReset={resetFilters}
        onApply={applyFilters}
        onClose={() => setFiltersOpen(false)}
      />
    </View>
  );
}
