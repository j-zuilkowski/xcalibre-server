/**
 * ImportPage (profile) — Goodreads and StoryGraph reading-history CSV import.
 *
 * Route: /profile/import
 *
 * Workflow:
 *   1. User selects an import source tab (Goodreads or StoryGraph).
 *   2. User drops or picks a .csv export file via a drag-and-drop zone.
 *   3. Clicking "Start import" calls the appropriate mutation:
 *      - Goodreads  → POST /api/v1/import/goodreads  (multipart)
 *      - StoryGraph → POST /api/v1/import/storygraph (multipart)
 *      Both return `{ job_id }`.
 *   4. `statusQuery` polls GET /api/v1/import/:jobId/status every 2 seconds
 *      while status is "pending" or "running".  Polling stops automatically
 *      when the job reaches a terminal state.
 *   5. Completed state shows matched / unmatched counts and a collapsible
 *      list of unmatched titles for manual reconciliation.
 *
 * State is scoped per source tab — switching tabs does not reset the other
 * tab's file selection or job ID.
 *
 * API calls:
 *   POST /api/v1/import/goodreads
 *   POST /api/v1/import/storygraph
 *   GET  /api/v1/import/:jobId/status
 */
import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { apiClient } from "../../lib/api-client";
import { ProfileSidebar } from "./ProfileSidebar";

type ImportSource = "goodreads" | "storygraph";

type SourceConfig = {
  title: string;
  description: string;
  instructionsLabel: string;
  instructionsUrl: string;
};

const SOURCE_CONFIGS: Record<ImportSource, SourceConfig> = {
  goodreads: {
    title: "Goodreads",
    description: "Upload your export file to import reading history and shelves.",
    instructionsLabel: "Export instructions",
    instructionsUrl: "https://www.goodreads.com/review/import",
  },
  storygraph: {
    title: "StoryGraph",
    description: "Upload your export file to import reading history and shelves.",
    instructionsLabel: "Export instructions",
    instructionsUrl: "https://app.thestorygraph.com",
  },
};

const SOURCE_OPTIONS: Array<{ key: ImportSource; label: string }> = [
  { key: "goodreads", label: "Goodreads" },
  { key: "storygraph", label: "StoryGraph" },
];

/**
 * ImportPage renders the Goodreads / StoryGraph CSV import wizard with a
 * drag-and-drop file zone and a live job-status progress section.
 */
export function ImportPage() {
  const { t } = useTranslation();
  const [activeSource, setActiveSource] = useState<ImportSource>("goodreads");
  const [files, setFiles] = useState<Record<ImportSource, File | null>>({
    goodreads: null,
    storygraph: null,
  });
  const [jobIds, setJobIds] = useState<Record<ImportSource, string | null>>({
    goodreads: null,
    storygraph: null,
  });
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const config = SOURCE_CONFIGS[activeSource];
  const selectedFile = files[activeSource];
  const jobId = jobIds[activeSource];

  const startMutation = useMutation({
    mutationFn: async (payload: { source: ImportSource; file: File }) => {
      if (payload.source === "goodreads") {
        return apiClient.startGoodreadsImport(payload.file);
      }
      return apiClient.startStorygraphImport(payload.file);
    },
    onSuccess: (response, variables) => {
      setJobIds((current) => ({
        ...current,
        [variables.source]: response.job_id,
      }));
      setError(null);
    },
    onError: () => {
      setError("Unable to start the import.");
    },
  });

  // Poll every 2 s while the job is running; refetchInterval returns false
  // once the job reaches a terminal state so the query stops on its own.
  const statusQuery = useQuery({
    queryKey: ["profile-import-status", activeSource, jobId],
    queryFn: () => apiClient.getReadingImportStatus(jobId as string),
    enabled: Boolean(jobId),
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      return status === "pending" || status === "running" ? 2000 : false;
    },
  });

  const status = jobId ? statusQuery.data : null;
  const processedRows = status ? status.matched + status.unmatched : 0;
  const totalRows = status?.total_rows ?? 0;
  const unmatchedTitles = useMemo(
    () => status?.errors.map((entry) => ({ title: entry.title, author: entry.author })) ?? [],
    [status?.errors],
  );

  async function beginImport() {
    if (!selectedFile) {
      setError("Choose a CSV file first.");
      return;
    }

    setError(null);
    await startMutation.mutateAsync({
      source: activeSource,
      file: selectedFile,
    });
  }

  function resetCurrentTab() {
    setFiles((current) => ({
      ...current,
      [activeSource]: null,
    }));
    setJobIds((current) => ({
      ...current,
      [activeSource]: null,
    }));
    setError(null);
  }

  function setCurrentFile(file: File | null) {
    setFiles((current) => ({
      ...current,
      [activeSource]: file,
    }));
    setError(null);
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 lg:flex-row">
      <ProfileSidebar active="import" />

      <main className="min-w-0 flex-1">
        <div className="flex flex-col gap-6">
          <header>
            <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("nav.profile")}</p>
            <h2 className="mt-2 text-3xl font-semibold text-zinc-50">Import history</h2>
          </header>

          <section className="rounded-3xl border border-zinc-800 bg-zinc-900/70 p-5 shadow-[0_24px_80px_-34px_rgba(15,118,110,0.45)]">
            <div className="flex flex-wrap gap-2">
              {SOURCE_OPTIONS.map((option) => {
                const isActive = option.key === activeSource;
                return (
                  <button
                    key={option.key}
                    type="button"
                    onClick={() => setActiveSource(option.key)}
                    className={`rounded-full border px-4 py-2 text-sm font-medium transition ${
                      isActive
                        ? "border-teal-500 bg-teal-500 text-zinc-950"
                        : "border-zinc-700 text-zinc-300 hover:border-zinc-600 hover:text-zinc-50"
                    }`}
                  >
                    {option.label}
                  </button>
                );
              })}
            </div>

            <div className="mt-5 rounded-2xl border border-zinc-800 bg-zinc-950/60 p-5">
              <p className="text-sm uppercase tracking-[0.18em] text-teal-300">{config.title}</p>
              <p className="mt-3 text-sm leading-6 text-zinc-300">{config.description}</p>
              <a
                href={config.instructionsUrl}
                target="_blank"
                rel="noreferrer"
                className="mt-3 inline-flex items-center gap-2 text-sm font-medium text-teal-300 underline decoration-teal-500/40 underline-offset-4"
              >
                {config.instructionsLabel}
                <span className="text-zinc-500">→</span>
              </a>

              <label
                className={`mt-5 flex min-h-44 cursor-pointer flex-col items-center justify-center rounded-3xl border border-dashed px-4 py-8 text-center transition ${
                  dragging
                    ? "border-teal-500 bg-teal-500/10"
                    : "border-zinc-700 bg-zinc-950 text-zinc-300 hover:border-zinc-500"
                }`}
                onDragEnter={(event) => {
                  event.preventDefault();
                  setDragging(true);
                }}
                onDragOver={(event) => {
                  event.preventDefault();
                  setDragging(true);
                }}
                onDragLeave={() => setDragging(false)}
                onDrop={(event) => {
                  event.preventDefault();
                  setDragging(false);
                  const nextFile = event.dataTransfer.files?.[0] ?? null;
                  if (nextFile && !nextFile.name.toLowerCase().endsWith(".csv")) {
                    setError("Upload a .csv file.");
                    return;
                  }
                  setCurrentFile(nextFile);
                }}
              >
                <input
                  type="file"
                  accept=".csv,text/csv"
                  onChange={(event) => {
                    const nextFile = event.target.files?.[0] ?? null;
                    if (nextFile && !nextFile.name.toLowerCase().endsWith(".csv")) {
                      setError("Upload a .csv file.");
                      return;
                    }
                    setCurrentFile(nextFile);
                  }}
                  className="sr-only"
                />
                <span className="text-base font-medium text-zinc-100">
                  Drop your CSV file here or click to choose one
                </span>
                <span className="mt-1 text-xs uppercase tracking-[0.18em] text-zinc-500">
                  CSV files only
                </span>
                {selectedFile ? (
                  <span className="mt-4 rounded-full border border-teal-500/40 bg-teal-500/10 px-3 py-1 text-xs text-teal-200">
                    {selectedFile.name}
                  </span>
                ) : null}
              </label>

              {error ? <p className="mt-4 text-sm text-red-300">{error}</p> : null}

              <div className="mt-5 flex flex-wrap gap-3">
                <button
                  type="button"
                  onClick={() => {
                    void beginImport();
                  }}
                  disabled={!selectedFile || startMutation.isPending}
                  className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {startMutation.isPending ? "Starting..." : "Start import"}
                </button>
                <button
                  type="button"
                  onClick={resetCurrentTab}
                  className="rounded-lg border border-zinc-700 px-4 py-2 text-sm font-medium text-zinc-100 hover:border-zinc-500"
                >
                  Import again
                </button>
              </div>
            </div>
          </section>

          <section className="rounded-3xl border border-zinc-800 bg-zinc-900/70 p-5">
            <div className="flex items-center justify-between gap-4">
              <h3 className="text-lg font-semibold text-zinc-50">Progress</h3>
              {status ? (
                <span className="rounded-full border border-zinc-700 px-3 py-1 text-xs uppercase tracking-[0.2em] text-zinc-400">
                  {status.status}
                </span>
              ) : null}
            </div>

            {!status ? (
              <p className="mt-4 text-sm text-zinc-400">Upload a file to start an import.</p>
            ) : status.status === "pending" || status.status === "running" ? (
              <div className="mt-4 flex items-center gap-4 rounded-2xl border border-teal-500/20 bg-teal-500/8 p-4">
                <div className="h-5 w-5 animate-spin rounded-full border-2 border-teal-300 border-t-transparent" />
                <p className="text-sm text-zinc-200">
                  {totalRows > 0
                    ? `Processing row ${Math.min(processedRows, totalRows)} of ${totalRows}...`
                    : "Processing import..."}
                </p>
              </div>
            ) : (
              <div className="mt-4 grid gap-4">
                <div className="grid gap-4 md:grid-cols-2">
                  <div className="rounded-2xl border border-emerald-500/20 bg-emerald-500/8 p-4">
                    <p className="text-xs uppercase tracking-[0.18em] text-emerald-300">Matched</p>
                    <p className="mt-2 text-3xl font-semibold text-zinc-50">{status.matched}</p>
                    <p className="mt-2 text-sm text-zinc-300">books matched and updated</p>
                  </div>
                  <div className="rounded-2xl border border-zinc-700 bg-zinc-950/70 p-4">
                    <p className="text-xs uppercase tracking-[0.18em] text-zinc-400">Not found</p>
                    <p className="mt-2 text-3xl font-semibold text-zinc-50">{status.unmatched}</p>
                    <p className="mt-2 text-sm text-zinc-300">books not found in your library</p>
                  </div>
                </div>

                {status.unmatched > 0 ? (
                  <details className="rounded-2xl border border-zinc-800 bg-zinc-950/60 p-4">
                    <summary className="cursor-pointer text-sm font-medium text-zinc-100">
                      Unmatched titles
                    </summary>
                    <div className="mt-4 space-y-3">
                      {unmatchedTitles.map((item, index) => (
                        <div
                          key={`${item.title}-${item.author}-${index}`}
                          className="rounded-xl border border-zinc-800 bg-zinc-900/80 px-3 py-2"
                        >
                          <p className="text-sm font-medium text-zinc-100">{item.title}</p>
                          <p className="text-xs text-zinc-400">{item.author}</p>
                        </div>
                      ))}
                    </div>
                  </details>
                ) : null}

                <p className="text-xs uppercase tracking-[0.18em] text-zinc-500">
                  Completed {status.completed_at ?? "just now"}
                </p>
              </div>
            )}
          </section>
        </div>
      </main>
    </div>
  );
}
