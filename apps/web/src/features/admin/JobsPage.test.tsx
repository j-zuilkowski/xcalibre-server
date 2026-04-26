import { describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { makeAdminUser, makeJob } from "../../test/fixtures";
import { server } from "../../test/setup";
import { screen, waitFor } from "@testing-library/react";

function renderJobsPage() {
  return renderWithProviders(<></>, {
    initialPath: "/admin/jobs",
    authenticated: true,
    user: makeAdminUser(),
  });
}

describe("JobsPage", () => {
  test("renders the job list", async () => {
    renderJobsPage();

    expect(await screen.findByRole("cell", { name: /import/i })).toBeTruthy();
    expect(await screen.findByRole("cell", { name: /pending/i })).toBeTruthy();
  });

  test("failed jobs show their status text", async () => {
    server.use(
      http.get("/api/v1/admin/jobs", () =>
        HttpResponse.json({
          items: [makeJob({ status: "failed" })],
          total: 1,
          page: 1,
          page_size: 25,
        }),
      ),
    );

    renderJobsPage();

    expect(await screen.findByText(/failed/i)).toBeTruthy();
  });

  test("canceling a pending job calls the delete endpoint", async () => {
    let cancelledPath = "";
    server.use(
      http.delete("/api/v1/admin/jobs/:id", ({ params }) => {
        cancelledPath = String(params.id);
        return HttpResponse.json(null, { status: 204 });
      }),
    );

    const user = userEvent.setup();
    renderJobsPage();

    await user.click(await screen.findByRole("button", { name: /cancel/i }));

    await waitFor(() => {
      expect(cancelledPath).toBe("job-1");
    });
  });
});
