import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type {
  ApiError,
  Book,
  BookCustomValue,
  BookCustomValuePatch,
  BookSummary,
  CustomColumnType,
  FormatRef,
  TagSuggestion,
  ValidationResult,
} from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "../../components/ui/collapsible";
import { CoverPlaceholder } from "./CoverPlaceholder";

type BookDetailPageProps = {
  bookId?: string;
};

type SectionKey = "description" | "formats" | "identifiers" | "series" | "custom_fields" | "history" | "ai";
type AiTab = "classify" | "validate" | "derive";

const READ_FORMAT_PRIORITY = ["epub", "pdf", "azw3", "mobi"];

function resolveBookId(bookId?: string): string | null {
  if (bookId && bookId.trim().length > 0) {
    return bookId;
  }

  const segments = window.location.pathname.split("/").filter(Boolean);
  const booksIndex = segments.findIndex((segment) => segment === "books");
  if (booksIndex < 0 || booksIndex + 1 >= segments.length) {
    return null;
  }

  return decodeURIComponent(segments[booksIndex + 1]);
}

function formatBytes(sizeBytes: number): string {
  if (!Number.isFinite(sizeBytes) || sizeBytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = sizeBytes;
  let index = 0;

  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }

  const decimals = size >= 10 || index === 0 ? 0 : 1;
  return `${size.toFixed(decimals)} ${units[index]}`;
}

function getAuthorsLabel(book: Book, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (book.authors.length === 0) {
    return t("common.unknown_author");
  }

  return book.authors.map((author) => author.name).join(", ");
}

function getReadFormat(formats: FormatRef[]): string | null {
  if (formats.length === 0) {
    return null;
  }

  const normalized = new Map(formats.map((format) => [format.format.toLowerCase(), format.format]));

  for (const preferred of READ_FORMAT_PRIORITY) {
    const match = normalized.get(preferred);
    if (match) {
      return match;
    }
  }

  return formats[0]?.format ?? null;
}

function getYearLabel(pubdate: string | null, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (!pubdate) {
    return t("common.unknown");
  }

  const parsed = new Date(pubdate);
  if (!Number.isNaN(parsed.getTime())) {
    return String(parsed.getUTCFullYear());
  }

  const fallback = pubdate.match(/\d{4}/);
  return fallback?.[0] ?? t("common.unknown");
}

function isAdminOrEditor(roleName: string | undefined, canEdit: boolean | undefined): boolean {
  if (canEdit) {
    return true;
  }

  return roleName?.toLowerCase() === "admin";
}

function buildStars(ratingOutOfTen: number | null): { display: string; outOfFive: number } {
  const clampedOutOfTen = Math.max(0, Math.min(10, ratingOutOfTen ?? 0));
  const outOfFive = Math.round(clampedOutOfTen) / 2;
  const filled = Math.round(outOfFive);
  const display = `${"★".repeat(filled)}${"☆".repeat(5 - filled)}`;

  return { display, outOfFive };
}

function pushLibraryTagFilter(tag: string) {
  const params = new URLSearchParams();
  params.set("tag", tag);
  const nextUrl = `/library?${params.toString()}`;
  window.history.pushState({}, "", nextUrl);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

function toLlmErrorMessage(error: unknown, t: (key: string, options?: Record<string, unknown>) => string): string {
  const apiError = error as ApiError;
  if (apiError?.status === 503) {
    return t("book.llm_unavailable");
  }
  return t("book.unable_to_complete_request");
}

function confidencePercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function severityStyles(severity: ValidationResult["severity"]): string {
  if (severity === "ok") {
    return "border-green-200 bg-green-50 text-green-700";
  }

  if (severity === "warning") {
    return "border-amber-200 bg-amber-50 text-amber-700";
  }

  return "border-red-200 bg-red-50 text-red-700";
}

function formatCustomValue(value: BookCustomValue["value"], t: (key: string) => string): string {
  if (typeof value === "boolean") {
    return value ? t("common.yes") : t("common.no");
  }
  if (value === null || value === undefined || value === "") {
    return t("common.none");
  }
  return String(value);
}

function customValueToDraft(value: BookCustomValue["value"], columnType: CustomColumnType): string | boolean {
  if (columnType === "bool") {
    return Boolean(value);
  }
  if (value === null || value === undefined) {
    return "";
  }
  return String(value);
}

function toCustomPatchValue(
  columnType: CustomColumnType,
  draftValue: string | boolean,
): BookCustomValuePatch["value"] | undefined {
  if (columnType === "bool") {
    return Boolean(draftValue);
  }

  const text = String(draftValue).trim();
  if (!text) {
    return null;
  }

  if (columnType === "integer") {
    const parsed = Number.parseInt(text, 10);
    if (!Number.isFinite(parsed)) {
      return undefined;
    }
    return parsed;
  }

  if (columnType === "float") {
    const parsed = Number.parseFloat(text);
    if (!Number.isFinite(parsed)) {
      return undefined;
    }
    return parsed;
  }

  return text;
}

function isSameCustomValue(
  left: BookCustomValue["value"],
  right: BookCustomValuePatch["value"],
): boolean {
  if (left === null && right === null) {
    return true;
  }
  if (typeof left === "number" && typeof right === "number") {
    return left === right;
  }
  if (typeof left === "boolean" && typeof right === "boolean") {
    return left === right;
  }
  if ((left === null || left === undefined) && (right === null || right === undefined)) {
    return true;
  }
  return String(left ?? "") === String(right ?? "");
}

function Spinner({ className = "", label = "Loading" }: { className?: string; label?: string }) {
  return (
    <span
      aria-label={label}
      className={`inline-block h-3.5 w-3.5 animate-spin rounded-full border-2 border-zinc-300 border-t-teal-600 ${className}`}
    />
  );
}

function CollapsibleSection({
  label,
  open,
  onToggle,
  children,
}: {
  label: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <Collapsible open={open} onOpenChange={onToggle} className="rounded-xl border border-zinc-200 bg-white">
      <CollapsibleTrigger className="flex w-full items-center justify-between px-4 py-3 text-left">
        <span className="font-medium text-zinc-900">{label}</span>
        <span className="text-zinc-500" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t border-zinc-200 px-4 py-3 text-sm text-zinc-700">
        {children}
      </CollapsibleContent>
    </Collapsible>
  );
}

export function BookDetailPage({ bookId }: BookDetailPageProps) {
  const { t } = useTranslation();
  const resolvedBookId = resolveBookId(bookId);
  const user = useAuthStore((state) => state.user);
  const queryClient = useQueryClient();

  const [downloadOpen, setDownloadOpen] = useState(false);
  const [shelfMenuOpen, setShelfMenuOpen] = useState(false);
  const [metadataLookupOpen, setMetadataLookupOpen] = useState(false);
  const [mergeModalOpen, setMergeModalOpen] = useState(false);
  const [mergeSearch, setMergeSearch] = useState("");
  const [selectedDuplicateId, setSelectedDuplicateId] = useState<string | null>(null);
  const [metadataSource, setMetadataSource] = useState<"openlibrary" | "googlebooks">("openlibrary");
  const [actionsOpen, setActionsOpen] = useState(false);
  const [aiTab, setAiTab] = useState<AiTab>("classify");
  const [pendingSuggestions, setPendingSuggestions] = useState<TagSuggestion[]>([]);
  const [metadataResult, setMetadataResult] = useState<Awaited<ReturnType<typeof apiClient.lookupBookMetadata>> | null>(null);
  const [customFieldDrafts, setCustomFieldDrafts] = useState<Record<string, string | boolean>>({});
  const [sectionsOpen, setSectionsOpen] = useState<Record<SectionKey, boolean>>({
    description: false,
    formats: false,
    identifiers: false,
    series: false,
    custom_fields: false,
    history: false,
    ai: false,
  });

  const bookQuery = useQuery({
    queryKey: ["book", resolvedBookId],
    queryFn: () => apiClient.getBook(resolvedBookId as string),
    enabled: Boolean(resolvedBookId),
  });

  const customValuesQuery = useQuery({
    queryKey: ["book-custom-values", resolvedBookId],
    queryFn: () => apiClient.getBookCustomValues(resolvedBookId as string),
    enabled: Boolean(resolvedBookId),
  });

  const mergeCandidatesQuery = useQuery({
    queryKey: ["merge-candidates", mergeSearch.trim()],
    queryFn: async () => {
      const response = await apiClient.listBooks({
        q: mergeSearch.trim(),
        page: 1,
        page_size: 8,
        sort: "title",
        order: "asc",
      });
      return response.items.filter((item) => item.id !== resolvedBookId);
    },
    enabled: mergeModalOpen && mergeSearch.trim().length >= 2,
    staleTime: 30_000,
  });

  const selectedDuplicateQuery = useQuery({
    queryKey: ["merge-duplicate-book", selectedDuplicateId],
    queryFn: () => apiClient.getBook(selectedDuplicateId as string),
    enabled: mergeModalOpen && Boolean(selectedDuplicateId),
  });

  const shelvesQuery = useQuery({
    queryKey: ["shelves"],
    queryFn: () => apiClient.listShelves(),
    enabled: Boolean(resolvedBookId),
    staleTime: 60_000,
  });

  const llmHealthQuery = useQuery({
    queryKey: ["llm-health"],
    queryFn: () => apiClient.getLlmHealth(),
    enabled: Boolean(resolvedBookId),
    staleTime: 60_000,
  });

  const classifyMutation = useMutation({
    mutationFn: () => apiClient.classifyBook(resolvedBookId as string),
    onSuccess: (result) => {
      setPendingSuggestions(result.suggestions);
    },
  });

  const confirmTagMutation = useMutation({
    mutationFn: (payload: { confirm: string[]; reject: string[] }) =>
      apiClient.confirmTags(resolvedBookId as string, payload.confirm, payload.reject),
    onSuccess: (updatedBook, payload) => {
      queryClient.setQueryData(["book", resolvedBookId], updatedBook);
      const removedNames = new Set([...payload.confirm, ...payload.reject]);
      setPendingSuggestions((previous) =>
        previous.filter((suggestion) => !removedNames.has(suggestion.name)),
      );
    },
  });

  const confirmAllMutation = useMutation({
    mutationFn: () => apiClient.confirmAllTags(resolvedBookId as string),
    onSuccess: (updatedBook) => {
      queryClient.setQueryData(["book", resolvedBookId], updatedBook);
      setPendingSuggestions([]);
    },
  });

  const addToShelfMutation = useMutation({
    mutationFn: (shelfId: string) => apiClient.addBookToShelf(shelfId, resolvedBookId as string),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["shelves"] });
      setShelfMenuOpen(false);
    },
  });

  const readStateMutation = useMutation({
    mutationFn: (isRead: boolean) => apiClient.setBookReadState(resolvedBookId as string, isRead),
    onSuccess: () => {
      void queryClient.invalidateQueries();
    },
  });

  const archiveStateMutation = useMutation({
    mutationFn: (isArchived: boolean) =>
      apiClient.setBookArchivedState(resolvedBookId as string, isArchived),
    onSuccess: () => {
      void queryClient.invalidateQueries();
    },
  });

  const lookupMetadataMutation = useMutation({
    mutationFn: (source: "openlibrary" | "googlebooks") =>
      apiClient.lookupBookMetadata(resolvedBookId as string, source),
    onSuccess: (result) => {
      setMetadataResult(result);
      setMetadataLookupOpen(true);
      setActionsOpen(false);
    },
  });

  const applyMetadataMutation = useMutation({
    mutationFn: async (
      payload:
        | { field: "title"; value: string }
        | { field: "description"; value: string | null }
        | { field: "pubdate"; value: string | null }
        | { field: "authors"; value: string[] }
        | { field: "isbn_13"; value: string },
    ) => {
      if (payload.field === "title") {
        return apiClient.patchBook(resolvedBookId as string, { title: payload.value });
      }
      if (payload.field === "description") {
        return apiClient.patchBook(resolvedBookId as string, { description: payload.value });
      }
      if (payload.field === "pubdate") {
        return apiClient.patchBook(resolvedBookId as string, { pubdate: payload.value });
      }
      if (payload.field === "authors") {
        return apiClient.patchBook(resolvedBookId as string, { authors: payload.value });
      }
      return apiClient.patchBook(resolvedBookId as string, {
        identifiers: [{ id_type: "isbn13", value: payload.value }],
      });
    },
    onSuccess: (updatedBook) => {
      queryClient.setQueryData(["book", resolvedBookId], updatedBook);
    },
  });

  const validateMutation = useMutation({
    mutationFn: () => apiClient.validateBook(resolvedBookId as string),
  });

  const deriveMutation = useMutation({
    mutationFn: () => apiClient.deriveBook(resolvedBookId as string),
  });

  const patchCustomValuesMutation = useMutation({
    mutationFn: (values: BookCustomValuePatch[]) =>
      apiClient.patchBookCustomValues(resolvedBookId as string, values),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["book-custom-values", resolvedBookId] });
    },
  });

  const mergeBookMutation = useMutation({
    mutationFn: (duplicateId: string) => apiClient.mergeBook(resolvedBookId as string, duplicateId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["book", resolvedBookId] });
      await queryClient.invalidateQueries({ queryKey: ["books"] });
      setMergeModalOpen(false);
      setSelectedDuplicateId(null);
      setMergeSearch("");
      window.location.assign(`/books/${encodeURIComponent(resolvedBookId as string)}`);
    },
  });

  const book = bookQuery.data;

  useEffect(() => {
    const values = customValuesQuery.data;
    if (!values) {
      return;
    }
    const nextDrafts: Record<string, string | boolean> = {};
    for (const value of values) {
      nextDrafts[value.column_id] = customValueToDraft(value.value, value.column_type);
    }
    setCustomFieldDrafts(nextDrafts);
  }, [customValuesQuery.data]);

  const canEditBook = useMemo(
    () => isAdminOrEditor(user?.role.name, user?.role.can_edit),
    [user?.role.can_edit, user?.role.name],
  );
  const isAdmin = user?.role.name.toLowerCase() === "admin";

  if (!resolvedBookId) {
    return (
      <main className="min-h-screen bg-zinc-50 px-4 py-8 text-zinc-900 md:px-6 lg:px-8">
        <div className="mx-auto max-w-5xl rounded-xl border border-red-200 bg-red-50 p-4 text-red-700">
          {t("book.invalid_book_id")}
        </div>
      </main>
    );
  }

  if (bookQuery.isLoading) {
    return (
      <main className="min-h-screen bg-zinc-50 px-4 py-8 md:px-6 lg:px-8">
        <div className="mx-auto max-w-5xl animate-pulse rounded-xl border border-zinc-200 bg-white p-8" />
      </main>
    );
  }

  if (bookQuery.isError || !book) {
    return (
      <main className="min-h-screen bg-zinc-50 px-4 py-8 text-zinc-900 md:px-6 lg:px-8">
        <div className="mx-auto max-w-5xl rounded-xl border border-red-200 bg-red-50 p-4 text-red-700">
          {t("book.unable_to_load")}
        </div>
      </main>
    );
  }

  const readFormat = getReadFormat(book.formats);
  const authorsLabel = getAuthorsLabel(book, t);
  const rating = buildStars(book.rating);
  const confirmedTags = book.tags.filter((tag) => tag.confirmed);
  const showAiPanel = llmHealthQuery.data?.enabled === true;
  const customValues = customValuesQuery.data ?? [];
  const selectedDuplicateBook = selectedDuplicateQuery.data;

  const commitCustomFieldValue = async (field: BookCustomValue, draftValue: string | boolean) => {
    const patchValue = toCustomPatchValue(field.column_type, draftValue);
    if (patchValue === undefined) {
      return;
    }
    if (isSameCustomValue(field.value, patchValue)) {
      return;
    }
    await patchCustomValuesMutation.mutateAsync([{ column_id: field.column_id, value: patchValue }]);
  };

  return (
    <main className="min-h-screen bg-zinc-50 px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
        <header className="rounded-xl border border-zinc-200 bg-white p-4 shadow-sm md:p-6">
          <div className="mb-4 flex items-center justify-between">
            <a href="/library" className="text-sm font-medium text-zinc-600 hover:text-zinc-900">
              ← {t("common.back")}
            </a>

            {canEditBook ? (
              <div className="relative">
                <button
                  type="button"
                  aria-label={t("book.more_actions")}
                  onClick={() => setActionsOpen((open) => !open)}
                  className="rounded-lg border border-zinc-300 px-3 py-2 text-sm text-zinc-700"
                >
                  •••
                </button>
                {actionsOpen ? (
                  <div className="absolute right-0 z-20 mt-2 w-48 rounded-lg border border-zinc-200 bg-white p-1 shadow-lg">
                    <button
                      type="button"
                      onClick={() => {
                        setMetadataLookupOpen(true);
                        setActionsOpen(false);
                        if (!metadataResult) {
                          lookupMetadataMutation.mutate(metadataSource);
                        }
                      }}
                      className="block w-full rounded px-3 py-2 text-left text-sm hover:bg-zinc-100"
                    >
                      {t("book.lookup_metadata")}
                    </button>
                    {isAdmin ? (
                      <button
                        type="button"
                        onClick={() => {
                          setMergeModalOpen(true);
                          setActionsOpen(false);
                          setSelectedDuplicateId(null);
                          setMergeSearch("");
                        }}
                        className="block w-full rounded px-3 py-2 text-left text-sm hover:bg-zinc-100"
                      >
                        {t("book.merge_duplicate_book")}
                      </button>
                    ) : null}
                    <button type="button" className="block w-full rounded px-3 py-2 text-left text-sm hover:bg-zinc-100">
                      {t("book.replace_cover")}
                    </button>
                    <button
                      type="button"
                      className="block w-full rounded px-3 py-2 text-left text-sm text-red-600 hover:bg-red-50"
                    >
                      {t("book.delete_book")}
                    </button>
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>

          <div className="grid gap-5 md:grid-cols-[220px_1fr]">
            <div className="w-full max-w-[240px]">
              {book.has_cover ? (
                <img
                  src={apiClient.coverUrl(book.id)}
                  alt={`${book.title} cover`}
                  className="aspect-[2/3] w-full rounded-lg object-cover"
                />
              ) : (
                <CoverPlaceholder title={book.title} />
              )}
            </div>

            <div className="flex flex-col gap-3">
              <h1 className="text-3xl font-semibold text-zinc-900">{book.title}</h1>
              <p className="text-zinc-700">{authorsLabel}</p>
              <p className="text-sm text-zinc-500">
                {book.series ? `${book.series.name} · ${t("book.book")} ${book.series_index ?? "?"}` : t("book.standalone")}
              </p>

              <p className="text-sm text-zinc-700">
                <span aria-label="rating-stars">{rating.display}</span>
                <span className="ml-2 text-zinc-500">({rating.outOfFive.toFixed(1)}/5)</span>
              </p>

              <div className="mt-1 flex flex-wrap items-center gap-2">
                <a
                  href={
                    readFormat
                      ? `/books/${encodeURIComponent(book.id)}/read/${encodeURIComponent(readFormat)}`
                      : "#"
                  }
                  className={`inline-flex rounded-lg px-4 py-2 text-sm font-semibold ${
                    readFormat
                      ? "bg-teal-600 text-white"
                      : "cursor-not-allowed bg-zinc-300 text-zinc-500"
                  }`}
                  aria-disabled={!readFormat}
                  >
                  {t("common.read")}
                </a>

                <button
                  type="button"
                  onClick={() => void readStateMutation.mutateAsync(!book.is_read)}
                  className="inline-flex rounded-lg border border-zinc-300 bg-white px-4 py-2 text-sm font-semibold text-zinc-800"
                >
                  {book.is_read ? t("book.mark_unread") : t("book.mark_as_read")}
                </button>

                <button
                  type="button"
                  onClick={() => void archiveStateMutation.mutateAsync(!book.is_archived)}
                  className={`inline-flex rounded-lg border px-4 py-2 text-sm font-semibold ${
                    book.is_archived
                      ? "border-amber-600 bg-amber-600 text-white"
                      : "border-zinc-300 bg-white text-zinc-800"
                  }`}
                >
                  {book.is_archived ? t("book.unarchive") : t("book.archive")}
                </button>

                <div className="relative">
                  <button
                    type="button"
                    onClick={() => setDownloadOpen((open) => !open)}
                    className="inline-flex items-center rounded-lg border border-zinc-300 bg-white px-4 py-2 text-sm font-semibold text-zinc-800"
                  >
                    {t("common.download")} ▾
                  </button>

                  {downloadOpen ? (
                    <div className="absolute left-0 z-20 mt-2 min-w-[280px] rounded-lg border border-zinc-200 bg-white p-2 shadow-lg">
                      {book.formats.length > 0 ? (
                        <ul className="space-y-1">
                          {book.formats.map((format) => (
                            <li key={format.id}>
                              <a
                                href={apiClient.downloadUrl(book.id, format.format)}
                                download
                                className="flex items-center justify-between rounded px-2 py-2 text-sm hover:bg-zinc-100"
                              >
                                <span>{format.format.toUpperCase()}</span>
                                <span className="text-zinc-500">{formatBytes(format.size_bytes)}</span>
                              </a>
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <p className="px-2 py-1 text-sm text-zinc-500">{t("book.no_formats_available")}</p>
                      )}
                    </div>
                  ) : null}
                </div>

                <div className="relative">
                  <button
                    type="button"
                    onClick={() => setShelfMenuOpen((open) => !open)}
                    className="inline-flex items-center rounded-lg border border-zinc-300 bg-white px-4 py-2 text-sm font-semibold text-zinc-800"
                  >
                    {t("book.add_to_shelf")} ▾
                  </button>

                  {shelfMenuOpen ? (
                    <div className="absolute left-0 z-20 mt-2 min-w-[240px] rounded-lg border border-zinc-200 bg-white p-2 shadow-lg">
                      {shelvesQuery.isLoading ? (
                        <p className="px-2 py-1 text-sm text-zinc-500">{t("book.loading_shelves")}</p>
                      ) : shelvesQuery.data?.length ? (
                        <ul className="space-y-1">
                          {shelvesQuery.data.map((shelf) => (
                            <li key={shelf.id}>
                              <button
                                type="button"
                                onClick={() => addToShelfMutation.mutate(shelf.id)}
                                className="flex w-full items-center justify-between rounded px-2 py-2 text-left text-sm hover:bg-zinc-100"
                              >
                                <span className="truncate">{shelf.name}</span>
                                <span className="text-zinc-500">{shelf.book_count}</span>
                              </button>
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <p className="px-2 py-1 text-sm text-zinc-500">{t("book.no_shelves_yet")}</p>
                      )}
                    </div>
                  ) : null}
                </div>
              </div>
            </div>
          </div>
        </header>

        <section className="rounded-xl border border-zinc-200 bg-white px-4 py-3 text-sm text-zinc-700">
          <div className="flex flex-wrap items-center gap-2">
            <span>
              <strong>{t("book.language")}:</strong> {book.language ? book.language.toUpperCase() : t("common.unknown")}
            </span>
            <span aria-hidden="true">·</span>
            <span>
              <strong>{t("book.year")}:</strong> {getYearLabel(book.pubdate, t)}
            </span>
            <span aria-hidden="true">·</span>
            <span className="flex flex-wrap items-center gap-1">
              <strong>{t("book.tags")}:</strong>
              {confirmedTags.length > 0 ? (
                confirmedTags.map((tag) => (
                  <button
                    key={tag.id}
                    type="button"
                    onClick={() => pushLibraryTagFilter(tag.name)}
                    className="rounded-full border border-zinc-300 px-2 py-0.5 text-xs hover:border-zinc-400"
                  >
                    {tag.name}
                  </button>
                ))
              ) : (
                <span>{t("common.none")}</span>
              )}
            </span>
            <span aria-hidden="true">·</span>
            <span>
              <strong>{t("book.formats")}:</strong>{" "}
              {book.formats.length > 0
                ? book.formats.map((format) => format.format.toUpperCase()).join(" ")
                : t("common.none")}
            </span>
          </div>
        </section>

        <div className="flex flex-col gap-3">
          <CollapsibleSection
            label={t("book.description")}
            open={sectionsOpen.description}
            onToggle={() =>
              setSectionsOpen((previous) => ({
                ...previous,
                description: !previous.description,
              }))
            }
          >
            {book.description?.trim() || t("book.no_description_available")}
          </CollapsibleSection>

          <CollapsibleSection
            label={t("book.formats")}
            open={sectionsOpen.formats}
            onToggle={() =>
              setSectionsOpen((previous) => ({
                ...previous,
                formats: !previous.formats,
              }))
            }
          >
            {book.formats.length > 0 ? (
              <ul className="space-y-2">
                {book.formats.map((format) => (
                  <li key={`${format.id}-section`} className="flex items-center justify-between gap-3">
                    <span>
                      {format.format.toUpperCase()} <span className="text-zinc-500">({formatBytes(format.size_bytes)})</span>
                    </span>
                    <a
                      href={apiClient.downloadUrl(book.id, format.format)}
                      download
                      className="text-teal-700 hover:text-teal-800"
                    >
                      {t("common.download")}
                    </a>
                  </li>
                ))}
              </ul>
            ) : (
              t("book.no_formats_available")
            )}
          </CollapsibleSection>

          <CollapsibleSection
            label={t("book.identifiers")}
            open={sectionsOpen.identifiers}
            onToggle={() =>
              setSectionsOpen((previous) => ({
                ...previous,
                identifiers: !previous.identifiers,
              }))
            }
          >
            {book.identifiers.length > 0 ? (
              <ul className="space-y-1">
                {book.identifiers.map((identifier) => (
                  <li key={identifier.id}>
                    <span className="font-medium text-zinc-900">{identifier.id_type.toUpperCase()}:</span>{" "}
                    <span>{identifier.value}</span>
                  </li>
                ))}
              </ul>
            ) : (
              t("book.no_identifiers_available")
            )}
          </CollapsibleSection>

          <CollapsibleSection
            label={t("book.series")}
            open={sectionsOpen.series}
            onToggle={() =>
              setSectionsOpen((previous) => ({
                ...previous,
                series: !previous.series,
              }))
            }
          >
            {book.series ? (
              <div>
                <p className="font-medium text-zinc-900">{book.series.name}</p>
                <p className="text-zinc-600">{t("book.book")} {book.series_index ?? "?"}</p>
              </div>
            ) : (
              t("book.not_in_series")
            )}
          </CollapsibleSection>

          <CollapsibleSection
            label={t("book.custom_fields")}
            open={sectionsOpen.custom_fields}
            onToggle={() =>
              setSectionsOpen((previous) => ({
                ...previous,
                custom_fields: !previous.custom_fields,
              }))
            }
          >
            {customValuesQuery.isLoading ? (
              <p className="text-zinc-500">{t("book.loading_custom_fields")}</p>
            ) : customValues.length === 0 ? (
              <p className="text-zinc-500">{t("book.no_custom_fields_configured")}</p>
            ) : (
              <div className="space-y-3">
                {customValues.map((field) => {
                  const draftValue = customFieldDrafts[field.column_id] ?? customValueToDraft(field.value, field.column_type);
                  return (
                    <div key={field.column_id} className="grid gap-2 md:grid-cols-[180px_1fr] md:items-center">
                      <label className="font-medium text-zinc-900">{field.label}</label>
                      {canEditBook ? (
                        field.column_type === "bool" ? (
                          <label className="inline-flex items-center gap-2 text-zinc-700">
                            <input
                              type="checkbox"
                              checked={Boolean(draftValue)}
                              onChange={(event) => {
                                const checked = event.target.checked;
                                setCustomFieldDrafts((previous) => ({
                                  ...previous,
                                  [field.column_id]: checked,
                                }));
                                void commitCustomFieldValue(field, checked);
                              }}
                            />
                            {Boolean(draftValue) ? t("common.yes") : t("common.no")}
                          </label>
                        ) : (
                          <input
                            type={field.column_type === "integer" || field.column_type === "float" ? "number" : "text"}
                            step={field.column_type === "float" ? "any" : undefined}
                            value={String(draftValue)}
                            onChange={(event) =>
                              setCustomFieldDrafts((previous) => ({
                                ...previous,
                                [field.column_id]: event.target.value,
                              }))
                            }
                            onBlur={(event) => {
                              void commitCustomFieldValue(field, event.target.value);
                            }}
                            className="w-full rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-800"
                          />
                        )
                      ) : (
                        <span className="text-zinc-700">{formatCustomValue(field.value, t)}</span>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </CollapsibleSection>

          {isAdmin ? (
            <CollapsibleSection
              label={t("book.history")}
              open={sectionsOpen.history}
              onToggle={() =>
                setSectionsOpen((previous) => ({
                  ...previous,
                  history: !previous.history,
                }))
              }
            >
              {t("book.no_history_entries_yet")}
            </CollapsibleSection>
          ) : null}

          {showAiPanel ? (
            <CollapsibleSection
              label={t("book.ai")}
              open={sectionsOpen.ai}
              onToggle={() =>
                setSectionsOpen((previous) => ({
                  ...previous,
                  ai: !previous.ai,
                }))
              }
            >
              <div className="space-y-4">
                <div className="flex flex-wrap gap-2">
                  {(["classify", "validate", "derive"] as const).map((tab) => {
                    const active = aiTab === tab;
                    return (
                      <button
                        key={tab}
                        type="button"
                        onClick={() => setAiTab(tab)}
                        className={`rounded-lg border px-3 py-2 text-sm ${
                          active
                            ? "border-teal-600 bg-teal-600 text-white"
                            : "border-zinc-300 bg-white text-zinc-700"
                        }`}
                      >
                        {tab === "classify" ? t("book.classify") : tab === "validate" ? t("book.validate") : t("book.derive")}
                      </button>
                    );
                  })}
                </div>

                {aiTab === "classify" ? (
                  <div className="space-y-3">
                    <button
                      type="button"
                      onClick={() => void classifyMutation.mutateAsync()}
                      disabled={classifyMutation.isPending}
                      className="inline-flex items-center gap-2 rounded-lg bg-teal-600 px-4 py-2 text-sm font-semibold text-white disabled:opacity-70"
                    >
                      {classifyMutation.isPending ? (
                        <Spinner className="border-zinc-200 border-t-white" label={t("common.loading")} />
                      ) : null}
                      {t("book.classify")}
                    </button>

                    {classifyMutation.isError ? (
                      <p className="text-sm text-red-700">{toLlmErrorMessage(classifyMutation.error, t)}</p>
                    ) : null}

                    {pendingSuggestions.length > 0 ? (
                      <div className="space-y-3">
                        <div className="flex flex-wrap gap-2">
                          {pendingSuggestions.map((suggestion) => (
                            <div
                              key={suggestion.name}
                              className="inline-flex items-center gap-1.5 rounded-full border border-teal-200 bg-teal-50 px-3 py-1 text-xs text-teal-800"
                            >
                              <span className="font-medium">
                                {suggestion.name} ({confidencePercent(suggestion.confidence)})
                              </span>
                              <button
                                type="button"
                                aria-label={t("book.confirm_suggestion", { name: suggestion.name })}
                                disabled={confirmTagMutation.isPending}
                                onClick={() =>
                                  void confirmTagMutation.mutateAsync({
                                    confirm: [suggestion.name],
                                    reject: [],
                                  })
                                }
                                className="rounded-full px-1 text-teal-700 hover:bg-teal-100 disabled:opacity-50"
                              >
                                ✓
                              </button>
                              <button
                                type="button"
                                aria-label={t("book.reject_suggestion", { name: suggestion.name })}
                                disabled={confirmTagMutation.isPending}
                                onClick={() =>
                                  void confirmTagMutation.mutateAsync({
                                    confirm: [],
                                    reject: [suggestion.name],
                                  })
                                }
                                className="rounded-full px-1 text-zinc-700 hover:bg-zinc-100 disabled:opacity-50"
                              >
                                x
                              </button>
                            </div>
                          ))}
                        </div>

                        <button
                          type="button"
                          onClick={() => void confirmAllMutation.mutateAsync()}
                          disabled={confirmAllMutation.isPending || pendingSuggestions.length === 0}
                          className="inline-flex items-center gap-2 rounded-lg border border-teal-600 px-3 py-2 text-sm font-semibold text-teal-700 disabled:opacity-50"
                        >
                          {confirmAllMutation.isPending ? <Spinner label={t("common.loading")} /> : null}
                          {t("book.confirm_all")}
                        </button>
                      </div>
                    ) : classifyMutation.isSuccess ? (
                      <p className="text-sm text-zinc-600">{t("book.no_pending_suggestions")}</p>
                    ) : null}
                  </div>
                ) : null}

                {aiTab === "validate" ? (
                  <div className="space-y-3">
                    <button
                      type="button"
                      onClick={() => void validateMutation.mutateAsync()}
                      disabled={validateMutation.isPending}
                      className="inline-flex items-center gap-2 rounded-lg bg-teal-600 px-4 py-2 text-sm font-semibold text-white disabled:opacity-70"
                    >
                      {validateMutation.isPending ? (
                        <Spinner className="border-zinc-200 border-t-white" label={t("common.loading")} />
                      ) : null}
                      {t("book.validate")}
                    </button>

                    {validateMutation.isError ? (
                      <p className="text-sm text-red-700">{toLlmErrorMessage(validateMutation.error, t)}</p>
                    ) : null}

                    {validateMutation.data ? (
                      <div className="space-y-3">
                        <span
                          className={`inline-flex rounded-full border px-3 py-1 text-xs font-semibold uppercase ${severityStyles(validateMutation.data.severity)}`}
                        >
                          {validateMutation.data.severity}
                        </span>

                        {validateMutation.data.issues.length === 0 ? (
                          <p className="inline-flex items-center gap-2 rounded-lg border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700">
                            <span aria-hidden="true">✓</span>
                          {t("book.no_issues_found")}
                          </p>
                        ) : (
                          <div className="space-y-2">
                            {validateMutation.data.issues.map((issue, index) => (
                              <article
                                key={`${issue.field}-${index}`}
                                className="rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2"
                              >
                                <p className="text-sm font-semibold text-zinc-900">{issue.field}</p>
                                <p className="mt-1 text-sm text-zinc-700">{issue.message}</p>
                                {issue.suggestion ? (
                                  <p className="mt-1 text-xs text-zinc-500">{t("book.suggestion")}: {issue.suggestion}</p>
                                ) : null}
                              </article>
                            ))}
                          </div>
                        )}
                      </div>
                    ) : null}
                  </div>
                ) : null}

                {aiTab === "derive" ? (
                  <div className="space-y-3">
                    <button
                      type="button"
                      onClick={() => void deriveMutation.mutateAsync()}
                      disabled={deriveMutation.isPending}
                      className="inline-flex items-center gap-2 rounded-lg bg-teal-600 px-4 py-2 text-sm font-semibold text-white disabled:opacity-70"
                    >
                      {deriveMutation.isPending ? (
                        <Spinner className="border-zinc-200 border-t-white" label={t("common.loading")} />
                      ) : null}
                      {t("book.derive")}
                    </button>

                    {deriveMutation.isError ? (
                      <p className="text-sm text-red-700">{toLlmErrorMessage(deriveMutation.error, t)}</p>
                    ) : null}

                    {deriveMutation.data ? (
                      <div className="space-y-3 text-zinc-700">
                        <p>{deriveMutation.data.summary}</p>

                        <div>
                          <p className="font-semibold text-zinc-900">{t("book.related_titles")}</p>
                          <ul className="ml-5 list-disc">
                            {deriveMutation.data.related_titles.map((title) => (
                              <li key={title}>{title}</li>
                            ))}
                          </ul>
                        </div>

                        <div>
                          <p className="font-semibold text-zinc-900">{t("book.discussion_questions")}</p>
                          <ol className="ml-5 list-decimal">
                            {deriveMutation.data.discussion_questions.map((question) => (
                              <li key={question}>{question}</li>
                            ))}
                          </ol>
                        </div>
                      </div>
                    ) : null}
                  </div>
                ) : null}
              </div>
            </CollapsibleSection>
          ) : null}
        </div>

        {mergeModalOpen ? (
          <div className="fixed inset-0 z-40 flex items-center justify-center bg-zinc-950/60 p-4">
            <div className="w-full max-w-4xl rounded-2xl border border-zinc-200 bg-white p-5 shadow-2xl">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="text-xs font-semibold uppercase tracking-wide text-teal-700">{t("book.merge_books")}</p>
                  <h2 className="mt-1 text-lg font-semibold text-zinc-900">{t("book.merge_duplicate_into_current_book")}</h2>
                </div>
                <button
                  type="button"
                  onClick={() => setMergeModalOpen(false)}
                  className="rounded-lg border border-zinc-300 px-3 py-2 text-sm text-zinc-700"
                >
                  {t("common.close")}
                </button>
              </div>

              <div className="mt-4 space-y-3">
                <label className="block text-sm font-medium text-zinc-700">
                  {t("book.search_duplicate_by_title")}
                  <input
                    value={mergeSearch}
                    onChange={(event) => setMergeSearch(event.target.value)}
                    placeholder={t("book.start_typing_title")}
                    className="mt-1 w-full rounded-lg border border-zinc-300 px-3 py-2 text-sm"
                  />
                </label>

                {mergeSearch.trim().length < 2 ? (
                  <p className="text-sm text-zinc-500">{t("book.type_at_least_2_characters")}</p>
                ) : mergeCandidatesQuery.isLoading ? (
                  <p className="text-sm text-zinc-500">{t("common.searching")}</p>
                ) : mergeCandidatesQuery.data && mergeCandidatesQuery.data.length > 0 ? (
                  <div className="max-h-44 overflow-auto rounded-lg border border-zinc-200">
                    {mergeCandidatesQuery.data.map((candidate: BookSummary) => (
                      <button
                        key={candidate.id}
                        type="button"
                        onClick={() => setSelectedDuplicateId(candidate.id)}
                        className={`flex w-full items-center justify-between px-3 py-2 text-left text-sm hover:bg-zinc-50 ${
                          selectedDuplicateId === candidate.id ? "bg-teal-50" : ""
                        }`}
                      >
                        <span>{candidate.title}</span>
                        <span className="text-zinc-500">
                          {candidate.authors.map((author) => author.name).join(", ")}
                        </span>
                      </button>
                    ))}
                  </div>
                ) : (
                  <p className="text-sm text-zinc-500">{t("book.no_matching_books_found")}</p>
                )}
              </div>

              {selectedDuplicateBook ? (
                <div className="mt-5 grid gap-4 md:grid-cols-2">
                  <section className="rounded-xl border border-zinc-200 p-3">
                    <p className="text-xs font-semibold uppercase tracking-wide text-zinc-500">{t("book.primary_keep")}</p>
                    <p className="mt-1 text-lg font-semibold text-zinc-900">{book.title}</p>
                    <p className="mt-1 text-sm text-zinc-700">
                      {t("book.authors")}: {book.authors.map((author) => author.name).join(", ") || t("common.unknown")}
                    </p>
                    <p className="mt-1 text-sm text-zinc-700">
                      {t("book.formats")}: {book.formats.map((format) => format.format.toUpperCase()).join(", ") || t("common.none")}
                    </p>
                    <p className="mt-1 text-sm text-zinc-700">{t("book.identifiers")}: {book.identifiers.length}</p>
                  </section>
                  <section className="rounded-xl border border-zinc-200 p-3">
                    <p className="text-xs font-semibold uppercase tracking-wide text-zinc-500">{t("book.duplicate_remove")}</p>
                    <p className="mt-1 text-lg font-semibold text-zinc-900">{selectedDuplicateBook.title}</p>
                    <p className="mt-1 text-sm text-zinc-700">
                      {t("book.authors")}:{" "}
                      {selectedDuplicateBook.authors.map((author) => author.name).join(", ") || t("common.unknown")}
                    </p>
                    <p className="mt-1 text-sm text-zinc-700">
                      {t("book.formats")}:{" "}
                      {selectedDuplicateBook.formats
                        .map((format) => format.format.toUpperCase())
                        .join(", ") || t("common.none")}
                    </p>
                    <p className="mt-1 text-sm text-zinc-700">
                      {t("book.identifiers")}: {selectedDuplicateBook.identifiers.length}
                    </p>
                  </section>
                </div>
              ) : null}

              <div className="mt-5 flex items-center justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setMergeModalOpen(false)}
                  className="rounded-lg border border-zinc-300 px-4 py-2 text-sm text-zinc-700"
                >
                  {t("common.cancel")}
                </button>
                <button
                  type="button"
                  disabled={!selectedDuplicateBook || mergeBookMutation.isPending}
                  onClick={() => {
                    if (selectedDuplicateBook) {
                      void mergeBookMutation.mutateAsync(selectedDuplicateBook.id);
                    }
                  }}
                  className="rounded-lg bg-red-600 px-4 py-2 text-sm font-semibold text-white disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {mergeBookMutation.isPending ? t("book.merging") : t("book.confirm_merge")}
                </button>
              </div>
            </div>
          </div>
        ) : null}

        {metadataLookupOpen ? (
          <aside className="fixed right-4 top-20 z-30 w-[min(92vw,420px)] rounded-2xl border border-zinc-200 bg-white p-4 shadow-2xl">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-xs font-semibold uppercase tracking-wide text-teal-700">{t("book.metadata_lookup")}</p>
                <h2 className="mt-1 text-lg font-semibold text-zinc-900">{t("book.external_suggestions")}</h2>
              </div>
              <button
                type="button"
                onClick={() => setMetadataLookupOpen(false)}
                className="rounded-lg border border-zinc-200 px-2 py-1 text-sm text-zinc-600"
              >
                {t("common.close")}
              </button>
            </div>

            <div className="mt-4 space-y-3">
              <label className="block text-sm font-medium text-zinc-700">
                {t("book.source")}
                <select
                  value={metadataSource}
                  onChange={(event) => setMetadataSource(event.target.value as "openlibrary" | "googlebooks")}
                  className="mt-1 w-full rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm"
                >
                  <option value="openlibrary">{t("book.open_library")}</option>
                  <option value="googlebooks">{t("book.google_books")}</option>
                </select>
              </label>

              <button
                type="button"
                onClick={() => lookupMetadataMutation.mutate(metadataSource)}
                disabled={lookupMetadataMutation.isPending}
                className="inline-flex items-center gap-2 rounded-lg bg-teal-600 px-4 py-2 text-sm font-semibold text-white disabled:opacity-70"
              >
                {t("book.lookup_metadata")}
              </button>

              {lookupMetadataMutation.isError ? (
                <p className="text-sm text-red-700">{t("book.unable_to_load_external_metadata")}</p>
              ) : null}

              {metadataResult ? (
                <div className="space-y-4 border-t border-zinc-200 pt-4">
                  <div>
                    <p className="text-xs font-semibold uppercase tracking-wide text-zinc-500">
                      {metadataResult.source}
                    </p>
                    <p className="mt-1 text-xl font-semibold text-zinc-900">{metadataResult.title}</p>
                    {metadataResult.description ? (
                      <p className="mt-2 text-sm text-zinc-600">{metadataResult.description}</p>
                    ) : null}
                  </div>

                  <div className="space-y-2 text-sm">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-zinc-700">
                        <strong>{t("book.title")}</strong>
                      </span>
                      <button
                        type="button"
                        onClick={() =>
                          void applyMetadataMutation.mutateAsync({
                            field: "title",
                            value: metadataResult.title,
                          })
                        }
                        className="rounded-md border border-zinc-300 px-2 py-1 text-xs font-semibold"
                      >
                        {t("book.apply")}
                      </button>
                    </div>

                    <div className="flex items-center justify-between gap-2">
                      <span className="text-zinc-700">
                        <strong>{t("book.authors")}</strong>
                      </span>
                      <button
                        type="button"
                        onClick={() =>
                          void applyMetadataMutation.mutateAsync({
                            field: "authors",
                            value: metadataResult.authors,
                          })
                        }
                        className="rounded-md border border-zinc-300 px-2 py-1 text-xs font-semibold"
                      >
                        {t("book.apply")}
                      </button>
                    </div>

                    <div className="flex items-center justify-between gap-2">
                      <span className="text-zinc-700">
                        <strong>{t("book.description")}</strong>
                      </span>
                      <button
                        type="button"
                        onClick={() =>
                          void applyMetadataMutation.mutateAsync({
                            field: "description",
                            value: metadataResult.description,
                          })
                        }
                        className="rounded-md border border-zinc-300 px-2 py-1 text-xs font-semibold"
                      >
                        {t("book.apply")}
                      </button>
                    </div>

                    <div className="flex items-center justify-between gap-2">
                      <span className="text-zinc-700">
                        <strong>{t("book.published_date")}</strong>
                      </span>
                      <button
                        type="button"
                        onClick={() =>
                          void applyMetadataMutation.mutateAsync({
                            field: "pubdate",
                            value: metadataResult.published_date,
                          })
                        }
                        className="rounded-md border border-zinc-300 px-2 py-1 text-xs font-semibold"
                      >
                        {t("book.apply")}
                      </button>
                    </div>

                    {metadataResult.isbn_13 ? (
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-zinc-700">
                          <strong>{t("book.isbn_13")}</strong>
                        </span>
                        <button
                          type="button"
                          onClick={() =>
                            void applyMetadataMutation.mutateAsync({
                              field: "isbn_13",
                              value: metadataResult.isbn_13,
                            })
                          }
                          className="rounded-md border border-zinc-300 px-2 py-1 text-xs font-semibold"
                        >
                          {t("book.apply")}
                        </button>
                      </div>
                    ) : null}
                  </div>

                  {metadataResult.publisher ? (
                    <p className="text-sm text-zinc-600">
                      <strong>{t("book.publisher")}:</strong> {metadataResult.publisher}
                    </p>
                  ) : null}
                  {metadataResult.categories.length > 0 ? (
                    <p className="text-sm text-zinc-600">
                      <strong>{t("book.categories")}:</strong> {metadataResult.categories.join(", ")}
                    </p>
                  ) : null}
                </div>
              ) : null}
            </div>
          </aside>
        ) : null}
      </div>
    </main>
  );
}
