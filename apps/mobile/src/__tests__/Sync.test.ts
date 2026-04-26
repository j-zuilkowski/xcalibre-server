import * as SQLite from "expo-sqlite";
import type { BookSummary } from "@xs/shared";
import { runMigrations } from "../lib/db";
import { syncLibrary } from "../lib/sync";

function makeBook(id: string, title: string, lastModified = "2026-01-01T00:00:00Z"): BookSummary {
  return {
    id,
    title,
    sort_title: title,
    authors: [{ id: `${id}-author`, name: "Author", sort_name: "Author" }],
    series: null,
    series_index: null,
    cover_url: null,
    has_cover: false,
    is_read: false,
    is_archived: false,
    language: "en",
    rating: 8,
    document_type: "novel",
    last_modified: lastModified,
  };
}

describe("syncLibrary", () => {
  it("test_sync_upserts_books", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);

    const mockListBooks = vi.fn().mockResolvedValue({
      items: [makeBook("1", "Book One")],
      total: 1,
      page: 1,
      page_size: 200,
    });

    const client = {
      listBooks: mockListBooks,
    } as never;

    await expect(syncLibrary(client, database)).resolves.toEqual({
      synced: 1,
      total: 1,
    });

    const rows = await database.getAllAsync<{
      id: string;
      title: string;
      authors_json: string;
      synced_at: string;
    }>("SELECT id, title, authors_json, synced_at FROM local_books");

    expect(rows).toHaveLength(1);
    expect(rows[0].id).toBe("1");
    expect(rows[0].title).toBe("Book One");
    expect(JSON.parse(rows[0].authors_json)).toEqual([
      { id: "1-author", name: "Author", sort_name: "Author" },
    ]);
    expect(rows[0].synced_at).toBeTruthy();
  });

  it("test_sync_incremental", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);

    const mockListBooks = vi
      .fn()
      .mockResolvedValueOnce({
        items: [makeBook("1", "Book One")],
        total: 1,
        page: 1,
        page_size: 200,
      })
      .mockResolvedValueOnce({
        items: [makeBook("1", "Book One Updated", "2026-01-02T00:00:00Z")],
        total: 1,
        page: 1,
        page_size: 200,
      });

    const client = {
      listBooks: mockListBooks,
    } as never;

    await syncLibrary(client, database);

    const syncState = await database.getFirstAsync<{ value: string }>(
      "SELECT value FROM local_sync_state WHERE key = ?",
      ["last_sync_at"],
    );

    await syncLibrary(client, database);

    expect(mockListBooks.mock.calls[1][0].since).toBe(syncState?.value);
  });

  it("test_sync_survives_network_error", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);

    const mockListBooks = vi.fn().mockRejectedValue(new Error("network down"));

    const client = {
      listBooks: mockListBooks,
    } as never;

    await expect(syncLibrary(client, database)).resolves.toEqual({
      synced: 0,
      total: 0,
    });
  });
});
