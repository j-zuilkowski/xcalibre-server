import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { apiClient } from "../../lib/api-client";
import { formatDateTime } from "./admin-utils";

type ImportMode = "upload" | "path";

export function ImportPage() {
  const { t } = useTranslation();
  const [mode, setMode] = useState<ImportMode>("upload");
  const [pathValue, setPathValue] = useState("");
  const [dryRun, setDryRun] = useState(false);
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);

  const startMutation = useMutation({
    mutationFn: (payload: Parameters<typeof apiClient.startBulkImport>[0]) => apiClient.startBulkImport(payload),
    onSuccess: (response) => {
      setJobId(response.job_id);
    },
  });

  const statusQuery = useQuery({
    queryKey: ["admin-import-status", jobId],
    queryFn: () => apiClient.getImportStatus(jobId as string),
    enabled: Boolean(jobId),
    refetchInterval: jobId ? 2000 : false,
  });

  const status = statusQuery.data;

  const logLines = useMemo(() => {
    if (!status) {
      return [];
    }

    const lines = [
      `Status: ${status.status}`,
      `Imported ${status.records_imported}/${status.records_total}`,
      `Failed: ${status.records_failed}`,
      `Skipped: ${status.records_skipped}`,
    ];

    for (const failure of status.failures) {
      lines.push(`${failure.file}: ${failure.reason}`);
    }

    return lines;
  }, [status]);

  useEffect(() => {
    if (jobId && status?.status === "completed") {
      setJobId(jobId);
    }
  }, [jobId, status?.status]);

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.import")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">{t("admin.bulk_import")}</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <div className="flex gap-2">
          {(["upload", "path"] as const).map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => setMode(item)}
              className={`rounded-lg border px-3 py-2 text-sm ${
                mode === item
                  ? "border-teal-500 bg-teal-500 text-zinc-950"
                  : "border-zinc-700 text-zinc-300"
              }`}
            >
              {item === "upload" ? t("admin.upload_zip") : t("admin.server_path")}
            </button>
          ))}
        </div>

        <form
          className="mt-5 grid gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (mode === "upload") {
              if (!selectedFile) {
                return;
              }
              void startMutation.mutateAsync({ source: "upload", file: selectedFile, dry_run: dryRun });
              return;
            }

            if (!pathValue.trim()) {
              return;
            }
            void startMutation.mutateAsync({ source: "path", path: pathValue.trim(), dry_run: dryRun });
          }}
        >
          {mode === "upload" ? (
            <input
              type="file"
              accept=".zip"
              onChange={(event) => setSelectedFile(event.target.files?.[0] ?? null)}
              className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
            />
          ) : (
            <input
              value={pathValue}
              onChange={(event) => setPathValue(event.target.value)}
              placeholder={t("admin.server_path_placeholder")}
              className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
            />
          )}

          <label className="flex items-center gap-2 text-sm text-zinc-300">
            <input type="checkbox" checked={dryRun} onChange={(event) => setDryRun(event.target.checked)} />
            {t("admin.dry_run")}
          </label>

          <button
            type="submit"
            className="w-fit rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950"
          >
            {startMutation.isPending ? t("common.starting") : t("admin.start_import")}
          </button>
        </form>
      </section>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <h3 className="text-lg font-semibold text-zinc-50">{t("admin.progress_log")}</h3>
        {status ? (
          <div className="mt-3 space-y-2 text-sm text-zinc-300">
            <p>{t("admin.job")}: {status.id}</p>
            <p>{t("common.started")}: {formatDateTime(status.started_at)}</p>
            <p>{t("common.completed")}: {formatDateTime(status.completed_at)}</p>
            <ul className="space-y-1 rounded-xl border border-zinc-800 bg-zinc-950 p-4">
              {logLines.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </div>
        ) : (
          <p className="mt-3 text-sm text-zinc-400">{t("admin.no_import_job_started_yet")}</p>
        )}
      </section>
    </div>
  );
}
