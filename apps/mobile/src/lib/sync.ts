import type { ApiClient, BookSummary, DocumentType } from "@xs/shared";
import type { SQLiteDatabase } from "expo-sqlite";
import { runMigrations } from "./db";

const PAGE_SIZE = 200;
const LAST_SYNC_KEY = "last_sync_at";

type SyncStateRow = {
  value: string | null;
};

function serializeSeries(series: BookSummary["series"]): string | null {
  return series ? JSON.stringify(series) : null;
}

function toDocumentType(value: BookSummary["document_type"]): DocumentType {
  return value;
}

async function getLastSyncAt(database: SQLiteDatabase): Promise<string | null> {
  const row = await database.getFirstAsync<SyncStateRow>(
    "SELECT value FROM local_sync_state WHERE key = ?",
    [LAST_SYNC_KEY],
  );
  return row?.value ?? null;
}

export async function syncLibrary(
  client: ApiClient,
  database: SQLiteDatabase,
): Promise<{ synced: number; total: number }> {
  await runMigrations(database);

  const lastSyncAt = await getLastSyncAt(database);
  const fetchedBooks: BookSummary[] = [];
  let total = 0;

  try {
    let page = 1;

    while (true) {
      const response = await client.listBooks({
        since: lastSyncAt ?? undefined,
        page_size: PAGE_SIZE,
        page,
      });

      total = response.total;
      fetchedBooks.push(...response.items);

      if (fetchedBooks.length >= total || response.items.length === 0) {
        break;
      }

      page += 1;
    }
  } catch {
    return { synced: 0, total: 0 };
  }

  const syncedAt = new Date().toISOString();

  await database.withTransactionAsync(async () => {
    for (const book of fetchedBooks) {
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
          book.id,
          book.title,
          book.sort_title,
          JSON.stringify(book.authors),
          book.cover_url,
          book.has_cover ? 1 : 0,
          book.language,
          book.rating,
          toDocumentType(book.document_type),
          serializeSeries(book.series),
          book.last_modified,
          syncedAt,
        ],
      );
    }

    await database.runAsync(
      "INSERT OR REPLACE INTO local_sync_state (key, value) VALUES (?, ?)",
      [LAST_SYNC_KEY, syncedAt],
    );
  });

  return { synced: fetchedBooks.length, total };
}
