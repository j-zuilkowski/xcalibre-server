import { beforeAll, vi } from "vitest";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { initializeI18n } from "../i18n";

const publicRoot = fileURLToPath(new URL("../../public", import.meta.url));
const originalFetch = globalThis.fetch?.bind(globalThis);

vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
  const requestUrl = typeof input === "string" ? input : input instanceof URL ? input.pathname : input.url;

  if (requestUrl.startsWith("/locales/") && requestUrl.endsWith("/translation.json")) {
    const match = requestUrl.match(/^\/locales\/([^/]+)\/translation\.json$/);
    const language = match?.[1];
    if (!language) {
      return new Response("Not found", { status: 404 });
    }

    try {
      const filePath = join(publicRoot, "locales", language, "translation.json");
      const contents = await readFile(filePath, "utf8");
      return new Response(contents, {
        status: 200,
        headers: {
          "Content-Type": "application/json",
        },
      });
    } catch {
      return new Response("Not found", { status: 404 });
    }
  }

  if (originalFetch) {
    return originalFetch(input, init);
  }

  return new Response("Not found", { status: 404 });
});

beforeAll(async () => {
  await initializeI18n();
});
