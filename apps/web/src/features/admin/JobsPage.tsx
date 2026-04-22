import { useTranslation } from "react-i18next";
import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { apiClient } from "../../lib/api-client";
import { formatDateTime } from "./admin-utils";

export function JobsPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [statusFilter, setStatusFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState("");

  const jobsQuery = useQuery({
    queryKey: ["admin-jobs", statusFilter, typeFilter],
    queryFn: () =>
      apiClient.listJobs({
        status: statusFilter || undefined,
        job_type: typeFilter || undefined,
        page: 1,
        page_size: 25,
      }),
  });

  const cancelMutation = useMutation({
    mutationFn: (id: string) => apiClient.cancelJob(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-jobs"] });
    },
  });

  const jobs = jobsQuery.data?.items ?? [];
  const jobTypes = useMemo(() => {
    return Array.from(new Set(jobs.map((job) => job.job_type))).sort();
  }, [jobs]);

  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-6">
      <header>
        <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("admin.jobs")}</p>
        <h2 className="mt-2 text-3xl font-semibold text-zinc-50">{t("admin.llm_job_queue")}</h2>
      </header>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
        <div className="flex flex-wrap gap-3">
          <select
            value={statusFilter}
            onChange={(event) => setStatusFilter(event.target.value)}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          >
            <option value="">{t("admin.all_statuses")}</option>
            <option value="pending">{t("common.pending")}</option>
            <option value="running">{t("common.running")}</option>
            <option value="completed">{t("common.completed")}</option>
            <option value="failed">{t("common.failed")}</option>
          </select>

          <select
            value={typeFilter}
            onChange={(event) => setTypeFilter(event.target.value)}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          >
            <option value="">{t("admin.all_job_types")}</option>
            {jobTypes.map((type) => (
              <option key={type} value={type}>
                {type}
              </option>
            ))}
          </select>
        </div>
      </section>

      <section className="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th className="px-4 py-3 font-medium">{t("admin.job")}</th>
              <th className="px-4 py-3 font-medium">{t("book.book")}</th>
              <th className="px-4 py-3 font-medium">{t("common.status")}</th>
              <th className="px-4 py-3 font-medium">{t("common.created")}</th>
              <th className="px-4 py-3 font-medium">{t("common.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {jobs.map((job) => (
              <tr key={job.id} className="border-t border-zinc-800">
                <td className="px-4 py-3 text-zinc-100">{job.job_type}</td>
                <td className="px-4 py-3 text-zinc-300">{job.book_title ?? t("admin.library_job")}</td>
                <td className="px-4 py-3 text-zinc-300">{job.status}</td>
                <td className="px-4 py-3 text-zinc-300">{formatDateTime(job.created_at)}</td>
                <td className="px-4 py-3">
                  {job.status === "pending" ? (
                    <button
                      type="button"
                      onClick={() => void cancelMutation.mutateAsync(job.id)}
                      className="rounded-lg border border-red-900 px-3 py-2 text-xs text-red-300"
                    >
                      {t("common.cancel")}
                    </button>
                  ) : (
                    <span className="text-xs text-zinc-500">{t("admin.not_cancelable")}</span>
                  )}
                  <p className="mt-2 text-xs text-zinc-500">
                    {t("admin.started", { value: formatDateTime(job.started_at) })} · {t("admin.completed", { value: formatDateTime(job.completed_at) })}
                  </p>
                  {job.error_text ? <p className="mt-1 text-xs text-red-300">{job.error_text}</p> : null}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}
