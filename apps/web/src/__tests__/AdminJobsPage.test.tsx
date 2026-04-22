import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { AdminJob, PaginatedResponse } from "@autolibre/shared";
import { AdminJobsPage } from "../features/admin/AdminJobsPage";
import { apiClient } from "../lib/api-client";

const listAdminJobsMock = vi.spyOn(apiClient, "listAdminJobs");
const cancelAdminJobMock = vi.spyOn(apiClient, "cancelAdminJob");

function makeJob(overrides: Partial<AdminJob>): AdminJob {
  return {
    id: overrides.id ?? "job-1",
    job_type: overrides.job_type ?? "classify",
    status: overrides.status ?? "pending",
    book_id: overrides.book_id ?? "book-1",
    book_title: overrides.book_title ?? "Dune",
    created_at: overrides.created_at ?? "2026-04-20T12:00:00Z",
    started_at: overrides.started_at ?? null,
    completed_at: overrides.completed_at ?? null,
    error_text: overrides.error_text ?? null,
  };
}

function makeResponse(items: AdminJob[]): PaginatedResponse<AdminJob> {
  return {
    items,
    total: items.length,
    page: 1,
    page_size: 25,
  };
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <AdminJobsPage />
    </QueryClientProvider>,
  );
}

describe("AdminJobsPage", () => {
  beforeEach(() => {
    listAdminJobsMock.mockReset();
    cancelAdminJobMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  test("test_jobs_table_renders", async () => {
    listAdminJobsMock.mockResolvedValue(
      makeResponse([
        makeJob({ id: "job-11111111", book_title: "Dune" }),
        makeJob({ id: "job-22222222", book_title: "Neuromancer", status: "completed" }),
      ]),
    );

    renderPage();

    expect(await screen.findByText("Dune")).toBeTruthy();
    expect(screen.getByText("Neuromancer")).toBeTruthy();
  });

  test("test_cancel_job_calls_api", async () => {
    listAdminJobsMock.mockResolvedValue(makeResponse([makeJob({ id: "job-cancel-1", status: "pending" })]));
    cancelAdminJobMock.mockResolvedValue();

    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    await waitFor(() => {
      expect(cancelAdminJobMock).toHaveBeenCalledWith("job-cancel-1");
    });
  });

  test("test_running_job_shows_spinner", async () => {
    listAdminJobsMock.mockResolvedValue(
      makeResponse([
        makeJob({
          id: "job-run-1",
          status: "running",
          started_at: "2026-04-20T12:00:00Z",
        }),
      ]),
    );

    renderPage();

    expect(await screen.findByTestId("running-spinner")).toBeTruthy();
  });
});
