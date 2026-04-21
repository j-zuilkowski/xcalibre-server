import * as FileSystem from "expo-file-system";
import * as SQLite from "expo-sqlite";
import { deleteDownload, downloadBook, getLocalPath } from "../lib/downloads";
import { runMigrations } from "../lib/db";

const { mockGetAccessToken } = vi.hoisted(() => ({
  mockGetAccessToken: vi.fn(),
}));

vi.mock("../lib/auth", () => ({
  getAccessToken: mockGetAccessToken,
}));

function createClient() {
  return {
    downloadUrl: vi.fn((bookId: string, format: string) => {
      return `http://example.test/api/v1/books/${bookId}/formats/${format}/download`;
    }),
  } as never;
}

describe("downloads", () => {
  beforeEach(() => {
    mockGetAccessToken.mockReset();
    mockGetAccessToken.mockResolvedValue("access-token");
    vi.mocked(FileSystem.downloadAsync).mockResolvedValue({
      uri: "file:///documents/books/book-1.epub",
      status: 200,
      headers: {},
      mimeType: null,
    } as never);
    vi.mocked(FileSystem.deleteAsync).mockResolvedValue(undefined);
    vi.mocked(FileSystem.makeDirectoryAsync).mockResolvedValue(undefined);
    vi.mocked(FileSystem.getInfoAsync).mockResolvedValue({
      exists: true,
      isDirectory: false,
      size: 1234,
      uri: "file:///documents/books/book-1.epub",
    } as never);
  });

  it("test_download_stores_path", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);
    const client = createClient();

    const result = await downloadBook(client, database, "book-1", "EPUB");

    expect(result.localPath).toBe("file:///documents/books/book-1.epub");
    expect(vi.mocked(FileSystem.downloadAsync)).toHaveBeenCalledWith(
      "http://example.test/api/v1/books/book-1/formats/EPUB/download",
      "file:///documents/books/book-1.epub",
      {
        headers: {
          Authorization: "Bearer access-token",
        },
      },
    );

    const row = await database.getFirstAsync<{ local_path: string; size_bytes: number }>(
      "SELECT local_path, size_bytes FROM local_downloads WHERE book_id = ? AND format = ?",
      ["book-1", "EPUB"],
    );

    expect(row?.local_path).toBe("file:///documents/books/book-1.epub");
    expect(row?.size_bytes).toBe(1234);
  });

  it("test_get_local_path_returns_null_when_not_downloaded", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);

    await expect(getLocalPath(database, "book-1", "EPUB")).resolves.toBeNull();
  });

  it("test_delete_removes_file_and_row", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);
    const client = createClient();

    await downloadBook(client, database, "book-1", "EPUB");
    await deleteDownload(database, "book-1", "EPUB");

    expect(vi.mocked(FileSystem.deleteAsync)).toHaveBeenCalledWith(
      "file:///documents/books/book-1.epub",
      {
        idempotent: true,
      },
    );

    const row = await database.getFirstAsync<{ local_path: string }>(
      "SELECT local_path FROM local_downloads WHERE book_id = ? AND format = ?",
      ["book-1", "EPUB"],
    );

    expect(row).toBeNull();
  });
});
