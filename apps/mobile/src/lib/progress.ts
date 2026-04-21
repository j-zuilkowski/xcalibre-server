import type { ApiClient as CalibreClient, ReadingProgress } from "@calibre/shared";
import type { SQLiteDatabase } from "expo-sqlite";
import { runMigrations } from "./db";

type ProgressPayload = {
  cfi?: string;
  page?: number;
  percentage: number;
};

type ProgressResponse = {
  cfi?: string | null;
  page?: number | null;
  percentage?: number | null;
};

type ProgressApiClient = CalibreClient & {
  get?: (path: string) => Promise<unknown>;
  post?: (path: string, body: Record<string, unknown>) => Promise<unknown>;
};

function progressKey(bookId: string): string {
  return `progress_${bookId}`;
}

function toStoredProgress(formatId: string, data: ProgressPayload): string {
  return JSON.stringify({
    format_id: formatId,
    cfi: data.cfi ?? null,
    page: typeof data.page === "number" ? data.page : null,
    percentage: data.percentage,
    updated_at: new Date().toISOString(),
  });
}

function normalizeProgress(data: unknown): { cfi?: string; page?: number; percentage: number } | null {
  if (!data || typeof data !== "object") {
    return null;
  }

  const candidate = data as ProgressResponse;
  if (typeof candidate.percentage !== "number") {
    return null;
  }

  return {
    cfi: typeof candidate.cfi === "string" ? candidate.cfi : undefined,
    page: typeof candidate.page === "number" ? candidate.page : undefined,
    percentage: candidate.percentage,
  };
}

async function postProgress(
  client: ProgressApiClient,
  bookId: string,
  formatId: string,
  data: ProgressPayload,
): Promise<void> {
  const payload: Record<string, unknown> = {
    percentage: data.percentage,
  };

  if (typeof data.cfi === "string") {
    payload.cfi = data.cfi;
  }
  if (typeof data.page === "number") {
    payload.page = data.page;
  }

  if (typeof client.post === "function") {
    await client.post(`/api/v1/progress/${encodeURIComponent(bookId)}`, payload);
    return;
  }

  if (typeof client.patchReadingProgress === "function") {
    await client.patchReadingProgress(bookId, {
      format_id: formatId,
      cfi: data.cfi ?? null,
      page: typeof data.page === "number" ? data.page : null,
      percentage: data.percentage,
    });
  }
}

export async function saveProgress(
  client: ProgressApiClient,
  database: SQLiteDatabase,
  bookId: string,
  formatId: string,
  data: ProgressPayload,
): Promise<void> {
  await runMigrations(database);

  await database.runAsync(
    "INSERT OR REPLACE INTO local_sync_state (key, value) VALUES (?, ?)",
    [progressKey(bookId), toStoredProgress(formatId, data)],
  );

  void postProgress(client, bookId, formatId, data).catch(() => undefined);
}

export async function loadProgress(
  client: ProgressApiClient,
  bookId: string,
): Promise<{ cfi?: string; page?: number; percentage: number } | null> {
  try {
    if (typeof client.get === "function") {
      const response = await client.get(`/api/v1/progress/${encodeURIComponent(bookId)}`);
      return normalizeProgress(response);
    }

    if (typeof client.getReadingProgress === "function") {
      const response = (await client.getReadingProgress(bookId)) as ReadingProgress | null;
      return normalizeProgress(response);
    }
  } catch {
    return null;
  }

  return null;
}
