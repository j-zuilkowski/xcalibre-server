# Codex Desktop App — xcalibre-server Phase 17: Security Remediation II

## What Phase 17 Builds

This is the final remediation phase. It incorporates every finding from two independent post-Phase 16 audits: the original 13-item list plus 5 additional items uncovered by deep targeted analysis of auth flows, file upload, LDAP, dynamic SQL, API tokens, and rate limiting. Stages 1–4 are High and must ship before tagging v1.4. Stages 5–10 are Medium. Stages 11–15 are Low/Info polish. Stages 16–18 are a new DB migration set plus an additional Medium batch discovered in audit pass 2.

- **Stage 1** — Admin route authorization (apply `require_admin` guard at router level; explicitly covers `list_users`, `list_roles`, and all mutation endpoints)
- **Stage 2** — Proxy auth deny-by-default (`is_trusted_proxy()` returns `false` when `trusted_cidrs` is empty)
- **Stage 3** — Webhook SSRF at creation (`validate_webhook_target()` called in `create_webhook` handler)
- **Stage 4** — TOTP verify/backup rate limiting (apply `auth_rate_limit_layer` to the `totp_pending` router)
- **Stage 5** — `custom_prompt` injection fence (wrap user-supplied prompt in `SOURCE_OPEN`/`SOURCE_CLOSE`)
- **Stage 6** — Collection CRUD transactions (atomic ownership check + operation in single statement)
- **Stage 7** — Backup code timing oracle (move format validation inside DB transaction)
- **Stage 8** — TOTP pending token TTL reset on re-auth
- **Stage 9** — API token TTL + revocation on user delete/disable
- **Stage 10** — API token scope enforcement (read-only vs. read-write; scope column + middleware check)
- **Stage 11** — Index on `collections(owner_id)`
- **Stage 12** — Index on `book_chunks(created_at)`
- **Stage 13** — Webhook payload cap at enqueue time (not just delivery)
- **Stage 14** — `generate_backup_code()` use `OsRng` instead of `thread_rng()`
- **Stage 15** — Eliminate double `file_size()` call in range serving
- **Stage 16** — Startup warning when `base_url` is HTTP and `https_only = false`
- **Stage 17** — OAuth state client IP binding (HMAC state token over nonce + client IP)
- **Stage 18** — Proxy auth: reject empty email from proxy headers

## Key Design Decisions

**Admin route authorization (Stage 1):**
`backend/src/api/admin.rs` currently registers handlers with `AuthenticatedUser` extractors but no role check at the router layer. Any authenticated user can call: `POST /api/admin/users`, `PATCH /api/admin/users/:id/role`, `DELETE /api/admin/users/:id`, `GET /api/admin/users` (user enumeration — emails, TOTP status, roles), `GET /api/admin/roles` (role/permission enumeration), and all tag/library admin routes. The audit confirmed `list_users()` and `list_roles()` take no `AuthenticatedUser` parameter at all — they are accessible to any session holder. The correct fix is a reusable `require_admin` extractor applied at the router level — not per-handler checks. `AuthenticatedUser` remains for identity; `RequireAdmin` is a zero-size guard that FromRequestParts-rejects with 403 if `user.role != "admin"`.

**TOTP verify/backup rate limiting (Stage 4):**
`POST /api/auth/login` is rate-limited at 10 requests/min per IP via `auth_rate_limit_layer`. However, the TOTP completion endpoints (`POST /api/auth/totp/verify` and `POST /api/auth/totp/verify-backup`) are on a separate `totp_pending` router that has no rate-limit layer. TOTP codes are 6-digit (1,000,000 possibilities) and a determined attacker with a stolen pending session token can attempt all values in ~1.7 hours from a single IP. The fix is to apply the same `auth_rate_limit_layer` to the `totp_pending` router routes. Account lockout is per-user and will also trigger, but rate limiting adds a per-IP layer.

**API token TTL and revocation (Stage 9):**
`api_tokens` table has no `expires_at` column — tokens are valid indefinitely once issued. Additionally, there is no cascade delete of tokens when a user is deleted or their `is_active` flag is set to false. A revoked user's API tokens remain valid. Stage 9 adds: (a) an optional `expires_at` column, (b) expiry enforcement in `authenticate_api_token()`, (c) cascade delete in the `delete_user` DB query, and (d) an active-user check in `authenticate_api_token()` before returning the user record.

**API token scope enforcement (Stage 10):**
All API tokens grant the full privilege set of the creating user. A read-only integration (e.g., a home assistant querying book metadata) receives the same privileges as a full admin token if the token owner is admin. Stage 10 adds a `scope` column to `api_tokens` (`read` | `write` | `admin`) enforced in the auth middleware. Token creation accepts an optional scope parameter; existing tokens default to `write`. Admin-scoped tokens are restricted to users with `role = "admin"`.

**OAuth state client IP binding (Stage 17):**
The OAuth state token is a random string stored in a cookie. An attacker who can set the `oauth_state` cookie (e.g., via subdomain cookie injection, XSS on any subdomain, or MITM on HTTP) can fixate the state and force the callback to bind to their session. Binding the state token to client IP using HMAC over `nonce + client_ip` prevents fixation even if the raw nonce is leaked, because the callback validates the HMAC rather than a bare string equality.

**Proxy auth deny-by-default (Stage 2):**
`is_trusted_proxy()` currently returns `true` when `trusted_cidrs` is empty — the intent was "feature not configured = disabled", but the implementation is the opposite: an empty list trusts everything. The fix is one line: `if cidrs.is_empty() { return false; }`. Operators who want proxy auth must explicitly configure at least one CIDR, which is the correct opt-in behavior.

**Webhook SSRF at creation (Stage 3):**
`validate_webhook_target()` is called in the delivery loop (`deliver_webhook()`) but not in `create_webhook()`. An attacker can register `http://169.254.169.254/latest/meta-data/` before `allow_private_endpoints = false` takes effect, or simply when they know the validation happens at delivery time. The URL must be validated at registration time with the same `validate_webhook_target()` function — if it fails, return `422 Unprocessable Entity`. This is the same pattern as LLM endpoint validation at config load time.

**`custom_prompt` injection fence (Stage 4):**
`build_synthesis_prompt()` wraps each retrieved chunk in `SOURCE_OPEN`/`SOURCE_CLOSE` structural delimiters to prevent prompt injection from crafted book content. However, `custom_prompt` (a user-supplied string from the API request body) is inserted into the system prompt verbatim without fencing. A user who can call the synthesis endpoint can use `custom_prompt` to break out of the instruction context. Wrap it: `format!("{SOURCE_OPEN}\n{custom_prompt}\n{SOURCE_CLOSE}")` and add an `INJECTION_NOTICE` comment before it.

**Collection CRUD transactions (Stage 5):**
`ensure_visible_collection()` does a SELECT to verify ownership, then the caller does the mutation as a separate query. Between these two statements, another concurrent request can delete the row — the mutation then silently no-ops or returns a confusing error. The fix is either: (a) a single SQL statement with a `WHERE id = ? AND owner_id = ?` ownership clause that returns 0 rows affected → 404, or (b) wrap both queries in an explicit `BEGIN`/`COMMIT` transaction with `SERIALIZABLE` isolation. Option (a) is simpler and preferred.

**Backup code timing oracle (Stage 6):**
`POST /api/auth/totp/backup` validates `code.len() != 8` before opening the DB transaction. Malformed codes (wrong length) return in ~0.1 ms; valid-format wrong codes return in ~5 ms (DB lookup). This microsecond timing difference is measurable by a determined attacker on a local network. Move the length check inside the transaction, after the SELECT — the DB round-trip dominates, equalizing timing for both paths.

**TOTP pending token TTL (Stage 7):**
When a user starts the TOTP flow, a pending session token is issued with a 5-minute TTL. If the user re-authenticates (e.g., wrong password → re-enter) during this window, a new pending token is issued but the old one is not cleared. Both tokens are valid for the TOTP step until expiry. The fix: at `POST /api/auth/login` success, invalidate any existing pending TOTP tokens for the user before issuing a new one.

## Key Schema Changes

No new tables this phase. Schema changes via ALTER TABLE and new indexes:

| Migration | Contents | Stage |
|---|---|---|
| `0022_collections_idx.sql` | `idx_collections_owner_id` index | Stage 11 |
| `0023_chunks_idx.sql` | `idx_book_chunks_created_at` index | Stage 12 |
| `0024_session_type.sql` | `session_type` discriminator on `sessions` (if not already present) | Stage 8 (conditional) |
| `0025_api_token_expiry.sql` | `expires_at` column on `api_tokens` | Stage 9 |
| `0026_api_token_scope.sql` | `scope` column on `api_tokens` | Stage 10 |

Matching MariaDB migrations must be created for all of the above.

## Reference Files

Read before starting each stage:
- `backend/src/api/admin.rs` — admin route handlers (Stage 1)
- `backend/src/main.rs` or `backend/src/router.rs` — router construction and rate limit application (Stages 1, 4)
- `backend/src/middleware/auth.rs` — `is_trusted_proxy()`, `authenticate_api_token()` (Stages 2, 9, 10)
- `backend/src/webhooks.rs` — `validate_webhook_target()`, `create_webhook` handler (Stage 3)
- `backend/src/llm/synthesize.rs` — `build_synthesis_prompt()`, `SOURCE_OPEN`/`SOURCE_CLOSE` (Stage 5)
- `backend/src/api/collections.rs` — `ensure_visible_collection()`, mutation handlers (Stage 6)
- `backend/src/api/auth.rs` — backup code handler, pending token flow, OAuth callback (Stages 7, 8, 17)
- `backend/src/db/queries/auth.rs` — `delete_user()`, token queries (Stages 8, 9)
- `backend/src/db/queries/api_tokens.rs` — token schema and queries (Stages 9, 10)
- `backend/migrations/sqlite/0020_collections.sql` — collections table schema (Stage 11)
- `backend/migrations/sqlite/0019_chunks.sql` — book_chunks table schema (Stage 12)
- `backend/src/auth/totp.rs` — `generate_backup_code()` (Stage 14)
- `backend/src/api/books.rs` — range serving, `file_size()` call sites (Stage 15)
- `backend/src/config.rs` — startup validation logic (Stage 16)

---

## STAGE 1 — Admin Route Authorization

**Priority: High (fix before v1.4 tag)**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/api/admin.rs and the router construction in
backend/src/main.rs (or backend/src/router.rs — find where the /api/admin
routes are registered).

Add a RequireAdmin guard extractor and apply it at the router level so every
admin endpoint is protected without per-handler checks.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

backend/src/api/admin.rs handlers accept AuthenticatedUser (proves the caller
has a valid session) but never assert user.role == "admin". Any authenticated
user can call:
  POST   /api/admin/users
  PATCH  /api/admin/users/:id/role
  DELETE /api/admin/users/:id
  GET    /api/admin/users
  ... and all tag/library admin routes

The correct fix is a zero-size guard type — not per-handler if-checks that can
be forgotten on new routes.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

backend/src/middleware/auth.rs — add:

  /// Zero-size extractor that rejects non-admin callers with 403.
  /// Usage: add `_admin: RequireAdmin` to handler signature, or apply the
  /// `require_admin` Router layer at registration time.
  pub struct RequireAdmin;

  #[async_trait]
  impl<S> FromRequestParts<S> for RequireAdmin
  where
      S: Send + Sync,
  {
      type Rejection = AppError;

      async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
          let user = AuthenticatedUser::from_request_parts(parts, state).await?;
          if user.role != "admin" {
              return Err(AppError::Forbidden("admin role required".into()));
          }
          Ok(RequireAdmin)
      }
  }

In the router, nest all /api/admin routes behind a layer that injects
RequireAdmin — OR simply add `_admin: RequireAdmin` to every existing admin
handler signature (simpler, harder to forget on future routes because the
pattern is visible).

Write a test in backend/tests/test_admin.rs:
  - Authenticated non-admin user gets 403 on GET /api/admin/users
  - Authenticated admin user gets 200 on GET /api/admin/users
  - Unauthenticated request gets 401

Run `cargo test admin` and `cargo clippy -- -D warnings`.
Commit: "feat: require_admin guard on all admin routes (Phase 17 Stage 1)"
```

---

## STAGE 2 — Proxy Auth Deny-by-Default

**Priority: High (fix before v1.4 tag)**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/middleware/auth.rs, focusing on is_trusted_proxy() and the
proxy auth extraction path.

Fix is_trusted_proxy() to return false (deny) when trusted_cidrs is empty.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Current logic (paraphrased):
  fn is_trusted_proxy(ip: IpAddr, cidrs: &[IpNet]) -> bool {
      cidrs.iter().any(|net| net.contains(&ip))
  }

When cidrs is empty, .any() returns false — that part is correct. BUT the
calling code in the middleware treats an empty list as "proxy auth not
configured = skip the feature entirely", while the actual branch that reaches
is_trusted_proxy() may not guard correctly.

Verify the call site: if the check is gated by `if config.trusted_cidrs.is_empty() { skip }`,
the logic is already correct and the bug is elsewhere. If is_trusted_proxy()
is called with an empty slice and the result is then inverted or misread,
add the explicit guard:

  if cidrs.is_empty() {
      return false;   // no CIDRs configured = feature disabled = deny
  }

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

1. Confirm the exact code path in middleware/auth.rs where X-Remote-User is
   processed. Add the empty-list short-circuit at the top of is_trusted_proxy():

     pub fn is_trusted_proxy(ip: IpAddr, cidrs: &[ipnet::IpNet]) -> bool {
         if cidrs.is_empty() {
             return false;
         }
         cidrs.iter().any(|net| net.contains(&ip))
     }

2. Add a unit test:
     #[test]
     fn empty_cidr_list_denies_all() {
         let ip: IpAddr = "127.0.0.1".parse().unwrap();
         assert!(!is_trusted_proxy(ip, &[]));
     }

3. Add an integration test in backend/tests/test_proxy_auth.rs:
   - With trusted_cidrs = [] (empty), X-Remote-User header is ignored even
     from 127.0.0.1
   - With trusted_cidrs = ["127.0.0.1/32"], X-Remote-User from 127.0.0.1 is
     accepted
   - With trusted_cidrs = ["10.0.0.0/8"], X-Remote-User from 127.0.0.1 is
     rejected

Run `cargo test proxy` and `cargo clippy -- -D warnings`.
Commit: "fix: proxy auth deny-by-default when trusted_cidrs is empty (Phase 17 Stage 2)"
```

---

## STAGE 3 — Webhook SSRF Validation at Creation

**Priority: High (fix before v1.4 tag)**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/webhooks.rs focusing on create_webhook(), validate_webhook_target(),
and deliver_webhook(). Read backend/src/config.rs for the allow_private_endpoints flag.

Call validate_webhook_target() at webhook creation time, not just at delivery.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

validate_webhook_target() checks the URL scheme (must be https or http) and,
when allow_private_endpoints = false, rejects RFC 1918 / loopback addresses.
It is currently called only inside deliver_webhook() — at delivery time.

This means:
  1. An attacker registers http://169.254.169.254/latest/meta-data/ as a webhook URL.
  2. The URL passes creation validation (none exists).
  3. Every book event fires a delivery attempt to the AWS metadata service.
  4. Even if delivery fails, the DNS lookup and TCP connect happen.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In create_webhook() (the POST /api/admin/webhooks handler):

  1. After parsing the request body, call validate_webhook_target(&payload.url, &config)?
  2. If validation fails, return 422 Unprocessable Entity with a JSON error body:
       {"error": "webhook URL is not allowed: <reason>"}
  3. validate_webhook_target() must be pub (or pub(crate)) — verify it is accessible
     from the handler context.

Write a test in backend/tests/test_webhooks.rs:
  - Admin creates webhook with http://127.0.0.1/hook → 422
  - Admin creates webhook with http://169.254.169.254/metadata → 422
  - Admin creates webhook with https://example.com/hook → 201
  - The existing delivery-time SSRF tests must still pass (defence in depth)

Run `cargo test webhook` and `cargo clippy -- -D warnings`.
Commit: "fix: validate webhook URL at creation time to prevent SSRF (Phase 17 Stage 3)"
```

---

## STAGE 4 — TOTP Verify / Backup Rate Limiting

**Priority: High (fix before v1.4 tag)**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/main.rs (or backend/src/router.rs) to find where auth_rate_limit_layer()
is applied, and backend/src/api/auth.rs to confirm which auth endpoints are on the
totp_pending router.

Apply the same rate-limit layer to TOTP verification endpoints.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

POST /api/auth/login is rate-limited at 10 requests/min per IP via
auth_rate_limit_layer() on the public router. The TOTP completion endpoints:
  POST /api/auth/totp/verify
  POST /api/auth/totp/verify-backup

are on a separate totp_pending router that has NO rate-limit layer. A TOTP code
is 6 digits (1,000,000 possibilities). With a stolen pending session token and
no rate limiting, an attacker can brute-force all 10^6 values in ~1.7 hours.

Account lockout (per-user) will also trigger, but rate limiting adds a per-IP
layer and is the first line of defence.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In the router construction, apply auth_rate_limit_layer() to the totp_pending
router the same way it is applied to the public router:

  let totp_pending_router = Router::new()
      .route("/api/auth/totp/verify", post(totp_verify))
      .route("/api/auth/totp/verify-backup", post(totp_verify_backup))
      .layer(auth_rate_limit_layer());   // ← add this line

If auth_rate_limit_layer() is parameterised (requests/duration), use the same
values as the login rate limiter (e.g., 10 req/min per IP) or stricter.

Write a test in backend/tests/test_auth.rs:
  - 11 rapid POST /api/auth/totp/verify requests from the same IP → 11th returns 429
  - Requests from a different IP are not affected by the first IP's rate limit

Run `cargo test auth` and `cargo clippy -- -D warnings`.
Commit: "fix: apply rate limiting to TOTP verify and backup endpoints (Phase 17 Stage 4)"
```

---

## STAGE 5 — `custom_prompt` Injection Fence

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/llm/synthesize.rs, specifically build_synthesis_prompt() and
the SOURCE_OPEN / SOURCE_CLOSE / INJECTION_NOTICE constants.

Wrap the user-supplied custom_prompt in the same structural delimiters used
for chunk content.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Chunk content is wrapped:
  format!("{SOURCE_OPEN}\n[CHUNK {i}]\n{chunk_text}\n{SOURCE_CLOSE}")

This tells the LLM to treat chunk content as data, not instructions.

custom_prompt (from the API request body) is inserted verbatim:
  if let Some(prompt) = &custom_prompt {
      system.push_str(prompt);
  }

A user who can call the synthesis endpoint can inject arbitrary instructions
into the system prompt via custom_prompt.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In build_synthesis_prompt(), change the custom_prompt insertion to:

  if let Some(prompt) = &custom_prompt {
      system.push_str(&format!(
          "\n{INJECTION_NOTICE}\n{SOURCE_OPEN}\n[USER INSTRUCTIONS]\n{}\n{SOURCE_CLOSE}\n",
          prompt
      ));
  }

Also add a comment explaining why: user-supplied prompts are wrapped to limit
injection scope — the structural delimiters signal data context to the model.

Write a test in backend/tests/test_synthesize.rs (or the existing LLM test file):
  - build_synthesis_prompt() with custom_prompt containing SOURCE_OPEN literal
    → the output contains the literal inside the outer SOURCE_OPEN/SOURCE_CLOSE
    envelope (not breaking out of it)
  - custom_prompt = "IGNORE ALL PREVIOUS INSTRUCTIONS" → appears inside delimiters

Run `cargo test synth` and `cargo clippy -- -D warnings`.
Commit: "fix: fence custom_prompt inside SOURCE delimiters to prevent injection (Phase 17 Stage 5)"
```

---

## STAGE 6 — Collection CRUD Atomic Ownership Check

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/api/collections.rs, specifically ensure_visible_collection()
and all mutation handlers that call it (add_book_to_collection, remove_book_from_collection,
delete_collection, update_collection, etc.).

Replace the two-query SELECT-then-mutate pattern with atomic single-statement ownership checks.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Current pattern:
  let col = ensure_visible_collection(pool, collection_id, user.id).await?;
  // ... gap where another request can delete `col` ...
  sqlx::query!("DELETE FROM collection_books WHERE ...").execute(pool).await?;

Between the SELECT and the DELETE, a concurrent request can delete the
collection row. The DELETE then silently no-ops (0 rows affected) or returns
a FK error, neither of which is a clean 404.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

For each mutation handler, replace the two-step pattern with a single query
that includes the ownership predicate:

  DELETE mutation example:
    let result = sqlx::query!(
        "DELETE FROM collection_books
         WHERE collection_id = ? AND book_id = ?
           AND EXISTS (
               SELECT 1 FROM collections
               WHERE id = ? AND (owner_id = ? OR is_public = 1)
           )",
        collection_id, book_id, collection_id, user.id
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("collection or book not found".into()));
    }

  UPDATE/INSERT mutations: similar pattern — include the ownership check in
  the WHERE clause or use a CTE.

  ensure_visible_collection() can remain for GET handlers (reads) where
  TOCTOU is not a concern.

Write a test in backend/tests/test_collections.rs:
  - User A tries to add a book to User B's private collection → 404
  - User A tries to delete a book from their own collection → 200
  - Concurrent delete (simulate with two rapid requests) → one 200, one 404

Run `cargo test collection` and `cargo clippy -- -D warnings`.
Commit: "fix: atomic ownership check in collection mutations (Phase 17 Stage 6)"
```

---

## STAGE 7 — Backup Code Timing Oracle Fix

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/api/auth.rs, specifically the POST /api/auth/totp/backup handler
and any helper that validates backup code format before the DB lookup.

Move the code length/format check inside the DB transaction so both valid-format
and malformed codes take the same DB round-trip time.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Current flow:
  1. Parse code from request body
  2. if code.len() != 8 { return Err(400) }  ← fast path, ~0.1ms
  3. BEGIN TRANSACTION
  4. SELECT backup code FROM db WHERE user_id = ? AND code_hash = ?  ← ~5ms
  5. DELETE used code
  6. COMMIT

A timing oracle: malformed codes (wrong length) return in 0.1ms; valid-format
wrong codes return in 5ms. An attacker can use this to filter candidates and
halve the effective search space.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

Move the format check inside the transaction:

  BEGIN TRANSACTION (or use sqlx::Transaction)
    1. Normalize code input (trim, uppercase — match however codes are stored)
    2. if code.len() != 8 { /* still inside transaction */ return Err(400) }
       — the transaction overhead equalizes timing
    3. SELECT + DELETE backup code
  COMMIT

  Alternatively, skip early return and let the DB lookup fail naturally for
  any input — 0 rows affected → 401. The format check becomes advisory only
  and doesn't short-circuit before the DB call.

Write a test in backend/tests/test_totp.rs:
  - POST /api/auth/totp/backup with 4-char code → 400 (format error still returned)
  - POST /api/auth/totp/backup with 8-char wrong code → 401
  - Timing: both paths exercise the DB (verify with a mock or by inspection)

Run `cargo test totp` and `cargo clippy -- -D warnings`.
Commit: "fix: move backup code format check inside transaction to prevent timing oracle (Phase 17 Stage 7)"
```

---

## STAGE 8 — TOTP Pending Token TTL Reset on Re-auth

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/api/auth.rs, specifically POST /api/auth/login and the
pending TOTP token issuance / session creation logic.

On successful password verification for a TOTP-enabled user, invalidate any
existing pending TOTP tokens before issuing a new one.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

When a TOTP-enabled user logs in:
  1. Password verified → pending session token issued (5-min TTL), stored in sessions table
  2. User POSTs to /api/auth/totp/verify with the pending token + TOTP code → full session

If the user re-authenticates (new POST /api/auth/login) during the 5-min window:
  - A second pending token is issued
  - The first pending token remains valid until it expires naturally
  - Either token can complete the TOTP step

Attacker scenario: an attacker who briefly accesses a logged-in browser tab
can capture the first pending token before the user notices and re-authenticates.
The captured token remains valid for up to 5 minutes.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In the POST /api/auth/login handler, immediately after password verification
succeeds and before issuing the pending token:

  sqlx::query!(
      "DELETE FROM sessions WHERE user_id = ? AND session_type = 'totp_pending'",
      user.id
  )
  .execute(pool)
  .await?;

Then issue the new pending token as before.

Ensure the sessions table has a session_type column (or equivalent discriminator)
that distinguishes pending TOTP tokens from full sessions. If the column does
not exist, add it in a new migration: 0024_session_type.sql.

Write a test in backend/tests/test_auth.rs:
  - User logs in → pending token A issued
  - User logs in again → pending token B issued; token A is rejected by /api/auth/totp/verify

Run `cargo test auth` and `cargo clippy -- -D warnings`.
Commit: "fix: invalidate stale pending TOTP tokens on re-authentication (Phase 17 Stage 8)"
```

---

## STAGE 9 — API Token TTL and User-Delete Revocation

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/db/queries/api_tokens.rs (schema and queries),
backend/src/middleware/auth.rs (authenticate_api_token),
and backend/src/db/queries/auth.rs (delete_user or equivalent).

Add token expiry support and revoke tokens on user deletion/deactivation.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

api_tokens has no expires_at column — tokens are valid indefinitely.
Issues:
  1. A leaked read-write token never expires.
  2. When a user is deleted or is_active is set to false, their API tokens
     remain valid — authenticate_api_token() returns the user without checking
     is_active or token expiry.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

Migration backend/migrations/sqlite/0025_api_token_expiry.sql:

  ALTER TABLE api_tokens ADD COLUMN expires_at INTEGER;  -- unix timestamp, nullable

backend/migrations/mariadb/0025_api_token_expiry.sql:

  ALTER TABLE api_tokens ADD COLUMN expires_at BIGINT DEFAULT NULL;

In authenticate_api_token() in backend/src/middleware/auth.rs:

  1. After fetching the token row, check:
       if let Some(exp) = token.expires_at {
           if exp < now_unix() { return Err(AppError::Unauthorized(...)); }
       }
  2. Check user.is_active: if !user.is_active { return Err(AppError::Unauthorized(...)); }

In the delete_user (or deactivate_user) DB query, add:

  DELETE FROM api_tokens WHERE user_id = ?;

  (Or rely on CASCADE DELETE if the FK is already configured — verify.)

In the create_api_token handler, accept an optional expires_in_days parameter
and set expires_at = now + days * 86400 if provided.

Write tests in backend/tests/test_api_tokens.rs:
  - Expired token returns 401
  - Deleted user's token returns 401
  - Disabled (is_active=false) user's token returns 401
  - Token with no expires_at is accepted indefinitely

Run `cargo test api_token` and `cargo clippy -- -D warnings`.
Commit: "fix: API token expiry enforcement and revocation on user delete (Phase 17 Stage 9)"
```

---

## STAGE 10 — API Token Scope Enforcement

**Priority: Medium**
**Blocks: Stage 9 (schema migration). Blocked by: Stage 9.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/db/queries/api_tokens.rs (after Stage 9 migration),
backend/src/middleware/auth.rs (authenticate_api_token), and
backend/src/api/books.rs, backend/src/api/admin.rs as scope consumers.

Add a scope column to api_tokens and enforce it in the auth middleware.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Every API token grants the full privilege set of the creating user. A home-automation
integration that only needs to query book metadata receives the same write/admin
privileges as the token owner. A leaked token can create, modify, or delete data.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

Migration backend/migrations/sqlite/0026_api_token_scope.sql:

  ALTER TABLE api_tokens ADD COLUMN scope TEXT NOT NULL DEFAULT 'write';
  -- Valid values: 'read' | 'write' | 'admin'

backend/migrations/mariadb/0026_api_token_scope.sql: equivalent.

Add a TokenScope enum to backend/src/auth/api_tokens.rs (or equivalent):
  pub enum TokenScope { Read, Write, Admin }

In authenticate_api_token(), attach scope to the returned auth context.

Add a guard function / extractor:
  pub fn require_write_scope(scope: TokenScope) -> Result<(), AppError> {
      match scope {
          TokenScope::Read => Err(AppError::Forbidden("token scope insufficient".into())),
          _ => Ok(()),
      }
  }

Apply scope enforcement:
  - Read-scope tokens: GET endpoints only (books, search, metadata)
  - Write-scope tokens: GET + POST/PATCH/DELETE on user-owned resources
  - Admin-scope tokens: only valid for tokens owned by admin-role users; gates admin endpoints

  Note: Admin endpoints are gated by require_admin (Stage 1) AND require admin scope.

In create_api_token handler, accept scope parameter; reject admin-scope request if
the creating user is not admin.

Write tests:
  - Read-scope token attempting POST /api/books → 403
  - Write-scope token on GET /api/books → 200
  - Admin-scope token on GET /api/admin/users (as admin user) → 200
  - Admin-scope token created by non-admin user → 422

Run `cargo test api_token` and `cargo clippy -- -D warnings`.
Commit: "feat: API token scope enforcement (read/write/admin) (Phase 17 Stage 10)"
```

---

## STAGE 11 — Index on `collections(owner_id)`

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/migrations/sqlite/0020_collections.sql.

Add a new migration that creates an index on collections(owner_id).
Add the matching MariaDB migration.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

GET /api/collections (list user's collections) queries:
  SELECT * FROM collections WHERE owner_id = ? [AND is_public = 1]

At scale (thousands of collections), this is a full table scan without an index.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

backend/migrations/sqlite/0022_collections_idx.sql:

  CREATE INDEX IF NOT EXISTS idx_collections_owner_id
      ON collections(owner_id);

backend/migrations/mariadb/0022_collections_idx.sql:

  CREATE INDEX idx_collections_owner_id
      ON collections(owner_id);

Run `cargo sqlx migrate run` against the dev DB, verify with:
  PRAGMA index_list('collections');

Run `cargo test` to confirm no regressions.
Commit: "perf: add index on collections(owner_id) (Phase 17 Stage 11)"
```

---

## STAGE 12 — Index on `book_chunks(created_at)`

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/migrations/sqlite/0019_chunks.sql.

Add a new migration that creates an index on book_chunks(created_at).
Add the matching MariaDB migration.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

The scheduled re-chunker queries:
  SELECT * FROM book_chunks WHERE created_at < ? ORDER BY created_at LIMIT 100

Without an index, this is a full table scan over potentially millions of rows.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

backend/migrations/sqlite/0023_chunks_idx.sql:

  CREATE INDEX IF NOT EXISTS idx_book_chunks_created_at
      ON book_chunks(created_at);

backend/migrations/mariadb/0023_chunks_idx.sql:

  CREATE INDEX idx_book_chunks_created_at
      ON book_chunks(created_at);

Run `cargo sqlx migrate run` against the dev DB, verify with:
  PRAGMA index_list('book_chunks');

Run `cargo test` to confirm no regressions.
Commit: "perf: add index on book_chunks(created_at) (Phase 17 Stage 12)"
```

---

## STAGE 13 — Webhook Payload Cap at Enqueue Time

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/webhooks.rs, focusing on enqueue_event() and the
MAX_WEBHOOK_PAYLOAD_BYTES constant.

Add the 1 MB payload size check at enqueue time, before inserting into the DB.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

MAX_WEBHOOK_PAYLOAD_BYTES = 1_000_000 is checked in deliver_webhook() before
sending the HTTP request. But oversized payloads are still:
  - Serialized to JSON
  - Inserted into the webhook_deliveries table
  - Read back from the DB on delivery attempt

This wastes DB I/O and storage. The check should happen at enqueue time.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In enqueue_event(), after serializing the payload to JSON bytes:

  let payload_bytes = serde_json::to_vec(&event_payload)?;
  if payload_bytes.len() > MAX_WEBHOOK_PAYLOAD_BYTES {
      tracing::warn!(
          size = payload_bytes.len(),
          "webhook payload exceeds size limit; skipping enqueue"
      );
      return Ok(());   // silently drop — or return a specific error if preferred
  }
  // proceed with INSERT

Update the existing oversized payload test to confirm the event is NOT inserted
into the DB (query webhook_deliveries table after enqueue, expect 0 rows).

Run `cargo test webhook` and `cargo clippy -- -D warnings`.
Commit: "fix: enforce webhook payload size cap at enqueue time (Phase 17 Stage 13)"
```

---

## STAGE 14 — `generate_backup_code()` Use OsRng

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/auth/totp.rs, specifically generate_backup_code().

Replace thread_rng() with OsRng for consistency with all other sensitive
crypto operations in the codebase.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

thread_rng() is seeded from OsRng and is cryptographically secure in Rust's
rand crate. However, every other sensitive random operation in the codebase
(session token generation, TOTP secret generation) uses OsRng directly.
Consistent use of OsRng avoids subtle seeding issues and is the documented
recommendation for security-sensitive applications.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In generate_backup_code():

  use rand::rngs::OsRng;
  use rand::Rng;

  pub fn generate_backup_code() -> String {
      let mut rng = OsRng;
      // ... rest of generation logic using rng instead of thread_rng()
  }

No behavior change expected. Verify existing backup code generation tests still pass.

Run `cargo test totp` and `cargo clippy -- -D warnings`.
Commit: "fix: use OsRng in generate_backup_code for consistency (Phase 17 Stage 14)"
```

---

## STAGE 15 — Eliminate Double `file_size()` Call in Range Serving

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/api/books.rs, specifically the range file-serving handler
around line 3329 where tokio::fs::metadata() is called, and get_range() which
also calls file_size() internally.

Refactor to a single metadata fetch.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Current pattern (paraphrased):

  let meta = tokio::fs::metadata(&path).await?;     // syscall 1
  let total = meta.len();
  // parse range using total...
  let range = storage.get_range(&path, start..end).await?;
  // get_range() calls file_size() internally → tokio::fs::metadata() again  ← syscall 2

Two metadata syscalls per range request on local storage.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

Option A (preferred): Have get_range() accept the total size as a parameter
  and skip the internal file_size() call when already known.

Option B: Remove the explicit metadata call before get_range() and instead
  add a file_size() method on the storage trait that get_range() uses — call
  it once from the handler and pass through.

Whichever approach: verify the range header parsing still receives the correct
total for Content-Range response headers.

Add a comment noting the single-syscall intent.

Run `cargo test books` and `cargo clippy -- -D warnings`.
Commit: "perf: single metadata syscall per range request (Phase 17 Stage 15)"
```

---

## STAGE 16 — Startup Warning for HTTP base_url with https_only = false

**Priority: Info**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/config.rs, specifically the startup validation / warn block
where other misconfigurations are logged (e.g., the allow_private_endpoints
warning from Phase 16).

Add a startup warning when base_url is an http:// URL and https_only is false.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

A misconfigured deployment with:
  base_url = "http://myserver.com"
  https_only = false

... will serve session cookies without Secure flag, allow CSRF via plain HTTP,
and may silently expose the library to network eavesdropping. There is no
existing warning for this combination.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In the config validation / startup logging section:

  if config.base_url.starts_with("http://") && !config.server.https_only {
      tracing::warn!(
          "SECURITY: base_url is HTTP and https_only is false. \
           Session cookies will not have the Secure flag. \
           Set server.https_only = true or use an HTTPS base_url in production."
      );
  }

Write a test that constructs a Config with http base_url + https_only=false
and verifies the warning is emitted (capture tracing output or test the
boolean condition directly).

Run `cargo test config` and `cargo clippy -- -D warnings`.
Commit: "chore: warn at startup when base_url is HTTP and https_only is false (Phase 17 Stage 16)"
```

---

## STAGE 17 — OAuth State Client IP Binding

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/api/auth.rs, specifically the OAuth initiation handler
(generate_oauth_state / set state cookie) and the OAuth callback handler
(validate state from cookie vs. query param).

Upgrade the state token to an HMAC-signed value that binds to the client's IP address.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Current OAuth state token: a random string stored in a cookie, validated by
bare string equality at callback time.

Attack scenario (state fixation):
  1. Attacker intercepts or injects the oauth_state cookie on the victim's browser
     (subdomain cookie injection, XSS on any sibling subdomain, or HTTP MITM).
  2. Attacker pre-registers the same state with their own OAuth flow.
  3. Victim completes OAuth consent; attacker's callback receives the code and
     the forged state, which passes the bare equality check.

IP binding prevents fixation: even if the raw nonce is known, the HMAC check
at callback time fails because the attacker's IP differs from the victim's IP.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In generate_oauth_state(), produce a signed state token:

  let nonce: String = generate_random_token(32);  // existing or new helper
  let client_ip = extract_client_ip(parts);        // same ConnectInfo extraction as proxy auth
  let state_payload = format!("{}:{}", nonce, client_ip);
  let mac = hmac_sha256(oauth_state_secret, &state_payload);
  let state_token = format!("{}.{}", nonce, hex::encode(mac));

  Store nonce only in the session/cookie (not the full token).

In validate_oauth_state() at callback:
  let (nonce, mac_hex) = state_token.split_once('.').ok_or(...)?;
  let expected_payload = format!("{}:{}", nonce, client_ip_from_request);
  let expected_mac = hmac_sha256(oauth_state_secret, &expected_payload);
  subtle::ConstantTimeEq::ct_eq(&expected_mac, &hex::decode(mac_hex)?)?;

oauth_state_secret: derive from the app's secret_key using HKDF with salt
b"xcalibre-server-oauth-state-v1".

Write tests in backend/tests/test_oauth.rs:
  - Valid state from same IP → callback succeeds
  - Valid nonce but tampered IP → callback returns 400
  - Tampered MAC → callback returns 400

Run `cargo test oauth` and `cargo clippy -- -D warnings`.
Commit: "fix: bind OAuth state token to client IP via HMAC (Phase 17 Stage 17)"
```

---

## STAGE 18 — Proxy Auth: Reject Empty Email from Proxy Headers

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/middleware/auth.rs, specifically the proxy auth user-creation
path where a new user is auto-provisioned from X-Remote-User / X-Remote-Email
headers.

Add validation that rejects proxy auth provisioning when email is empty.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

When proxy auth is enabled and a user authenticates for the first time:
  - Username is extracted from the X-Remote-User header (required)
  - Email is extracted from the X-Remote-Email header (optional)

If no email header is configured or the proxy doesn't forward it, email defaults
to "" (empty string). A user account is created with an empty email field, which:
  - Fails uniqueness constraints if a second proxy-auth user also has no email
  - Breaks any feature that assumes a valid email (e.g., notifications, password reset)
  - Is user-hostile (no way to receive system emails)

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In the proxy auth user-creation path:

  let proxy_email = extract_proxy_email(headers, &config);
  if proxy_email.trim().is_empty() {
      // Option A: reject with a clear log message
      tracing::error!(
          username = %proxy_username,
          "proxy auth user provisioning failed: no email provided by proxy. \
           Configure auth.proxy.email_header in config.toml."
      );
      return Err(AppError::Unauthorized(
          "proxy auth requires a valid email from the proxy".into()
      ));
      // Option B (if preferred): synthesize a placeholder
      // let proxy_email = format!("{}@proxy.local", proxy_username);
  }

Option A (reject) is safer — it forces operators to configure the email header
correctly rather than silently creating broken accounts.

Write tests in backend/tests/test_proxy_auth.rs:
  - Proxy request with X-Remote-User but no X-Remote-Email → 401
  - Proxy request with X-Remote-User and X-Remote-Email set → 200, user provisioned
  - Existing user with empty email in DB (pre-fix data) → login succeeds (don't break existing users)

Run `cargo test proxy` and `cargo clippy -- -D warnings`.
Commit: "fix: reject proxy auth provisioning when email header is missing (Phase 17 Stage 18)"
```

---

## Post-Phase-17 Checklist

After all 18 stages are committed:

- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] `cargo audit` — zero CVEs
- [ ] `cargo sqlx migrate run` against dev DB — migrations 0022–0026 applied (0024 conditional)
- [ ] `PRAGMA index_list('collections')` — `idx_collections_owner_id` present
- [ ] `PRAGMA index_list('book_chunks')` — `idx_book_chunks_created_at` present
- [ ] `PRAGMA table_info('api_tokens')` — `expires_at` and `scope` columns present
- [ ] `PRAGMA table_info('sessions')` — `session_type` (or equivalent) discriminator present
- [ ] Multi-arch Docker build passes CI (amd64 / arm64 / armv7)
- [ ] Update `docs/STATE.md` — Phase 17 complete; update table count and open items
- [ ] Update `docs/CHANGELOG.md` — Phase 17 entry
- [ ] Tag `v1.4.0` locally: `git tag -a v1.4.0 -m "Phase 17: Security Remediation II (final)"`

## Phase Summary

| Stage | Area | Priority |
|-------|------|----------|
| 1 | Admin route authorization (`RequireAdmin` guard — covers `list_users`, `list_roles`, all mutations) | 🔴 High |
| 2 | Proxy auth deny-by-default (empty CIDR = deny) | 🔴 High |
| 3 | Webhook SSRF validation at creation time | 🔴 High |
| 4 | TOTP verify/backup rate limiting (apply `auth_rate_limit_layer` to `totp_pending` router) | 🔴 High |
| 5 | `custom_prompt` injection fence | 🟠 Medium |
| 6 | Collection CRUD atomic ownership check | 🟠 Medium |
| 7 | Backup code timing oracle fix | 🟠 Medium |
| 8 | TOTP pending token TTL reset on re-auth | 🟠 Medium |
| 9 | API token TTL + revocation on user delete/disable | 🟠 Medium |
| 10 | API token scope enforcement (read/write/admin) | 🟠 Medium |
| 11 | Index on `collections(owner_id)` | 🟢 Low |
| 12 | Index on `book_chunks(created_at)` | 🟢 Low |
| 13 | Webhook payload cap at enqueue time | 🟢 Low |
| 14 | `generate_backup_code()` → OsRng | 🟢 Low |
| 15 | Single `file_size()` syscall in range serving | 🟢 Low |
| 16 | Startup warning for HTTP base_url + https_only=false | ℹ️ Info |
| 17 | OAuth state client IP binding via HMAC | 🟠 Medium |
| 18 | Proxy auth: reject empty email from proxy headers | 🟢 Low |
