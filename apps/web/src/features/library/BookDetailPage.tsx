import { type ReactNode, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { ApiError, Book, FormatRef, TagSuggestion, ValidationResult } from "@calibre/shared";
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

type SectionKey = "description" | "formats" | "identifiers" | "series" | "history" | "ai";
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

function getAuthorsLabel(book: Book): string {
  if (book.authors.length === 0) {
    return "Unknown author";
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

function getYearLabel(pubdate: string | null): string {
  if (!pubdate) {
    return "Unknown";
  }

  const parsed = new Date(pubdate);
  if (!Number.isNaN(parsed.getTime())) {
    return String(parsed.getUTCFullYear());
  }

  const fallback = pubdate.match(/\d{4}/);
  return fallback?.[0] ?? "Unknown";
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

function toLlmErrorMessage(error: unknown): string {
  const apiError = error as ApiError;
  if (apiError?.status === 503) {
    return "LLM unavailable";
  }
  return "Unable to complete this request right now.";
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

function Spinner({ className = "" }: { className?: string }) {
  return (
    <span
      aria-label="Loading"
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
  const resolvedBookId = resolveBookId(bookId);
  const user = useAuthStore((state) => state.user);
  const queryClient = useQueryClient();

  const [downloadOpen, setDownloadOpen] = useState(false);
  const [actionsOpen, setActionsOpen] = useState(false);
  const [aiTab, setAiTab] = useState<AiTab>("classify");
  const [pendingSuggestions, setPendingSuggestions] = useState<TagSuggestion[]>([]);
  const [sectionsOpen, setSectionsOpen] = useState<Record<SectionKey, boolean>>({
    description: false,
    formats: false,
    identifiers: false,
    series: false,
    history: false,
    ai: false,
  });

  const bookQuery = useQuery({
    queryKey: ["book", resolvedBookId],
    queryFn: () => apiClient.getBook(resolvedBookId as string),
    enabled: Boolean(resolvedBookId),
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

  const validateMutation = useMutation({
    mutationFn: () => apiClient.validateBook(resolvedBookId as string),
  });

  const deriveMutation = useMutation({
    mutationFn: () => apiClient.deriveBook(resolvedBookId as string),
  });

  const book = bookQuery.data;

  const canEditBook = useMemo(
    () => isAdminOrEditor(user?.role.name, user?.role.can_edit),
    [user?.role.can_edit, user?.role.name],
  );
  const isAdmin = user?.role.name.toLowerCase() === "admin";

  if (!resolvedBookId) {
    return (
      <main className="min-h-screen bg-zinc-50 px-4 py-8 text-zinc-900 md:px-6 lg:px-8">
        <div className="mx-auto max-w-5xl rounded-xl border border-red-200 bg-red-50 p-4 text-red-700">
          Invalid book id.
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
          Unable to load this book right now.
        </div>
      </main>
    );
  }

  const readFormat = getReadFormat(book.formats);
  const authorsLabel = getAuthorsLabel(book);
  const rating = buildStars(book.rating);
  const confirmedTags = book.tags.filter((tag) => tag.confirmed);
  const showAiPanel = llmHealthQuery.data?.enabled === true;

  return (
    <main className="min-h-screen bg-zinc-50 px-4 py-6 text-zinc-900 md:px-6 lg:px-8">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
        <header className="rounded-xl border border-zinc-200 bg-white p-4 shadow-sm md:p-6">
          <div className="mb-4 flex items-center justify-between">
            <a href="/library" className="text-sm font-medium text-zinc-600 hover:text-zinc-900">
              ← Back
            </a>

            {canEditBook ? (
              <div className="relative">
                <button
                  type="button"
                  aria-label="More actions"
                  onClick={() => setActionsOpen((open) => !open)}
                  className="rounded-lg border border-zinc-300 px-3 py-2 text-sm text-zinc-700"
                >
                  •••
                </button>
                {actionsOpen ? (
                  <div className="absolute right-0 z-20 mt-2 w-48 rounded-lg border border-zinc-200 bg-white p-1 shadow-lg">
                    <button type="button" className="block w-full rounded px-3 py-2 text-left text-sm hover:bg-zinc-100">
                      Edit metadata
                    </button>
                    <button type="button" className="block w-full rounded px-3 py-2 text-left text-sm hover:bg-zinc-100">
                      Replace cover
                    </button>
                    <button
                      type="button"
                      className="block w-full rounded px-3 py-2 text-left text-sm text-red-600 hover:bg-red-50"
                    >
                      Delete book
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
                {book.series ? `${book.series.name} · Book ${book.series_index ?? "?"}` : "Standalone"}
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
                  Read
                </a>

                <div className="relative">
                  <button
                    type="button"
                    onClick={() => setDownloadOpen((open) => !open)}
                    className="inline-flex items-center rounded-lg border border-zinc-300 bg-white px-4 py-2 text-sm font-semibold text-zinc-800"
                  >
                    Download ▾
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
                        <p className="px-2 py-1 text-sm text-zinc-500">No formats available.</p>
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
              <strong>Language:</strong> {book.language ? book.language.toUpperCase() : "Unknown"}
            </span>
            <span aria-hidden="true">·</span>
            <span>
              <strong>Year:</strong> {getYearLabel(book.pubdate)}
            </span>
            <span aria-hidden="true">·</span>
            <span className="flex flex-wrap items-center gap-1">
              <strong>Tags:</strong>
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
                <span>None</span>
              )}
            </span>
            <span aria-hidden="true">·</span>
            <span>
              <strong>Formats:</strong>{" "}
              {book.formats.length > 0
                ? book.formats.map((format) => format.format.toUpperCase()).join(" ")
                : "None"}
            </span>
          </div>
        </section>

        <div className="flex flex-col gap-3">
          <CollapsibleSection
            label="Description"
            open={sectionsOpen.description}
            onToggle={() =>
              setSectionsOpen((previous) => ({
                ...previous,
                description: !previous.description,
              }))
            }
          >
            {book.description?.trim() || "No description available."}
          </CollapsibleSection>

          <CollapsibleSection
            label="Formats"
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
                      Download
                    </a>
                  </li>
                ))}
              </ul>
            ) : (
              "No formats available."
            )}
          </CollapsibleSection>

          <CollapsibleSection
            label="Identifiers"
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
              "No identifiers available."
            )}
          </CollapsibleSection>

          <CollapsibleSection
            label="Series"
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
                <p className="text-zinc-600">Book {book.series_index ?? "?"}</p>
              </div>
            ) : (
              "This book is not in a series."
            )}
          </CollapsibleSection>

          {isAdmin ? (
            <CollapsibleSection
              label="History"
              open={sectionsOpen.history}
              onToggle={() =>
                setSectionsOpen((previous) => ({
                  ...previous,
                  history: !previous.history,
                }))
              }
            >
              No history entries yet.
            </CollapsibleSection>
          ) : null}

          {showAiPanel ? (
            <CollapsibleSection
              label="AI"
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
                        {tab === "classify" ? "Classify" : tab === "validate" ? "Validate" : "Derive"}
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
                      {classifyMutation.isPending ? <Spinner className="border-zinc-200 border-t-white" /> : null}
                      Classify
                    </button>

                    {classifyMutation.isError ? (
                      <p className="text-sm text-red-700">{toLlmErrorMessage(classifyMutation.error)}</p>
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
                                aria-label={`Confirm ${suggestion.name}`}
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
                                aria-label={`Reject ${suggestion.name}`}
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
                          {confirmAllMutation.isPending ? <Spinner /> : null}
                          Confirm All
                        </button>
                      </div>
                    ) : classifyMutation.isSuccess ? (
                      <p className="text-sm text-zinc-600">No pending suggestions.</p>
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
                      {validateMutation.isPending ? <Spinner className="border-zinc-200 border-t-white" /> : null}
                      Validate
                    </button>

                    {validateMutation.isError ? (
                      <p className="text-sm text-red-700">{toLlmErrorMessage(validateMutation.error)}</p>
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
                            No issues found
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
                                  <p className="mt-1 text-xs text-zinc-500">Suggestion: {issue.suggestion}</p>
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
                      {deriveMutation.isPending ? <Spinner className="border-zinc-200 border-t-white" /> : null}
                      Generate
                    </button>

                    {deriveMutation.isError ? (
                      <p className="text-sm text-red-700">{toLlmErrorMessage(deriveMutation.error)}</p>
                    ) : null}

                    {deriveMutation.data ? (
                      <div className="space-y-3 text-zinc-700">
                        <p>{deriveMutation.data.summary}</p>

                        <div>
                          <p className="font-semibold text-zinc-900">Related titles</p>
                          <ul className="ml-5 list-disc">
                            {deriveMutation.data.related_titles.map((title) => (
                              <li key={title}>{title}</li>
                            ))}
                          </ul>
                        </div>

                        <div>
                          <p className="font-semibold text-zinc-900">Discussion questions</p>
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
      </div>
    </main>
  );
}
