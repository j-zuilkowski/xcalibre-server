import { http, HttpResponse } from "msw";
import {
  makeAnnotation,
  makeAuthProviders,
  makeAuthSession,
  makeBook,
  makeBookSummary,
  makeCollection,
  makeImportStatus,
  makeJob,
  makeLibrary,
  makeProgress,
  makeShelf,
  makeUser,
  makeAdminUser,
} from "./fixtures";

export const handlers = [
  http.get("/api/v1/auth/providers", () => HttpResponse.json(makeAuthProviders())),
  http.post("/api/v1/auth/login", async ({ request }) => {
    const body = (await request.json()) as { username?: string };
    if (body.username === "totp") {
      return HttpResponse.json({ totp_required: true, totp_token: "totp-token" });
    }
    return HttpResponse.json(makeAuthSession());
  }),
  http.post("/api/v1/auth/register", () => HttpResponse.json(makeAuthSession().user, { status: 201 })),
  http.post("/api/v1/auth/totp/verify", () => HttpResponse.json(makeAuthSession())),
  http.post("/api/v1/auth/totp/verify-backup", () => HttpResponse.json(makeAuthSession())),
  http.post("/api/v1/auth/refresh", () =>
    HttpResponse.json({ access_token: "new-token", refresh_token: "new-refresh" }),
  ),
  http.get("/api/v1/auth/me", () => HttpResponse.json(makeUser())),
  http.patch("/api/v1/auth/me/password", () => HttpResponse.json(null, { status: 204 })),
  http.get("/api/v1/libraries", () => HttpResponse.json([makeLibrary()])),
  http.get("/api/v1/books", ({ request }) => {
    const url = new URL(request.url);
    const page = Number(url.searchParams.get("page") ?? "1");
    return HttpResponse.json({
      items: page === 2 ? [makeBookSummary({ id: "2", title: "Children of Dune" })] : [],
      total: page === 2 ? 1 : 0,
      page,
      page_size: 24,
    });
  }),
  http.get("/api/v1/books/:id", ({ params }) => HttpResponse.json(makeBook({ id: String(params.id) }))),
  http.patch("/api/v1/books/:id", async ({ params, request }) => {
    const patch = (await request.json()) as Record<string, unknown>;
    return HttpResponse.json(makeBook({ id: String(params.id), ...patch }));
  }),
  http.delete("/api/v1/books/:id", () => HttpResponse.json(null, { status: 204 })),
  http.post("/api/v1/books/:id/read", () => HttpResponse.json(null, { status: 204 })),
  http.post("/api/v1/books/:id/archive", () => HttpResponse.json(null, { status: 204 })),
  http.patch("/api/v1/books/:id/progress", () => HttpResponse.json(makeProgress())),
  http.get("/api/v1/books/:id/progress", () => HttpResponse.json(makeProgress())),
  http.get("/api/v1/books/:id/annotations", () => HttpResponse.json([makeAnnotation()])),
  http.post("/api/v1/books/:id/annotations", async ({ request }) => {
    const body = (await request.json()) as Partial<ReturnType<typeof makeAnnotation>>;
    return HttpResponse.json(makeAnnotation(body as never));
  }),
  http.patch("/api/v1/books/:id/annotations/:annotationId", async ({ params, request }) => {
    const body = (await request.json()) as Partial<ReturnType<typeof makeAnnotation>>;
    return HttpResponse.json(makeAnnotation({ id: String(params.annotationId), ...body }));
  }),
  http.delete("/api/v1/books/:id/annotations/:annotationId", () => HttpResponse.json(null, { status: 204 })),
  http.get("/api/v1/shelves", () => HttpResponse.json([makeShelf()])),
  http.post("/api/v1/shelves", async ({ request }) => {
    const body = (await request.json()) as { name?: string; is_public?: boolean };
    return HttpResponse.json(
      makeShelf({
        id: "shelf-2",
        name: body.name ?? "New shelf",
        is_public: body.is_public ?? false,
      }),
      { status: 201 },
    );
  }),
  http.delete("/api/v1/shelves/:id", () => HttpResponse.json(null, { status: 204 })),
  http.get("/api/v1/shelves/:id/books", () =>
    HttpResponse.json({ items: [makeBookSummary()], total: 1, page: 1, page_size: 100 }),
  ),
  http.delete("/api/v1/shelves/:id/books/:bookId", () => HttpResponse.json(null, { status: 204 })),
  http.get("/api/v1/search/status", () =>
    HttpResponse.json({ fts: true, meilisearch: true, semantic: true, backend: "meilisearch" }),
  ),
  http.get("/api/v1/collections", () => HttpResponse.json([makeCollection()])),
  http.get("/api/v1/search", ({ request }) => {
    const url = new URL(request.url);
    const q = url.searchParams.get("q") ?? "";
    if (!q) {
      return HttpResponse.json({ items: [], total: 0, page: 1, page_size: 24 });
    }
    if (q === "error") {
      return HttpResponse.json({ message: "search failed" }, { status: 500 });
    }
    return HttpResponse.json({
      items: [makeBookSummary({ title: "Dune" }), makeBookSummary({ id: "2", title: "Children of Dune" })],
      total: 2,
      page: 1,
      page_size: 24,
    });
  }),
  http.get("/api/v1/search/semantic", () =>
    HttpResponse.json({
      items: [makeBookSummary({ title: "Dune", progress_percentage: 75, document_type: "novel" })],
      total: 1,
      page: 1,
      page_size: 24,
    }),
  ),
  http.get("/api/v1/admin/users", () => HttpResponse.json([makeAdminUser()])),
  http.get("/api/v1/admin/roles", () => HttpResponse.json([makeAdminUser().role])),
  http.post("/api/v1/admin/users", async ({ request }) => {
    const body = (await request.json()) as { username?: string; email?: string; role_id?: string };
    return HttpResponse.json(
      makeAdminUser({
        id: "user-created",
        username: body.username ?? "new-user",
        email: body.email ?? "new@example.com",
        role: body.role_id === "role-admin" ? makeAdminUser().role : makeUser().role,
      }),
      { status: 201 },
    );
  }),
  http.patch("/api/v1/admin/users/:id", async ({ params, request }) => {
    const body = (await request.json()) as { role_id?: string; is_active?: boolean; force_pw_reset?: boolean };
    return HttpResponse.json(
      makeAdminUser({
        id: String(params.id),
        role: body.role_id === "role-admin" ? makeAdminUser().role : makeUser().role,
        is_active: body.is_active ?? true,
        force_pw_reset: body.force_pw_reset ?? false,
      }),
    );
  }),
  http.delete("/api/v1/admin/users/:id", () => HttpResponse.json(null, { status: 204 })),
  http.post("/api/v1/admin/users/:id/reset-password", () => HttpResponse.json(null, { status: 204 })),
  http.post("/api/v1/admin/users/:id/totp/disable", () => HttpResponse.json(null, { status: 204 })),
  http.get("/api/v1/admin/jobs", () =>
    HttpResponse.json({ items: [makeJob()], total: 1, page: 1, page_size: 25 }),
  ),
  http.delete("/api/v1/admin/jobs/:id", () => HttpResponse.json(null, { status: 204 })),
  http.post("/api/v1/admin/import/bulk", () => HttpResponse.json({ job_id: "job-1" }, { status: 201 })),
  http.get("/api/v1/admin/import/:id", () => HttpResponse.json(makeImportStatus())),
  http.get("/api/v1/users/me", () => HttpResponse.json(makeUser())),
  http.get("/api/v1/auth/totp/setup", () =>
    HttpResponse.json({ secret_base32: "JBSWY3DPEHPK3PXP", otpauth_uri: "otpauth://totp/xcalibre" }),
  ),
  http.post("/api/v1/auth/totp/confirm", () => HttpResponse.json({ backup_codes: ["ABC12345"] })),
  http.post("/api/v1/auth/totp/disable", () => HttpResponse.json(null, { status: 204 })),
];
