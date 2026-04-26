import { describe, expect, test } from "vitest";
import { http, HttpResponse } from "msw";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../test/render";
import { makeImportStatus } from "../../test/fixtures";
import { server } from "../../test/setup";
import { screen } from "@testing-library/react";

function renderImportPage() {
  return renderWithProviders(<></>, {
    initialPath: "/admin/import",
    authenticated: true,
  });
}

describe("ImportPage", () => {
  test("the file input accepts zip files", async () => {
    renderImportPage();

    const dropzone = await screen.findByTestId("import-dropzone");
    const fileInput = dropzone.querySelector("input[type='file']");
    expect(fileInput).toHaveAttribute("accept", ".zip");
  });

  test("uploading a file posts a FormData payload and shows completion", async () => {
    let resolveStart!: () => void;
    const startPending = new Promise<void>((resolve) => {
      resolveStart = resolve;
    });

    server.use(
      http.post("/api/v1/admin/import/bulk", async () => {
        await startPending;
        return HttpResponse.json({ job_id: "job-1" }, { status: 201 });
      }),
      http.get("/api/v1/admin/import/:id", () => HttpResponse.json(makeImportStatus())),
    );

    const user = userEvent.setup();
    renderImportPage();

    const file = new File(["zip"], "library.zip", { type: "application/zip" });
    const dropzone = await screen.findByTestId("import-dropzone");
    const fileInput = dropzone.querySelector("input[type='file']");
    expect(fileInput).toBeTruthy();
    await user.upload(fileInput as HTMLInputElement, file);
    await user.click(screen.getByRole("button", { name: /start import/i }));

    expect(await screen.findByRole("button", { name: /starting/i })).toBeTruthy();
    resolveStart();

    expect(await screen.findByText(/import complete: 1 books added/i)).toBeTruthy();
  });
});
