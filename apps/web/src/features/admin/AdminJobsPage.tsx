import { useMemo, useState } from "react";
import type { AdminJob } from "@calibre/shared";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { apiClient } from "../../lib/api-client";
import { formatDateTime } from "./admin-utils";

const PAGE_SIZE = 25;

function formatDuration(job: AdminJob): string {
  if (!job.started_at) {
    return "—";
  }

  const start = new Date(job.started_at).getTime();
  if (Number.isNaN(start)) {
    return "—";
  }

  const endValue = job.completed_at ? new Date(job.completed_at).getTime() : Date.now();
  if (Number.isNaN(endValue) || endValue <= start) {
    return "<1s";
  }

  const totalSeconds = Math.floor((endValue - start) / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;

  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }

  return `${seconds}s`;
}

function statusBadge(job: AdminJob) {
  if (job.status === "pending") {
    return (
      <span className="inline-flex rounded-full border border-amber-200 bg-amber-50 px-2.5 py-1 text-xs font-medium text-amber-700">
        Pending
      </span>
    );
  }

  if (job.status === "running") {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-full border border-blue-200 bg-blue-50 px-2.5 py-1 text-xs font-medium text-blue-700">
        <span
          data-testid="running-spinner"
          aria-label="Running"
          className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-blue-200 border-t-blue-700"
        />
        Running
      </span>
    );
  }

  if (job.status === "completed") {
    return (
      <span className="inline-flex rounded-full border border-green-200 bg-green-50 px-2.5 py-1 text-xs font-medium text-green-700">
        Completed
      </span>
    );
  }

  return (
    <span className="inline-flex rounded-full border border-red-200 bg-red-50 px-2.5 py-1 text-xs font-medium text-red-700">
      Failed
    </span>
  );
}

export function AdminJobsPage() {
  const queryClient = useQueryClient();
  const [statusFilter, setStatusFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState("");
  const [page, setPage] = useState(1);

  const jobsQuery = useQuery({
    queryKey: ["admin-jobs", statusFilter, typeFilter, page],
    queryFn: () =>
      apiClient.listAdminJobs({
        status: statusFilter || undefined,
        job_type: typeFilter || undefined,
        page,
        page_size: PAGE_SIZE,
      }),
    refetchInterval: (query) =>
      query.state.data?.items.some((job) => job.status === "running") ? 5000 : false,
  });

  const cancelMutation = useMutation({
    mutationFn: (jobId: string) => apiClient.cancelAdminJob(jobId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["admin-jobs"] });
    },
  });

  const jobs = jobsQuery.data?.items ?? [];
  const total = jobsQuery.data?.total ?? 0;
  const pageSize = jobsQuery.data?.page_size ?? PAGE_SIZE;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));

  const jobTypes = useMemo(() => {
    const values = new Set(jobs.map((job) => job.job_type));
    if (typeFilter) {
      values.add(typeFilter);
    }
    return Array.from(values).sort();
  }, [jobs, typeFilter]);

  return (
    <main className="mx-auto flex w-full max-w-7xl flex-col gap-5">
      <header>
        <h2 className="text-2xl font-semibold text-zinc-50">Jobs</h2>
        <p className="text-sm text-zinc-400">Monitor and manage AI job execution.</p>
      </header>

      <section className="rounded-xl border border-zinc-800 bg-zinc-900/70 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <select
            value={statusFilter}
            onChange={(event) => {
              setStatusFilter(event.target.value);
              setPage(1);
            }}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          >
            <option value="">All statuses</option>
            <option value="pending">Pending</option>
            <option value="running">Running</option>
            <option value="completed">Completed</option>
            <option value="failed">Failed</option>
          </select>

          <select
            value={typeFilter}
            onChange={(event) => {
              setTypeFilter(event.target.value);
              setPage(1);
            }}
            className="rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100"
          >
            <option value="">All job types</option>
            {jobTypes.map((jobType) => (
              <option key={jobType} value={jobType}>
                {jobType}
              </option>
            ))}
          </select>
        </div>
      </section>

      <section className="overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900/70">
        <table className="min-w-full border-collapse text-left text-sm">
          <thead className="bg-zinc-950/60 text-zinc-400">
            <tr>
              <th className="px-4 py-3 font-medium">Job ID</th>
              <th className="px-4 py-3 font-medium">Type</th>
              <th className="px-4 py-3 font-medium">Status</th>
              <th className="px-4 py-3 font-medium">Book</th>
              <th className="px-4 py-3 font-medium">Created</th>
              <th className="px-4 py-3 font-medium">Duration</th>
              <th className="px-4 py-3 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {jobs.map((job) => (
              <tr key={job.id} className="border-t border-zinc-800">
                <td className="px-4 py-3 font-mono text-zinc-200">{job.id.slice(0, 8)}</td>
                <td className="px-4 py-3 text-zinc-200">{job.job_type}</td>
                <td className="px-4 py-3">{statusBadge(job)}</td>
                <td className="px-4 py-3 text-zinc-300">{job.book_title ?? "Library-wide"}</td>
                <td className="px-4 py-3 text-zinc-300">{formatDateTime(job.created_at)}</td>
                <td className="px-4 py-3 text-zinc-300">{formatDuration(job)}</td>
                <td className="px-4 py-3">
                  {job.status === "pending" ? (
                    <button
                      type="button"
                      onClick={() => void cancelMutation.mutateAsync(job.id)}
                      disabled={cancelMutation.isPending}
                      className="rounded-lg border border-red-900 px-3 py-2 text-xs text-red-300 disabled:opacity-60"
                    >
                      Cancel
                    </button>
                  ) : (
                    <span className="text-xs text-zinc-500">—</span>
                  )}
                </td>
              </tr>
            ))}

            {!jobsQuery.isLoading && jobs.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-4 py-8 text-center text-sm text-zinc-400">
                  No jobs found.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </section>

      <footer className="flex items-center justify-between rounded-xl border border-zinc-800 bg-zinc-900/70 p-4">
        <button
          type="button"
          onClick={() => setPage((previous) => Math.max(1, previous - 1))}
          disabled={page <= 1}
          className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200 disabled:cursor-not-allowed disabled:opacity-50"
        >
          Previous
        </button>
        <p className="text-sm text-zinc-400">
          Page {page} of {totalPages}
        </p>
        <button
          type="button"
          onClick={() => setPage((previous) => Math.min(totalPages, previous + 1))}
          disabled={page >= totalPages}
          className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200 disabled:cursor-not-allowed disabled:opacity-50"
        >
          Next
        </button>
      </footer>
    </main>
  );
}
