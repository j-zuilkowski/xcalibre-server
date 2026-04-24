import * as FileSystem from "expo-file-system";
import * as SecureStore from "expo-secure-store";
import { Alert } from "react-native";
import { useSyncExternalStore } from "react";
import type { ApiClient as CalibreClient } from "@autolibre/shared";
import type { SQLiteDatabase } from "expo-sqlite";
import { getAccessToken } from "./auth";
import { runMigrations } from "./db";

const DOWNLOADS_DIR = "books";
const LOW_STORAGE_THRESHOLD_BYTES = 200 * 1024 * 1024;
const PREFERRED_DOWNLOAD_FORMAT_KEY = "preferred_download_format";
const PREFERRED_FORMAT_ORDER = ["EPUB", "MOBI", "PDF"] as const;

type DownloadRow = {
  local_path: string | null;
};

export type DownloadSummary = {
  fileCount: number;
  usedBytes: number;
};

export type DownloadedBookRow = {
  bookId: string;
  title: string;
  coverUrl: string | null;
  hasCover: boolean;
  format: string;
  localPath: string;
  sizeBytes: number;
  downloadedAt: string;
};

export type DownloadFormatCandidate = {
  id?: string;
  format: string;
  size_bytes: number;
};

export type DownloadQueueItem = {
  key: string;
  bookId: string;
  title: string;
  coverUrl: string | null;
  hasCover: boolean;
  format: string;
  sizeBytes: number | null;
  status: "downloading" | "failed";
  progress: number;
  totalBytesWritten: number;
  totalBytesExpected: number;
  errorMessage: string | null;
  queuedAt: number;
};

export type DownloadContext = {
  title?: string;
  coverUrl?: string | null;
  hasCover?: boolean;
  sizeBytes?: number;
  skipStorageWarning?: boolean;
};

type DownloadProgress = {
  totalBytesWritten: number;
  totalBytesExpectedToWrite: number;
};

type DownloadHandle = {
  resumable: ReturnType<typeof FileSystem.createDownloadResumable>;
};

const downloadEntries = new Map<string, DownloadQueueItem>();
const downloadHandles = new Map<string, DownloadHandle>();
const cancelledDownloads = new Set<string>();
const listeners = new Set<() => void>();

export class DownloadCancelledError extends Error {
  constructor(message = "Download cancelled.") {
    super(message);
    this.name = "DownloadCancelledError";
  }
}

function normalizeFormat(format: string): string {
  return format.toUpperCase();
}

function downloadKey(bookId: string, format: string): string {
  return `${bookId}:${normalizeFormat(format)}`;
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

function formatQueueItem(item: DownloadQueueItem): DownloadQueueItem {
  return { ...item };
}

function getQueueSnapshot(): DownloadQueueItem[] {
  return Array.from(downloadEntries.values())
    .map(formatQueueItem)
    .sort((left, right) => {
      if (left.status !== right.status) {
        return left.status === "downloading" ? -1 : 1;
      }

      if (left.queuedAt !== right.queuedAt) {
        return left.queuedAt - right.queuedAt;
      }

      return left.title.localeCompare(right.title);
    });
}

function notifyListeners(): void {
  for (const listener of listeners) {
    listener();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function setQueueItem(key: string, item: DownloadQueueItem): void {
  downloadEntries.set(key, item);
  notifyListeners();
}

function updateQueueItem(
  key: string,
  updater: (current: DownloadQueueItem | undefined) => DownloadQueueItem | undefined,
): void {
  const current = downloadEntries.get(key);
  const next = updater(current);

  if (next) {
    downloadEntries.set(key, next);
  } else {
    downloadEntries.delete(key);
  }

  notifyListeners();
}

function removeQueueItem(key: string): void {
  if (downloadEntries.delete(key)) {
    notifyListeners();
  }
}

function toQueueItem(
  key: string,
  bookId: string,
  format: string,
  context: DownloadContext,
  status: DownloadQueueItem["status"],
  progress = 0,
  totalBytesWritten = 0,
  totalBytesExpected = 0,
  errorMessage: string | null = null,
): DownloadQueueItem {
  return {
    key,
    bookId,
    title: context.title?.trim().length ? context.title : bookId,
    coverUrl: context.coverUrl ?? null,
    hasCover: Boolean(context.hasCover && context.coverUrl),
    format: normalizeFormat(format),
    sizeBytes: typeof context.sizeBytes === "number" ? context.sizeBytes : null,
    status,
    progress: Math.max(0, Math.min(1, progress)),
    totalBytesWritten,
    totalBytesExpected,
    errorMessage,
    queuedAt: Date.now(),
  };
}

function normalizeProgress(value: DownloadProgress): number {
  if (!Number.isFinite(value.totalBytesExpectedToWrite) || value.totalBytesExpectedToWrite <= 0) {
    return 0;
  }

  return Math.max(
    0,
    Math.min(1, value.totalBytesWritten / value.totalBytesExpectedToWrite),
  );
}

async function confirmLowStorage(sizeBytes: number): Promise<boolean> {
  try {
    const freeBytes = await FileSystem.getFreeDiskStorageAsync();
    const remainingBytes = freeBytes - sizeBytes;

    if (remainingBytes >= LOW_STORAGE_THRESHOLD_BYTES) {
      return true;
    }

    return await new Promise<boolean>((resolve) => {
      Alert.alert(
        "Low storage",
        `Only ${formatBytes(Math.max(0, remainingBytes))} remaining. Continue?`,
        [
          {
            text: "Cancel",
            style: "cancel",
            onPress: () => resolve(false),
          },
          {
            text: "Download",
            onPress: () => resolve(true),
          },
        ],
        {
          cancelable: true,
          onDismiss: () => resolve(false),
        },
      );
    });
  } catch {
    return true;
  }
}

async function getPreferredDownloadFormatPreference(): Promise<string | null> {
  const value = await SecureStore.getItemAsync(PREFERRED_DOWNLOAD_FORMAT_KEY);
  if (!value) {
    return null;
  }

  const normalized = value.trim().toUpperCase();
  return normalized.length > 0 ? normalized : null;
}

export async function setPreferredDownloadFormatPreference(format: string | null): Promise<void> {
  if (!format) {
    await SecureStore.deleteItemAsync(PREFERRED_DOWNLOAD_FORMAT_KEY);
    return;
  }

  await SecureStore.setItemAsync(PREFERRED_DOWNLOAD_FORMAT_KEY, normalizeFormat(format));
}

export function useDownloadQueue(): DownloadQueueItem[] {
  return useSyncExternalStore(subscribe, getQueueSnapshot, getQueueSnapshot);
}

export function formatBytes(sizeBytes: number): string {
  if (!Number.isFinite(sizeBytes) || sizeBytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = sizeBytes;
  let index = 0;

  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }

  const decimals = size >= 10 || index === 0 ? 0 : 1;
  return `${size.toFixed(decimals)} ${units[index]}`;
}

export function resolvePreferredDownloadFormat(
  formats: DownloadFormatCandidate[],
  preferredFormat?: string | null,
): DownloadFormatCandidate | null {
  if (formats.length === 0) {
    return null;
  }

  const normalizedPreferred = preferredFormat?.trim().toUpperCase() ?? null;
  if (normalizedPreferred) {
    const preferredMatch = formats.find(
      (format) => normalizeFormat(format.format) === normalizedPreferred,
    );

    if (preferredMatch) {
      return preferredMatch;
    }
  }

  for (const candidate of PREFERRED_FORMAT_ORDER) {
    const match = formats.find((format) => normalizeFormat(format.format) === candidate);
    if (match) {
      return match;
    }
  }

  return formats[0] ?? null;
}

export async function getDownloadSummary(database: SQLiteDatabase): Promise<DownloadSummary> {
  await runMigrations(database);

  const row = await database.getFirstAsync<{
    file_count: number | null;
    used_bytes: number | null;
  }>(
    `SELECT
      COUNT(*) AS file_count,
      COALESCE(SUM(size_bytes), 0) AS used_bytes
     FROM local_downloads`,
  );

  return {
    fileCount: row?.file_count ?? 0,
    usedBytes: row?.used_bytes ?? 0,
  };
}

export async function listDownloadedBooks(
  database: SQLiteDatabase,
): Promise<DownloadedBookRow[]> {
  await runMigrations(database);

  const rows = await database.getAllAsync<{
    book_id: string;
    format: string;
    local_path: string;
    size_bytes: number | null;
    downloaded_at: string;
    title: string | null;
    cover_url: string | null;
    has_cover: number | null;
  }>(
    `SELECT
      d.book_id,
      d.format,
      d.local_path,
      d.size_bytes,
      d.downloaded_at,
      b.title,
      b.cover_url,
      b.has_cover
     FROM local_downloads d
     LEFT JOIN local_books b ON b.id = d.book_id
     ORDER BY d.downloaded_at DESC, b.sort_title COLLATE NOCASE ASC, b.title ASC`,
  );

  return rows.map((row) => ({
    bookId: row.book_id,
    title: row.title ?? row.book_id,
    coverUrl: row.cover_url,
    hasCover: row.has_cover === 1,
    format: normalizeFormat(row.format),
    localPath: row.local_path,
    sizeBytes: row.size_bytes ?? 0,
    downloadedAt: row.downloaded_at,
  }));
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

export async function downloadBook(
  client: CalibreClient,
  database: SQLiteDatabase,
  bookId: string,
  format: string,
  context: DownloadContext = {},
): Promise<{ localPath: string }> {
  await runMigrations(database);

  const normalizedFormat = normalizeFormat(format);
  const localPath = downloadPath(bookId, normalizedFormat);
  const key = downloadKey(bookId, normalizedFormat);
  const accessToken = await getAccessToken();

  if (!accessToken) {
    throw new Error("You must be signed in to download books.");
  }

  if (!context.skipStorageWarning && typeof context.sizeBytes === "number") {
    const canContinue = await confirmLowStorage(context.sizeBytes);
    if (!canContinue) {
      throw new DownloadCancelledError();
    }
  }

  await FileSystem.makeDirectoryAsync(booksDirectory(), { intermediates: true });

  const metadata = toQueueItem(key, bookId, normalizedFormat, context, "downloading");
  const progressCallback = (progress: DownloadProgress) => {
    updateQueueItem(key, (current) => {
      if (!current) {
        return current;
      }

      return {
        ...current,
        progress: normalizeProgress(progress),
        totalBytesWritten: progress.totalBytesWritten,
        totalBytesExpected: progress.totalBytesExpectedToWrite,
      };
    });
  };

  const resumable = FileSystem.createDownloadResumable(
    client.downloadUrl(bookId, normalizedFormat),
    localPath,
    {
      headers: {
        Authorization: `Bearer ${accessToken}`,
      },
    },
    progressCallback,
  );

  downloadHandles.set(key, { resumable });
  setQueueItem(key, metadata);

  try {
    const result = await resumable.downloadAsync();

    if (!result) {
      await FileSystem.deleteAsync(localPath, { idempotent: true });
      cancelledDownloads.delete(key);
      removeQueueItem(key);
      throw new DownloadCancelledError();
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

    removeQueueItem(key);
    return { localPath };
  } catch (error) {
    const message = error instanceof Error ? error.message : "Unknown download error.";

    if (error instanceof DownloadCancelledError || cancelledDownloads.has(key)) {
      cancelledDownloads.delete(key);
      removeQueueItem(key);
      throw new DownloadCancelledError(message);
    }

    await FileSystem.deleteAsync(localPath, { idempotent: true });
    updateQueueItem(key, (current) => {
      if (!current) {
        return current;
      }

      return {
        ...current,
        status: "failed",
        errorMessage: message,
      };
    });

    throw new Error(`Failed to download ${bookId}.${normalizedFormat}: ${message}`);
  } finally {
    downloadHandles.delete(key);
    cancelledDownloads.delete(key);
  }
}

export async function cancelDownload(bookId: string, format: string): Promise<void> {
  const key = downloadKey(bookId, format);
  const handle = downloadHandles.get(key);
  cancelledDownloads.add(key);

  if (!handle) {
    removeQueueItem(key);
    cancelledDownloads.delete(key);
    return;
  }

  await handle.resumable.cancelAsync();
  await FileSystem.deleteAsync(downloadPath(bookId, normalizeFormat(format)), {
    idempotent: true,
  });
  downloadHandles.delete(key);
  removeQueueItem(key);
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

export async function getPreferredDownloadFormat(): Promise<string | null> {
  return await getPreferredDownloadFormatPreference();
}
