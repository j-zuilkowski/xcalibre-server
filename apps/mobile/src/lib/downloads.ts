import * as FileSystem from "expo-file-system";
import type { ApiClient as CalibreClient } from "@calibre/shared";
import type { SQLiteDatabase } from "expo-sqlite";
import { getAccessToken } from "./auth";
import { runMigrations } from "./db";

const DOWNLOADS_DIR = "books";

type DownloadRow = {
  local_path: string | null;
};

function normalizeFormat(format: string): string {
  return format.toUpperCase();
}

function booksDirectory(): string {
  const baseDirectory = FileSystem.documentDirectory;
  if (!baseDirectory) {
    throw new Error("Document directory is unavailable.");
  }
  return `${baseDirectory}${DOWNLOADS_DIR}`;
}

function downloadPath(bookId: string, format: string): string {
  return `${booksDirectory()}/${bookId}.${format.toLowerCase()}`;
}

export async function downloadBook(
  client: CalibreClient,
  database: SQLiteDatabase,
  bookId: string,
  format: string,
): Promise<{ localPath: string }> {
  await runMigrations(database);

  const normalizedFormat = normalizeFormat(format);
  const localPath = downloadPath(bookId, normalizedFormat);
  const accessToken = await getAccessToken();

  if (!accessToken) {
    throw new Error("You must be signed in to download books.");
  }

  await FileSystem.makeDirectoryAsync(booksDirectory(), { intermediates: true });

  try {
    const result = await FileSystem.downloadAsync(client.downloadUrl(bookId, normalizedFormat), localPath, {
      headers: {
        Authorization: `Bearer ${accessToken}`,
      },
    });

    if (!result) {
      throw new Error("Download was cancelled.");
    }

    const info = await FileSystem.getInfoAsync(localPath, { size: true });
    const sizeBytes = info.exists && typeof info.size === "number" ? info.size : 0;
    const downloadedAt = new Date().toISOString();

    await database.runAsync(
      `INSERT OR REPLACE INTO local_downloads (
        book_id,
        format,
        local_path,
        size_bytes,
        downloaded_at
      ) VALUES (?, ?, ?, ?, ?)`,
      [bookId, normalizedFormat, localPath, sizeBytes, downloadedAt],
    );

    return { localPath };
  } catch (error) {
    const message = error instanceof Error ? error.message : "Unknown download error.";
    throw new Error(`Failed to download ${bookId}.${normalizedFormat}: ${message}`);
  }
}

export async function getLocalPath(
  database: SQLiteDatabase,
  bookId: string,
  format: string,
): Promise<string | null> {
  await runMigrations(database);

  const row = await database.getFirstAsync<DownloadRow>(
    "SELECT local_path FROM local_downloads WHERE book_id = ? AND format = ?",
    [bookId, normalizeFormat(format)],
  );

  return row?.local_path ?? null;
}

export async function deleteDownload(
  database: SQLiteDatabase,
  bookId: string,
  format: string,
): Promise<void> {
  await runMigrations(database);

  const normalizedFormat = normalizeFormat(format);
  const row = await database.getFirstAsync<DownloadRow>(
    "SELECT local_path FROM local_downloads WHERE book_id = ? AND format = ?",
    [bookId, normalizedFormat],
  );

  if (row?.local_path) {
    await FileSystem.deleteAsync(row.local_path, { idempotent: true });
  }

  await database.runAsync("DELETE FROM local_downloads WHERE book_id = ? AND format = ?", [
    bookId,
    normalizedFormat,
  ]);
}
