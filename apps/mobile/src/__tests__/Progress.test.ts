import * as SQLite from "expo-sqlite";
import { loadProgress, saveProgress } from "../lib/progress";

describe("progress", () => {
  it("test_save_progress_posts_to_server", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    const mockPost = vi.fn().mockResolvedValue({
      ok: true,
    });
    const client = {
      post: mockPost,
    } as never;

    await saveProgress(client, database, "book-1", "EPUB", {
      cfi: "epubcfi(/6/12!/4/2/8)",
      percentage: 0.42,
    });

    expect(mockPost).toHaveBeenCalledWith("/api/v1/progress/book-1", {
      cfi: "epubcfi(/6/12!/4/2/8)",
      percentage: 0.42,
    });
  });

  it("test_save_progress_survives_network_error", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    const client = {
      post: vi.fn().mockRejectedValue(new Error("network down")),
    } as never;

    await expect(
      saveProgress(client, database, "book-1", "PDF", {
        page: 12,
        percentage: 0.25,
      }),
    ).resolves.toBeUndefined();
  });

  it("test_load_progress_returns_null_on_error", async () => {
    const client = {
      get: vi.fn().mockRejectedValue(new Error("timeout")),
    } as never;

    await expect(loadProgress(client, "book-1")).resolves.toBeNull();
  });
});
