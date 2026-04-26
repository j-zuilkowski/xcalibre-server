import * as SQLite from "expo-sqlite";
import type { SQLiteDatabase } from "expo-sqlite";
import type { BookSummary, DocumentType, SeriesRef } from "@xs/shared";

export const db = SQLite.openDatabaseAsync("calibre_local.db");

const CREATE_LOCAL_BOOKS_TABLE = `
  CREATE TABLE IF NOT EXISTS local_books (
    id TEXT PRIMARY KEY,
    title TEXT,
    sort_title TEXT,
    authors_json TEXT,
    cover_url TEXT,
    has_cover INTEGER,
    language TEXT,
    rating INTEGER,
    document_type TEXT,
    series_json TEXT,
    last_modified TEXT,
    synced_at TEXT
  );
`;

const CREATE_LOCAL_SYNC_STATE_TABLE = `
  CREATE TABLE IF NOT EXISTS local_sync_state (
    key TEXT PRIMARY KEY,
    value TEXT
  );
`;

const CREATE_LOCAL_DOWNLOADS_TABLE = `
  CREATE TABLE IF NOT EXISTS local_downloads (
    book_id TEXT,
    format TEXT,
    local_path TEXT,
    size_bytes INTEGER,
    downloaded_at TEXT,
    PRIMARY KEY (book_id, format)
  );
`;

type LocalBookRow = {
  id: string;
  title: string;
  sort_title: string;
  authors_json: string;
  cover_url: string | null;
  has_cover: number | null;
  language: string | null;
  rating: number | null;
  document_type: DocumentType;
  series_json: string | null;
  last_modified: string;
  synced_at: string;
};

function parseJsonArray<T>(value: string | null, fallback: T[]): T[] {
  if (!value) {
    return fallback;
  }

  try {
    const parsed = JSON.parse(value) as T[];
    return Array.isArray(parsed) ? parsed : fallback;
  } catch {
    return fallback;
  }
}

function parseJsonObject<T>(value: string | null): T | null {
  if (!value) {
    return null;
  }

  try {
    const parsed = JSON.parse(value) as T | null;
    return parsed ?? null;
  } catch {
    return null;
  }
}

function rowToBookSummary(row: LocalBookRow): BookSummary {
  return {
    id: row.id,
    title: row.title,
    sort_title: row.sort_title,
    authors: parseJsonArray(row.authors_json, []),
    series: parseJsonObject<SeriesRef>(row.series_json),
    series_index: null,
    cover_url: row.cover_url,
    has_cover: row.has_cover === 1,
    is_read: false,
    is_archived: false,
    language: row.language,
    rating: row.rating,
    document_type: row.document_type,
    last_modified: row.last_modified,
  };
}

export async function runMigrations(database?: SQLiteDatabase): Promise<void> {
  const targetDb = database ?? (await db);

  await targetDb.execAsync(`
    ${CREATE_LOCAL_BOOKS_TABLE}
    ${CREATE_LOCAL_SYNC_STATE_TABLE}
    ${CREATE_LOCAL_DOWNLOADS_TABLE}
  `);
}

export async function listLocalBooks(database?: SQLiteDatabase): Promise<BookSummary[]> {
  const targetDb = database ?? (await db);
  await runMigrations(targetDb);

  const rows = await targetDb.getAllAsync<LocalBookRow>(
    `SELECT
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
     FROM local_books
     ORDER BY sort_title COLLATE NOCASE ASC, title ASC`,
  );

  return rows.map(rowToBookSummary);
}
