import type { AuthSession, Book } from "@xs/shared";
import { readFile } from "node:fs/promises";
import path from "node:path";

const API = process.env.PLAYWRIGHT_API_URL ?? "http://127.0.0.1:8083";

export async function uploadFixtureBook(
  session: AuthSession,
  filePath: string,
  metadata: Record<string, unknown> = {},
): Promise<Book> {
  const buffer = await readFile(filePath);
  const formData = new FormData();
  formData.append(
    "file",
    new Blob([buffer], { type: "application/epub+zip" }),
    path.basename(filePath),
  );
  formData.append("metadata", JSON.stringify(metadata));

  const response = await fetch(`${API}/api/v1/books`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${session.access_token}`,
    },
    body: formData,
  });

  if (!response.ok) {
    throw new Error(`Failed to upload fixture book: ${response.status}`);
  }

  return (await response.json()) as Book;
}
