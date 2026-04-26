import "@testing-library/jest-dom";
import { afterAll, afterEach, beforeAll, vi } from "vitest";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { setupServer } from "msw/node";
import { initializeI18n } from "../i18n";
import { handlers } from "./handlers";

const publicRoot = resolve(process.cwd(), "public");
export const server = setupServer(...handlers);

function getLocaleLanguage(input: RequestInfo | URL): string | null {
  const requestUrl =
    typeof input === "string" ? input : input instanceof URL ? input.pathname : input.url;
  const match = requestUrl.match(/^\/locales\/([^/]+)\/translation\.json$/);
  return match?.[1] ?? null;
}

beforeAll(async () => {
  server.listen({ onUnhandledRequest: "warn" });
  vi.stubGlobal("scrollTo", vi.fn());

  const fetchThroughMsw = globalThis.fetch?.bind(globalThis);
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const language = getLocaleLanguage(input);
    if (language) {
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

    if (fetchThroughMsw) {
      return fetchThroughMsw(input, init);
    }

    return new Response("Not found", { status: 404 });
  });

  await initializeI18n();
});

afterEach(() => {
  server.resetHandlers();
});

afterAll(() => {
  server.close();
  vi.unstubAllGlobals();
});
