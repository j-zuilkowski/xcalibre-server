/**
 * Integration tests for the download queue and local file management module.
 *
 * Tests cover:
 * - `formatBytes` display helper
 * - `resolvePreferredDownloadFormat` format selection logic
 * - `downloadBook` happy path (file written, SQLite row inserted, auth header set)
 * - Low-storage warning alert flow
 * - `getLocalPath` returns null when not downloaded
 * - `deleteDownload` removes the file and the SQLite row
 * - `getDownloadSummary` and `listDownloadedBooks` return correct data
 * - `downloadBook` throws `DownloadCancelledError` when `downloadAsync` returns undefined
 *
 * All `expo-file-system` and `expo-secure-store` calls are mocked at the module
 * level. Each test uses an in-memory SQLite database (`:memory:`) with migrations
 * applied so the schema is always fresh.
 */
import { Alert } from "react-native";
import * as FileSystem from "expo-file-system";
import * as SQLite from "expo-sqlite";
import {
  DownloadCancelledError,
  deleteDownload,
  downloadBook,
  formatBytes,
  getDownloadSummary,
  getLocalPath,
  listDownloadedBooks,
  resolvePreferredDownloadFormat,
} from "../lib/downloads";
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

function createResumableMock() {
  return {
    cancelAsync: vi.fn(async () => undefined),
    downloadAsync: vi.fn(async () => ({
      uri: "file:///documents/books/book-1.epub",
      status: 200,
      headers: {},
      mimeType: null,
    })),
    pauseAsync: vi.fn(async () => ({
      url: "http://example.test",
      fileUri: "file:///documents/books/book-1.epub",
      options: {},
      resumeData: null,
    })),
    resumeAsync: vi.fn(async () => ({
      uri: "file:///documents/books/book-1.epub",
      status: 200,
      headers: {},
      mimeType: null,
    })),
    savable: vi.fn(() => ({
      url: "http://example.test",
      fileUri: "file:///documents/books/book-1.epub",
      options: {},
      resumeData: null,
    })),
  } as never;
}

/**
 * Test suite for the downloads module.
 * Each test gets a fresh in-memory SQLite database and reset mock state.
 */
describe("downloads", () => {
  beforeEach(() => {
    mockGetAccessToken.mockReset();
    mockGetAccessToken.mockResolvedValue("access-token");
    vi.mocked(FileSystem.createDownloadResumable).mockReturnValue(createResumableMock());
    vi.mocked(FileSystem.deleteAsync).mockResolvedValue(undefined);
    vi.mocked(FileSystem.makeDirectoryAsync).mockResolvedValue(undefined);
    vi.mocked(FileSystem.getInfoAsync).mockResolvedValue({
      exists: true,
      isDirectory: false,
      size: 1234,
      uri: "file:///documents/books/book-1.epub",
    } as never);
    vi.mocked(FileSystem.getFreeDiskStorageAsync).mockResolvedValue(10 * 1024 * 1024 * 1024);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  /** Verifies human-readable byte formatting for zero, KB, and MB values. */
  it("formats bytes for display", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(3_456_789)).toBe("3.3 MB");
  });

  /**
   * Verifies that an explicit preference (e.g. "mobi") overrides the default order,
   * and that null preference falls back to EPUB as the highest-priority default.
   */
  it("resolves preferred formats with explicit preference first", () => {
    const formats = [
      { id: "1", format: "PDF", size_bytes: 20 },
      { id: "2", format: "EPUB", size_bytes: 10 },
      { id: "3", format: "MOBI", size_bytes: 15 },
    ];

    expect(resolvePreferredDownloadFormat(formats, "mobi")?.format).toBe("MOBI");
    expect(resolvePreferredDownloadFormat(formats, null)?.format).toBe("EPUB");
  });

  /**
   * Verifies that a successful download:
   * - Returns the correct local file path
   * - Calls `FileSystem.createDownloadResumable` with the correct server URL and Bearer token
   * - Inserts a row with the local path and file size into `local_downloads`
   */
  it("test_download_stores_path", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);
    const client = createClient();

    const result = await downloadBook(client, database, "book-1", "EPUB", {
      title: "Example Book",
      coverUrl: "http://example.test/cover.jpg",
      hasCover: true,
      sizeBytes: 1234,
      skipStorageWarning: true,
    });

    expect(result.localPath).toBe("file:///documents/books/book-1.epub");
    expect(vi.mocked(FileSystem.createDownloadResumable)).toHaveBeenCalledWith(
      "http://example.test/api/v1/books/book-1/formats/EPUB/download",
      "file:///documents/books/book-1.epub",
      {
        headers: {
          Authorization: "Bearer access-token",
        },
      },
      expect.any(Function),
    );

    const row = await database.getFirstAsync<{ local_path: string; size_bytes: number }>(
      "SELECT local_path, size_bytes FROM local_downloads WHERE book_id = ? AND format = ?",
      ["book-1", "EPUB"],
    );

    expect(row?.local_path).toBe("file:///documents/books/book-1.epub");
    expect(row?.size_bytes).toBe(1234);
  });

  /**
   * Verifies that when free disk space minus the file size falls below 200 MB,
   * an `Alert.alert` is shown and the download proceeds if the user confirms.
   */
  it("test_download_prompts_on_low_storage", async () => {
    vi.mocked(FileSystem.getFreeDiskStorageAsync).mockResolvedValue(100 * 1024 * 1024);

    const alertSpy = vi.spyOn(Alert, "alert").mockImplementation((_title, _message, buttons) => {
      buttons?.find((button) => button.text === "Download")?.onPress?.();
    });

    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);
    const client = createClient();

    await downloadBook(client, database, "book-1", "EPUB", {
      title: "Example Book",
      sizeBytes: 80 * 1024 * 1024,
    });

    expect(alertSpy).toHaveBeenCalled();
    alertSpy.mockRestore();
  });

  /** Verifies that `getLocalPath` returns null for a book that has not been downloaded. */
  it("test_get_local_path_returns_null_when_not_downloaded", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);

    await expect(getLocalPath(database, "book-1", "EPUB")).resolves.toBeNull();
  });

  /**
   * Verifies that `deleteDownload` calls `FileSystem.deleteAsync` with the correct
   * path and removes the corresponding row from `local_downloads`.
   */
  it("test_delete_removes_file_and_row", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);
    const client = createClient();

    await downloadBook(client, database, "book-1", "EPUB", {
      title: "Example Book",
      sizeBytes: 1234,
      skipStorageWarning: true,
    });
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

  /**
   * Verifies that after downloading a book:
   * - `getDownloadSummary` returns the correct file count and total bytes
   * - `listDownloadedBooks` returns a row matching the downloaded book's metadata
   * This test also seeds `local_books` to exercise the LEFT JOIN in `listDownloadedBooks`.
   */
  it("test_summary_and_listing_include_downloaded_rows", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);
    const client = createClient();

    await database.runAsync(
      `INSERT OR REPLACE INTO local_books (
        id,
        title,
        sort_title,
        authors_json,
        cover_url,
        has_cover,
        language,
        rating,
        document_type,
        series_json,
        last_modified,
        synced_at
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        "book-1",
        "Example Book",
        "Example Book",
        "[]",
        "http://example.test/cover.jpg",
        1,
        "en",
        4,
        "novel",
        null,
        "2024-01-01T00:00:00Z",
        "2024-01-01T00:00:00Z",
      ],
    );

    await downloadBook(client, database, "book-1", "EPUB", {
      title: "Example Book",
      coverUrl: "http://example.test/cover.jpg",
      hasCover: true,
      sizeBytes: 1234,
      skipStorageWarning: true,
    });

    await expect(getDownloadSummary(database)).resolves.toEqual({
      fileCount: 1,
      usedBytes: 1234,
    });

    await expect(listDownloadedBooks(database)).resolves.toEqual([
      expect.objectContaining({
        bookId: "book-1",
        title: "Example Book",
        format: "EPUB",
        sizeBytes: 1234,
      }),
    ]);
  });

  /**
   * Verifies that when `FileSystem.downloadAsync` returns undefined (OS-level cancel),
   * `downloadBook` throws `DownloadCancelledError` rather than a generic Error.
   */
  it("test_cancel_download_throws_cancelled_error", async () => {
    const database = await SQLite.openDatabaseAsync(":memory:");
    await runMigrations(database);
    const client = createClient();

    vi.mocked(FileSystem.createDownloadResumable).mockReturnValue({
      cancelAsync: vi.fn(async () => undefined),
      downloadAsync: vi.fn(async () => undefined),
      pauseAsync: vi.fn(async () => ({
        url: "http://example.test",
        fileUri: "file:///documents/books/book-1.epub",
        options: {},
        resumeData: null,
      })),
      resumeAsync: vi.fn(async () => undefined),
      savable: vi.fn(() => ({
        url: "http://example.test",
        fileUri: "file:///documents/books/book-1.epub",
        options: {},
        resumeData: null,
      })),
    } as never);

    const promise = downloadBook(client, database, "book-1", "EPUB", {
      title: "Example Book",
      sizeBytes: 1234,
      skipStorageWarning: true,
    });

    await expect(promise).rejects.toBeInstanceOf(DownloadCancelledError);
  });
});
