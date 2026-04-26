# Changelog — xcalibre-server (Rust Rewrite)

All notable changes to the xcalibre-server Rust rewrite. Format: `[YYYY-MM-DD] — Commit — Summary`

> **Note:** Earlier versions of this file documented the calibre-web Python predecessor project
> (dependency upgrades, flake8/bandit audit results). That content is no longer relevant.
> This file now tracks the Rust rewrite exclusively.

---

## 2026-04-24 — Phase 17 Stage 18: Proxy auth email validation — reject provisioning when email header is missing (6d00318)

- `backend/src/api/auth.rs` — proxy provisioning rejects new users when extracted email is empty; logs actionable error; returns 401 with message
- `backend/src/error.rs` — 401-with-message variant added without breaking existing callers
- `backend/tests/test_proxy_auth.rs` — missing email → 401; valid email → 200 + user created; legacy empty-email user can still log in
- `cargo test proxy` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 17: OAuth state client IP binding via HMAC (801add5)

- `backend/src/api/auth.rs` — OAuth initiation generates nonce + HMAC-signed state token bound to client IP (HKDF salt `b"xcalibre-server-oauth-state-v1"`); cookie stores nonce only; callback validates constant-time MAC against client IP
- `backend/Cargo.toml` — HKDF/HMAC dependency added
- `backend/tests/test_oauth.rs` — same-IP round-trip → success; tampered IP → 400; tampered MAC → 400
- `cargo test oauth` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 16: Startup warning for HTTP base_url with https_only = false (committed)

- `backend/src/config.rs` — `tracing::warn!` emitted at startup when `base_url` starts with `http://` and `server.https_only = false`; warns about missing Secure cookie flag
- Unit test for the misconfiguration predicate
- `cargo test config` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 15: Single metadata syscall per range request (21ecadb)

- `backend/src/storage.rs` — `get_range()` accepts optional total-size hint; `LocalFsStorage` skips internal `metadata()` call when hint is present
- `backend/src/api/books.rs` — range handler fetches metadata once, passes size to `get_range()` and `parse_range()`
- `backend/src/api/authors.rs` + `backend/tests/test_storage_s3.rs` — call sites updated
- `cargo test books` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 14: generate_backup_code() uses OsRng (d2bd0b8)

- `backend/src/auth/totp.rs` — `generate_backup_code()` now uses `OsRng` instead of `thread_rng()` for consistency with all other sensitive crypto operations
- `cargo test totp` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 13: Webhook payload cap at enqueue time (65ac7d3)

- `backend/src/webhooks.rs` — `enqueue_event()` serializes to JSON bytes first; skips DB insert and logs warning when payload exceeds `MAX_WEBHOOK_PAYLOAD_BYTES` (1 MB)
- `backend/tests/test_webhooks.rs` — asserts `webhook_deliveries` stays at 0 rows for oversized events; delivery-time skip behavior retained
- `cargo test webhook` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 12: Index on book_chunks(created_at) (bb8c382)

- `backend/migrations/sqlite/0023_chunks_idx.sql` + `backend/migrations/mariadb/0023_chunks_idx.sql` — `idx_book_chunks_created_at` index; verified with `PRAGMA index_list('book_chunks')`
- `cargo test --workspace` passing

---

## 2026-04-24 — Phase 17 Stage 11: Index on collections(owner_id) (48273a1)

- `backend/migrations/sqlite/0022_collections_idx.sql` + `backend/migrations/mariadb/0022_collections_idx.sql` — `idx_collections_owner_id` index; verified with `PRAGMA index_list('collections')`
- `cargo test --workspace` passing

---

## 2026-04-24 — Phase 17 Stage 10: API token scope enforcement — read/write/admin (abefbfd)

- `backend/src/auth/api_tokens.rs` — `TokenScope` enum; `require_write_scope()` / `require_admin_scope()` helpers; re-exported from `auth/mod.rs`
- `backend/src/middleware/auth.rs` — `AuthKind::Token(scope)` carries scope through middleware; read tokens blocked on non-GET (403); admin-scope tokens require token owner to have `role = admin`; scope check is token-path only — session-authenticated admins pass through `RequireAdmin` without scope requirement
- `backend/src/api/admin.rs` — token creation accepts `scope` parameter; rejects admin-scope from non-admin creators (422); scope returned in response
- `backend/migrations/sqlite/0026_api_token_scope.sql` + `backend/migrations/mariadb/0026_api_token_scope.sql` — `scope TEXT NOT NULL DEFAULT 'write'` on `api_tokens`
- Tests: read-token POST denial, write-token GET access, admin-token admin access, non-admin admin-scope rejection
- `cargo test api_token` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 9: API token expiry and revocation on user delete (acf5ae4)

- `backend/src/db/queries/api_tokens.rs` — `expires_at` column support in insert/select paths
- `backend/src/middleware/auth.rs` + `backend/src/middleware/kobo.rs` — expiry enforced at auth time; inactive-user (is_active = false) rejected; both auth paths covered
- `backend/src/api/admin.rs` — token creation accepts optional `expires_in_days`
- `backend/src/db/queries/auth.rs` — explicit token cleanup on user delete (FK cascade also present)
- `backend/migrations/sqlite/0025_api_token_expiry.sql` + `backend/migrations/mariadb/0025_api_token_expiry.sql`
- Tests: expired token → 401, deleted user → 401, disabled user → 401, no-expiry token → accepted
- `cargo test api_token` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 8: Invalidate stale pending TOTP tokens on re-authentication (b42ffea)

- `backend/src/api/auth.rs` — login handler deletes existing `totp_pending` session rows before issuing new pending token; verify handlers consume the pending row inside the transaction before minting full session
- `backend/src/middleware/auth.rs` — pending token carried through middleware; stale tokens rejected
- `backend/migrations/sqlite/0024_session_type.sql` + `backend/migrations/mariadb/0024_session_type.sql` — `session_type` discriminator on `sessions`
- `backend/tests/test_auth.rs` — regression: second login invalidates first pending token; new token still works
- `cargo test auth` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 fix: JWT jti uniqueness for pending TOTP tokens (220a2c3)

- `backend/src/middleware/auth.rs` — unique `jti` claim added to JWT minting for both access tokens and pending TOTP tokens; prevents two logins within the same second producing identical token strings
- `cargo test --workspace` — zero failures

---

## 2026-04-24 — Phase 17 Stage 7: Backup code timing oracle fix — format check inside transaction (committed)

- `backend/src/api/auth.rs` — backup code handler trims input, opens transaction, runs DB lookup, then decides 400 (malformed) or 401 (wrong code); pre-DB fast-path eliminated
- `backend/tests/test_totp.rs` — 4-char → 400 and 8-char wrong → 401 both verified to hit DB via `login_attempts` counter assertion
- `cargo test totp` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 6: Collection CRUD atomic ownership check (committed)

- `backend/src/api/collections.rs` + `backend/src/db/queries/collections.rs` — `update_collection`, `delete_collection`, `add_books_to_collection`, `remove_book_from_collection` now use atomic ownership predicate in mutation SQL; `ensure_manageable_collection()` two-step helper removed; `update_collection` uses COALESCE for partial updates
- `backend/tests/test_collections.rs` — private-collection add denial and back-to-back delete race regression
- `cargo test collection` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 5: Fence custom_prompt inside SOURCE delimiters (b295315)

- `backend/src/llm/synthesize.rs` — `custom_prompt` wrapped in `[USER INSTRUCTIONS]` block inside `SOURCE_OPEN`/`SOURCE_CLOSE` fence; comment explains injection-scope limit
- `backend/tests/test_synthesize.rs` — adversarial string and delimiter-stuffing both verified to stay inside fence
- `cargo test synth` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 4: Rate limiting on TOTP verify and backup endpoints (a034c90)

- `backend/src/api/auth.rs` — `auth_rate_limit_layer()` applied to `totp_pending` router covering `/api/v1/auth/totp/verify` and `/api/v1/auth/totp/verify-backup`
- `backend/tests/test_auth.rs` — 11th request from same IP → 429; different IP unaffected
- `cargo test auth` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 3 follow-up: RFC 1918 10.x webhook SSRF test (8872553)

- `backend/tests/test_webhooks.rs` — `http://10.0.0.1/hook` → 422 added alongside existing private-target checks

---

## 2026-04-24 — Phase 17 Stage 3: Webhook SSRF validation at creation time (9117eaf)

- `backend/src/api/webhooks.rs` — `validate_webhook_target()` called in `create_webhook()` before DB insert; private/loopback targets return 422 `{"error":"webhook URL is not allowed: ..."}` ; delivery-time validation retained (defence in depth)
- `backend/tests/test_webhooks.rs` — 127.0.0.1 → 422, 169.254.169.254 → 422, example.com → 201
- `cargo test webhook` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 2: Proxy auth deny-by-default when trusted_cidrs is empty (854ff68)

- `backend/src/middleware/auth.rs` — `is_trusted_proxy()` returns `false` immediately on empty CIDR list; `X-Remote-User` header silently ignored when no CIDRs configured
- `backend/src/config.rs` — startup warning reworded to reflect deny-by-default behaviour
- Unit test: `empty_cidr_list_denies_all()`; integration tests: empty-list denies, 127.0.0.1/32 allows, 10.0.0.0/8 rejects 127.0.0.1
- `cargo test proxy` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 17 Stage 1 follow-up: list_roles under RequireAdmin guard; authors admin tests (e183147)

- `backend/src/api/admin.rs` — `list_roles()` now takes `_admin: RequireAdmin`; handler cannot execute without guard extraction
- `backend/src/middleware/auth.rs` — `RequireAdmin` doc comment states 401/403 semantics and `require_auth` ordering dependency
- `backend/tests/test_admin.rs` — authors admin subtree covered with 401/403/200 ladder

---

## 2026-04-24 — Phase 17 Stage 1: require_admin guard on all admin routes (eaaf48c)

- `backend/src/middleware/auth.rs` — `RequireAdmin` zero-size extractor: extracts `AuthenticatedUser` from extensions (401 if missing), checks `role == "admin"` (403 if not); cannot be used standalone without `require_auth` middleware having run
- `backend/src/api/admin.rs` — full admin router wrapped in `RequireAdmin` layer; covers all handlers including `list_users` and `list_roles`
- `backend/src/api/authors.rs` — `RequireAdmin` applied to `/api/v1/admin/authors` subtree
- `backend/tests/test_admin.rs` — unauthenticated → 401, non-admin → 403, admin → 200 for both users and authors subtrees
- `cargo test admin` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 14: S3 path traversal unit tests — sanitize_relative_path coverage (b023f47)

- `backend/tests/test_storage_s3.rs` — `test_sanitize_rejects_double_dot`, `test_sanitize_rejects_absolute_path`, `test_sanitize_rejects_windows_absolute`, `test_sanitize_strips_cur_dir`, `test_sanitize_allows_normal_nested_path`, `test_sanitize_rejects_empty_path`, `test_s3_key_with_prefix_prepends_correctly`, `test_s3_key_traversal_does_not_escape_prefix`, `test_sanitize_url_encoded_dots_are_treated_as_literal` (documents `%2e%2e` as a safe literal Normal component)
- `backend/src/storage.rs` + `backend/src/api/books.rs` — precheck added to reject drive-letter absolute paths (Windows `C:\` style) consistently across both sanitizers; required for the Windows-drive test case to pass on macOS
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 13: Annotation cross-user rejection tests — PATCH and DELETE by non-owner (570f3ec)

- `backend/tests/test_annotations.rs` — local `create_user_with_token` helper for two-user test setup; `test_list_annotations_excludes_other_users` (User A sees only their own rows when User B has annotated the same book); `test_patch_annotation_by_non_owner_returns_403`; `test_delete_annotation_by_non_owner_returns_403` (both assert 403 or 404)
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 12: Expand SSRF test coverage — full RFC 1918, IPv6, localhost (bb8053e)

- `backend/tests/test_webhooks.rs` — `test_create_webhook_rejects_all_private_destinations()` replaces single-case test; covers IPv4 loopback, full RFC 1918 (10.x, 172.16–31.x, 192.168.x), IPv6 loopback (`::1`), `localhost`, link-local (`169.254.x.x`), AWS metadata service (`169.254.169.254`), unspecified address (`0.0.0.0`); each asserts 400 or 422
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 11: Enforce SameSite=Strict on refresh token cookie (e4b05e3)

- `backend/src/api/auth.rs` — shared cookie helpers emit `SameSite=Strict`, `Path=/api/v1/auth`, `HttpOnly`, and `Secure` (when `server.https_only` or HTTPS base URL); applied at all four write sites: login, OAuth callback, refresh rotation, logout
- `backend/src/config.rs` — `server.https_only: bool` added with env override; cookie helper reads this rather than inferring from base URL alone
- Tests: `test_login_sets_samesite_strict_cookie`, `test_refresh_rotates_cookie_with_samesite_strict`, HTTPS-secure cookie path assertion
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 10: list_books N+1 fix — single GROUP_CONCAT JOIN (34b5b90)

- `backend/src/db/queries/books.rs` — `list_books()` and `list_book_summaries_by_ids()` rewritten as single-query paths; authors and tags aggregated in SQL via GROUP_CONCAT; `BookSummary` now carries `tags: Vec<TagRef>` populated from the JOIN (no per-book secondary queries)
- Regression test in `backend/tests/test_books.rs` — seeds 10 books × 3 authors × 5 tags; calls `list_books()` directly; counts SQL statements via a tiny sqlx executor wrapper; asserts statement count stays under threshold
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 9: LLM endpoint SSRF validation — reject private IPs unless allow_private_endpoints = true (285e158)

- `backend/src/config.rs` — `allow_private_endpoints: bool` added to `LlmSection`; startup validation rejects `http`/`https` LLM endpoints pointing at localhost, RFC 1918, link-local, or documentation ranges unless flag is set; validation is DNS-independent (IP-based only)
- `xs-mcp/src/main.rs` — shared validator call site updated for synchronous check
- `config.example.toml` — `# allow_private_endpoints = false` comment added with operator guidance for local model servers (LM Studio, Ollama)
- Tests: `test_llm_endpoint_rejects_localhost_by_default`, `test_llm_endpoint_allows_localhost_when_flag_set`, `test_llm_endpoint_allows_public_https`
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 8: Clamp chunk search limit to 100 on all search endpoints (838dca2)

- `backend/src/api/search.rs` — `MAX_CHUNK_SEARCH_RESULTS = 100`; `search_chunks` clamps `limit` before passing to pipeline
- `backend/src/api/collections.rs` — same 100 cap applied to `search_collection_chunks`
- Tests: `test_chunk_search_clamps_limit_to_100`, `test_collection_chunk_search_clamps_limit_to_100` — both seed 120 matching chunks so the `<= 100` assertion is meaningful
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 7: Webhook payload size cap — reject > 1 MB payloads (cb0e923)

- `backend/src/webhooks.rs` — `MAX_WEBHOOK_PAYLOAD_BYTES = 1_000_000`; oversized payloads return `DeliveryAttemptResult { delivered: false, should_retry: false, error: Some("payload_too_large: ...") }` before any HTTP POST; retry scheduler respects `should_retry = false` (goes straight to failed)
- Tests: `test_webhook_delivery_skips_oversized_payload`
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 6: Synthesis prompt injection fence — delimit source material (98d0339)

- `backend/src/llm/synthesize.rs` — `SOURCE_OPEN`, `SOURCE_CLOSE`, `INJECTION_NOTICE` constants; `build_synthesis_prompt()` restructured so synthesis instruction + query appear before the fence, all chunk text appears between delimiters labeled `[Source N: title > heading]`
- Tests: delimiter presence and ordering assertions; injection-text fencing (crafted chunk text lands inside fence, not before instruction); existing grounding assertions updated for `[Source N]` label format
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 5: TOTP verify — generate tokens before clearing lockout (0e204d7)

- `backend/src/api/auth.rs` L939–958 — session creation (`create_login_session_response`) now precedes `clear_login_lockout`; token generation failure returns `Internal` without touching lockout state; lockout clear failure is non-fatal (logged, request succeeds)
- Tests: `test_totp_verify_lockout_not_cleared_on_token_failure` (invariant note); `test_totp_verify_success_returns_tokens_and_clears_lockout` (seeds lockout state, verifies `login_attempts = 0` and `locked_until = NULL` after success)
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 4: HKDF domain-specific salts for TOTP and webhook key derivation (758be28)

- `backend/src/auth/totp.rs` — `TOTP_HKDF_SALT = b"xcalibre-server-totp-v1"` and `WEBHOOK_HKDF_SALT = b"xcalibre-server-webhook-v1"` constants; `derive_key(jwt_secret, salt)` accepts explicit salt; TOTP and webhook crypto helpers use separate derivation paths
- `backend/src/webhooks.rs` — webhook secret encryption/decryption switched to webhook-specific `derive_key(..., WEBHOOK_HKDF_SALT)` path
- Tests: `test_totp_key_derivation_is_stable`, `test_totp_and_webhook_keys_are_distinct`; webhook tests updated for new crypto path
- `docs/DEPLOY.md` — Key Rotation section added for existing deployments upgrading from None-salt derivation
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 3: Proxy auth IP whitelist — gate X-Remote-User on trusted_cidrs (89b2149)

- `backend/src/config.rs` — `ProxyAuthConfig.trusted_cidrs: Vec<String>` with loopback defaults (`["127.0.0.1/32", "::1/128"]`); startup warning logged when `proxy.enabled = true` and `trusted_cidrs` is empty
- `backend/src/middleware/auth.rs` — proxy auth handler gated on `ConnectInfo<SocketAddr>` CIDR membership via `ipnet`; untrusted sources have `X-Remote-User` header ignored, falling through to JWT auth
- `backend/src/lib.rs` — production server wrapped with `into_make_service_with_connect_info::<SocketAddr>()` to populate `ConnectInfo` extension
- `config.example.toml` — commented `trusted_cidrs` example added to `[auth.proxy]` section
- `backend/Cargo.toml` — `ipnet = "2"` added
- Tests: trusted IP accepted, untrusted IP rejected, proxy disabled ignores header, CIDR matching unit tests
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 2: Range header validation against file size — 416 on out-of-bounds (8eef8de)

- `backend/src/api/books.rs` — `parse_range()` now rejects unsatisfiable ranges up front (start >= total → None); `serve_storage_file()` fetches file size before parsing Range header; returns 416 with `Content-Range: bytes */{size}` for invalid ranges
- `backend/src/storage.rs` — `file_size()` added to `StorageBackend` trait
- `backend/src/storage_s3.rs` — `file_size()` implemented via S3 `HeadObject`
- Tests: `test_range_request_beyond_file_size_returns_416`, `test_range_request_u64_max_returns_416`, `test_range_request_start_equals_file_size_returns_416`, `test_range_request_valid_still_returns_206`
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 16 Stage 1: S3 path traversal fix — Path::components() sanitization (a1824df)

- `backend/src/storage.rs` — `sanitize_relative_path()` shared function using `Path::components()`; rejects `ParentDir`, `RootDir`, `Prefix`; strips `CurDir`; rejects empty paths
- `LocalFsStorage::resolve()` updated to call `sanitize_relative_path()` for consistency
- `S3Storage::s3_key()` changed to return `anyhow::Result<String>`; all call sites propagate with `?`
- Tests: parent dir rejection, absolute path rejection, `./` normalization, normal nested paths, literal `%2e%2e` documented as safe (Rust's `Path::new` treats it as a Normal component, not a traversal)
- Note: percent-encoded traversal (`%2e%2e`) passes as a literal segment — not decoded by `Path::components()`. This is safe for S3 keys (the literal string `%2e%2e` is a valid S3 key component, not an escape). If URL-decoded input is ever passed to storage paths, add `percent_decode` before sanitization.
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-24 — Phase 15 Stage 3: Collections, synthesize MCP tool (14 formats), collection search UI (0fe8762)

- Migration 0020 (SQLite) / 0019 (MariaDB): `collections` + `collection_books` tables
- Migration 0021 (SQLite): `book_chunks_fts` FTS5 virtual table + triggers (renumbered from 0020 to avoid collision)
- `backend/src/db/queries/collections.rs` + `backend/src/api/collections.rs` — full collections CRUD: `GET/POST /collections`, `GET/PATCH/DELETE /collections/:id`, `POST/DELETE /collections/:id/books`; collection-scoped chunk search with per-book provenance
- `backend/src/api/search.rs` + `backend/src/search/{mod,fts5,meili}.rs` — `collection_id` scoping wired into hybrid retrieval
- `backend/src/llm/synthesize.rs` — synthesis helper: retrieves top chunks, builds format-specific prompt, streams to LLM
- `xs-mcp/src/tools/mod.rs` — `synthesize` MCP tool with 14 formats: runsheet, design-spec, spice-netlist, kicad-schematic, netlist-json, svg-schematic, bom, recipe, compliance-summary, comparison, study-guide, cross-reference, research-synthesis, custom; full 8-tool MCP set registered
- `apps/web/src/features/admin/CollectionsPage.tsx` — create, list, manage books in collection
- `apps/web/src/features/search/SearchPage.tsx` — collection filter in search UI
- `AdminLayout.tsx` + `router.tsx` — collections nav + route wired
- Tests: `test_collections.rs`, `test_synthesize.rs`, `test_hybrid_search.rs` (collection scoping), `test_mcp_tools.rs`
- `cargo test --workspace` + `cargo clippy -- -D warnings` + web build all passing

---

## 2026-04-24 — Phase 15 Stage 2: Hybrid BM25+semantic chunk retrieval, RRF fusion, cross-encoder reranking (5bebbec)

- Migration 0020 (SQLite): `book_chunks_fts` FTS5 virtual table + insert/update/delete triggers keeping it in sync with `book_chunks`
- `backend/src/api/search.rs` — `GET /api/v1/search/chunks`: BM25 (FTS5) + cosine similarity (sqlite-vec) combined via RRF (k=60); optional cross-encoder reranking (parallel LLM calls, 10s total timeout, silent fallback to RRF order); filters: `book_id`, `collection_id`, `chunk_type`; provenance-rich response (book title, heading path, chunk type per result)
- `backend/src/db/queries/book_chunks.rs` — BM25 query, semantic query, and RRF merge helpers
- `backend/src/llm/chat.rs` — deterministic rerank mock behavior for test isolation
- `xs-mcp/src/tools/mod.rs` — `search_chunks` MCP tool; `semantic_search` deprecated as proxy alias to new endpoint
- Tests: BM25-only, semantic-only, RRF fusion, reranker fallback, empty results, `book_id` filter, `collection_id` filter, MCP tool integration
- Note: `llm_features.rs` wiremock tests fail in this sandbox (mock HTTP port bind blocked) — environment constraint, not a regression; new hybrid-search + MCP tests pass

---

## 2026-04-24 — Phase 15 Stage 1: Sub-chapter chunking + vision LLM pass (a450770)

- Migration 0019 (SQLite) / 0018 (MariaDB): `book_chunks` table (`id, book_id, chunk_index, chapter_index, heading_path, chunk_type, text, word_count, has_image, embedding, created_at`)
- `backend/src/ingest/chunker.rs` — `ChunkConfig`, `ChunkDomain` enum, domain-aware boundary detection: heading hierarchy, procedure detection (numbered lists never split), recipe boundaries, overlap, image-heavy detection
- `backend/src/llm/vision.rs` + `backend/src/ingest/vision.rs` — `describe_image_page()`: domain-specific prompts, appended to OCR text, 10s timeout + silent fallback
- `backend/src/ingest/text.rs` — chunking integrated into ingest pipeline; backfill path for existing books
- `backend/src/lib.rs` — startup re-chunking job hooked in
- `backend/src/db/queries/book_chunks.rs` — chunk persistence (upsert, delete-by-book, list-by-book)
- `backend/src/api/books.rs` — `GET /api/v1/books/:id/chunks` endpoint
- `backend/src/api/docs.rs` — OpenAPI registration
- Tests: `test_chunker.rs` (boundary detection, procedure/recipe handling, overlap), `test_chunks_api.rs` (endpoint + persistence)
- `cargo test -p backend --test test_chunker` + `--test test_chunks_api` + `cargo clippy -p backend -- -D warnings` passing

---

## 2026-04-24 — Phase 14 Stage 6: Author photo upload + serving (8802bc6)

- `backend/src/api/authors.rs` — `POST /authors/:id/photo` (multipart upload, square crop, 400×400 + 100×100 JPEG + WebP output); `GET /authors/:id/photo` (serves WebP when `Accept: image/webp`, falls back to JPEG; SVG letter-placeholder when no photo set)
- `backend/src/db/queries/authors.rs` — `photo_path` stored on `author_profiles`; update on upload
- `apps/web/src/features/library/AuthorPage.tsx` — photo display with upload overlay (admin only); uses authenticated photo endpoint
- `packages/shared/src/client.ts` — `uploadAuthorPhoto` helper
- Tests: `test_author_photos.rs` — upload, WebP serve, JPEG fallback, placeholder SVG, non-admin 403
- `cargo test --workspace` + `cargo clippy -- -D warnings` + web build all passing

---

## 2026-04-24 — Phase 14 Stage 5: WCAG 2.1 AA accessibility remediation (834b81c)

- **Keyboard navigation**: library grid cards focusable + arrow-key navigable; EpubReader toolbar keyboard-accessible, hidden controls removed from tab order, Esc exits reader, `?` opens shortcuts panel; PdfReader same tab-order fix
- **Focus trapping**: primitives added to `Sheet.tsx` + `Dialog.tsx`; wired into author merge drawer, user/tag/scheduled-task/webhook confirmation dialogs, tag restrictions modal
- **Contrast**: light + dark placeholder colors fixed across shared form inputs and admin forms; sepia theme text/background contrast corrected in EPUB reader
- **Screen reader announcements**: polite `aria-live` regions for library loading, search results, admin import progress; `Toast.tsx` — `role="status"` + `aria-live="polite"`
- **Semantics**: login + book metadata label associations + `aria-describedby` fixed; library grid converted to `ul/li`; admin table headers get `scope="col"`; `main` + admin nav landmarks labeled
- **CI**: `accessibility.spec.ts` using `@axe-core/playwright`; `@axe-core/playwright` added to `package.json`; login page check passes; backend-dependent checks skip gracefully when API not running
- `pnpm --filter @xs/web build` passing

---

## 2026-04-23 — Phase 14 Stage 4: Mobile download queue UI (8f85b4e)

- `apps/mobile/src/lib/downloads.ts` — download queue store: active/downloaded/failed sections, low-storage guard (200MB warning), storage summary, cancel/delete helpers
- `apps/mobile/src/app/downloads.tsx` — Downloads screen with active, downloaded, and failed sections; swipe-to-delete on completed items
- `apps/mobile/src/app/(tabs)/profile.tsx` — Downloads entry point with size/count subtitle
- `apps/mobile/src/app/book/[id].tsx` — book download wired with file size + cover metadata; user-cancelled storage prompts ignored gracefully
- `apps/mobile/src/app/shelf/[id].tsx` — shelf detail route with "Download all" batch queuing, format resolution
- `apps/mobile/src/tests/Downloads.test.ts` + `test/setup.ts` — download flow tests with file-system mocks
- No backend changes — Expo only
- `tsc --noEmit` + `vitest run` passing

---

## 2026-04-23 — Phase 14 Stage 3: Webhook delivery — CRUD, HMAC signing, retry, SSRF guard (19401d1)

- Migration 0018 (SQLite) / 0017 (MariaDB): `webhooks` table (url, secret encrypted AES-256-GCM, events, active) + `webhook_deliveries` table (status, attempt count, next retry, response code)
- `backend/src/webhooks.rs` — delivery engine: HMAC-SHA256 `X-Xcalibre-server-Signature: sha256=...` on every payload; AES-256-GCM encrypted secret at rest (same key derivation as TOTP); SSRF guard at creation and delivery (rejects RFC 1918 + loopback); exponential backoff retry: 30s → 5m → 30m (3 attempts)
- `backend/src/api/webhooks.rs` — `GET/POST/DELETE /webhooks`, `POST /webhooks/:id/test`
- Event enqueue points wired at: ingest, book delete, import completion, LLM job completion, user registration
- `backend/src/scheduler.rs` — delivery polling every 30s
- `apps/web/src/features/profile/WebhooksPage.tsx` — create (url + event selector), list with delivery status, delete, test-fire
- `packages/shared/src/client.ts` + `types.ts` — webhook API bindings
- Tests: CRUD, HMAC signature verification, SSRF guard rejection, retry scheduling
- `cargo clippy -- -D warnings` + web build passing; `cargo test --workspace` passed on implementation pass

---

## 2026-04-23 — Phase 14 Stage 2: Author management — profiles, detail page, admin merge (d5e9aa9)

- Migration 0017 (SQLite) / 0016 (MariaDB): `author_profiles` table (`id, author_id, bio, website, born_date, died_date, photo_path, created_at, updated_at`)
- `backend/src/db/queries/authors.rs` — `get_author`, `list_authors`, `update_author_profile`, `merge_authors` (atomic transaction: reassign `book_authors` → delete source, duplicate suppression)
- `backend/src/api/authors.rs` — `GET /authors`, `GET /authors/:id`, `PATCH /authors/:id` (omitted vs explicit null distinguished for field clearing); `POST /admin/authors/:id/merge`
- `backend/src/api/mod.rs` + `docs.rs` — routes registered + OpenAPI annotated
- Tests: author CRUD, merge atomicity, duplicate suppression, 403 on non-admin merge
- `apps/web/src/features/library/AuthorPage.tsx` — bio, website, born/died, book grid, merge button (admin only)
- `apps/web/src/features/admin/AuthorsPage.tsx` — searchable list with merge combobox
- `BookCard.tsx`, `BookListRow.tsx`, `BookDetailPage.tsx` — author links wired to `/authors/:id`
- `AdminLayout.tsx` + `router.tsx` — authors nav entry + routes
- `packages/shared/src/client.ts` + `types.ts` — author API bindings
- Vite chunk-size warnings present (non-blocking)
- `cargo test`, `cargo clippy -- -D warnings`, web build all passing

---

## 2026-04-23 — Phase 14 Stage 1: Goodreads and StoryGraph CSV import (de63010)

- Migration 0016 (SQLite) / 0015 (MariaDB): `import_logs` table for background import job tracking
- `backend/src/ingest/goodreads.rs` — `parse_goodreads_csv` + `parse_storygraph_csv`; updates shelves, ratings, and `book_user_state` (read/unread) on match
- `backend/src/db/queries/import_logs.rs` — import log CRUD; `backend/src/db/queries/books.rs`, `shelves.rs`, `book_user_state.rs` extended for import upserts
- `backend/src/api/users.rs` — `POST /users/me/import` (multipart CSV upload, runs background job); `GET /users/me/import/:id` (status polling)
- `apps/web/src/features/profile/ImportPage.tsx` — file drop zone, dry-run toggle, progress polling, results summary
- `apps/web/src/router.tsx` + `ProfileSidebar.tsx` — `/profile/import` route + sidebar nav entry
- `packages/shared/src/client.ts` + `types.ts` — import API bindings
- `vendor/csv/` — `csv` crate vendored for offline build environment
- `cargo test --offline --workspace` + `cargo clippy --offline -- -D warnings` + web build passing

---

## 2026-04-23 — Phase 13 Stage 3: Prometheus metrics endpoint + custom gauges

- `backend/src/lib.rs` — `PrometheusMetricLayer::pair()` at root router; `GET /metrics` unauthenticated; HTTP request counters + latency histograms out of the box via `axum-prometheus`
- `backend/src/api/mod.rs` — duplicate `/metrics` route and Prometheus layer removed
- `backend/src/metrics.rs` — custom gauge name constants: `xcalibre-server_llm_jobs_queued`, `xcalibre-server_llm_jobs_running`, `xcalibre-server_llm_jobs_failed_total`, `xcalibre-server_import_jobs_active`, `xcalibre-server_search_unindexed_books`, `xcalibre-server_db_pool_connections`
- `backend/src/db/queries/llm.rs` — `cancel_job()` now correctly decrements `xcalibre-server_llm_jobs_queued` and increments `xcalibre-server_llm_jobs_failed_total` (stale metrics path fixed)
- Pre-existing deliverables confirmed in place: `docs/DEPLOY.md` `/metrics` reverse-proxy block note, `docker/prometheus.yml`, `docker/docker-compose.yml` commented Prometheus/Grafana services, `docker/grafana/xcalibre-server-dashboard.json`
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-23 — Phase 13 Stage 4: Reading statistics — streak, monthly books, top authors/tags (8604162)

- `backend/src/db/queries/stats.rs` — `get_user_stats`: aggregate queries on existing `reading_progress` + `book_user_state` + `formats` + `book_tags` tables; streak computed in Rust from sorted date list; monthly books (last 12 months); top 5 tags and authors; formats breakdown
- `backend/src/api/users.rs` — `GET /users/me/stats` returning `UserStats { total_books_read, books_read_this_year, books_read_this_month, books_in_progress, reading_streak_days, longest_streak_days, formats_read, top_tags, top_authors, monthly_books }`
- `backend/src/api/docs.rs` — `/users/me/stats` registered in OpenAPI spec
- `apps/web/src/features/profile/StatsPage.tsx` — 4 stat cards, SVG bar chart (last 12 months, no external lib), top authors + tags panels, formats breakdown pill bar; teal accent color
- `apps/web/src/features/profile/ProfileSidebar.tsx` + `ProfilePage.tsx` — "Reading Stats" nav entry; `/profile/stats` route wired
- `apps/mobile/src/app/(tabs)/profile.tsx` — stats summary card (books read, streak, in progress); taps through to stats screen
- `apps/mobile/src/app/stats.tsx` — full mobile stats screen (top 3 authors/tags, formats as text)
- `packages/shared/src/client.ts` + `types.ts` — `UserStats` type + client binding
- No new DB tables — all queries on existing schema
- `cargo test --workspace` + `cargo clippy -- -D warnings` + web + mobile TS builds all passing

---

## 2026-04-23 — Phase 13 Stage 2: OpenAPI spec + Swagger UI (79c3eff)

- `backend/Cargo.toml` — `utoipa` (axum_extras, chrono, uuid features) + `utoipa-swagger-ui` (axum feature)
- `backend/src/api/docs.rs` — `ApiDoc` struct with `#[derive(OpenApi)]`; `SecurityAddon` wires bearer JWT scheme; `openapi_routes()` returns merged `SwaggerUi` router
- `backend/src/api/mod.rs` — `/api/docs` and `/api/docs/openapi.json` mounted without auth middleware
- `backend/src/error.rs` — `AppErrorResponse` schema for error response documentation
- `#[utoipa::path]` annotations on priority handler groups: auth (login, refresh, logout, TOTP), books (CRUD, cover, download, progress, annotations), search, shelves, users, health
- `#[derive(utoipa::ToSchema)]` on priority types: Book, Author, Tag, User, Annotation, PaginatedResponse, AppError
- Tests: `test_openapi_json_endpoint_returns_200`, `test_openapi_json_is_valid_json`, `test_openapi_json_contains_books_path`, `test_openapi_json_requires_no_auth`, `test_swagger_ui_returns_200`
- `cargo test --workspace` + `cargo clippy -- -D warnings` + web + mobile TS builds all passing

---

## 2026-04-23 — Phase 13 Stage 1: Reader annotations — highlights, notes, bookmarks (586c69b)

- Migration 0015 (`sqlite`) / 0014 (`mariadb`): `book_annotations` table — `id, user_id, book_id, type, cfi_range, highlighted_text, note, color, created_at, updated_at`; index on `(user_id, book_id)`
- `backend/src/db/queries/annotations.rs` — `list_annotations`, `create_annotation`, `update_annotation`, `delete_annotation`; ownership enforced at query layer
- `backend/src/api/books.rs` — `GET/POST /books/:id/annotations`, `PATCH/DELETE /books/:id/annotations/:ann_id`; 403 on cross-user patch/delete
- `apps/web/src/features/reader/EpubReader.tsx` — annotations loaded on mount; selection menu (color picker, note input, bookmark); highlight click tooltip (edit note, change color, delete); `rendition.themes` CSS injection for 4 colors; Chapters | Annotations tab in TOC panel
- `apps/mobile/src/features/reader/EpubReaderScreen.tsx` — read-only annotation display; TODO Phase 14 for creation/editing
- `packages/shared/src/client.ts` + `types.ts` — annotation API bindings
- `docs/SCHEMA.md` — `book_annotations` table documented
- Tests: `test_create_highlight_returns_201`, `test_create_note_requires_note_text`, `test_create_bookmark_accepts_null_highlighted_text`, `test_list_annotations_only_returns_own`, `test_update_annotation_changes_color`, `test_update_annotation_owned_by_other_user_returns_403`, `test_delete_annotation_returns_204`, `test_delete_annotation_owned_by_other_user_returns_403`, `test_annotations_cascade_delete_on_book_delete`
- `cargo test --workspace` + `cargo clippy -- -D warnings` + web + mobile TS builds all passing

---

## 2026-04-23 — Phase 12 Stage 8: Global tag management — admin tag rename, merge, delete (3edfa93)

- `backend/src/api/admin.rs` — `GET /admin/tags` (paginated, with counts, search); `PATCH /admin/tags/:id` (rename, 409 on name conflict); `DELETE /admin/tags/:id`; `POST /admin/tags/:id/merge` (atomic transaction)
- `backend/src/db/queries/tags.rs` — `list_tags_with_counts`, `rename_tag`, `delete_tag`, `merge_tags` (single DB transaction: reassign book_tags → delete source tag)
- `apps/web/src/features/admin/TagsPage.tsx` — table with inline rename, merge combobox, delete confirm dialog, search, pagination
- `apps/web/src/router.tsx` + `AdminLayout.tsx` — `/admin/tags` route + sidebar nav entry
- `packages/shared/src/client.ts` + `types.ts` — admin tag API bindings
- `docs/API.md` — tags section updated with admin routes
- Tests: `test_list_tags_returns_book_counts`, `test_rename_tag_updates_name`, `test_rename_tag_conflicts_with_existing_name_returns_409`, `test_delete_tag_removes_from_all_books`, `test_delete_nonexistent_tag_returns_404`, `test_merge_tag_moves_books_to_target`, `test_merge_tag_does_not_duplicate_on_books_that_already_have_target`, `test_merge_is_atomic_source_deleted_after_merge`
- `cargo test --workspace` + `cargo clippy -- -D warnings` + `pnpm --filter @xs/web build` all passing

---

## 2026-04-23 — Phase 12 Stage 7: WebP cover conversion with JPEG fallback (090ca10)

- `backend/Cargo.toml` — `image` crate `webp` feature enabled
- `backend/src/api/books.rs` — `render_cover_variants()` now generates 4 files per book: `.jpg`, `.thumb.jpg`, `.webp`, `.thumb.webp`; cover-serving handler negotiates via `Accept: image/webp`, falls back to `.jpg` when `.webp` absent (covers uploaded before this change)
- WebP encoding uses `WebPEncoder::new_lossless` — `image` 0.25.x has no quality-setting API; lossless is the correct choice for cover art fidelity at current API surface
- Tests: `test_cover_upload_generates_webp_variants`, `test_cover_serve_returns_webp_when_accepted`, `test_cover_serve_falls_back_to_jpeg_when_webp_not_accepted`, `test_cover_serve_falls_back_to_jpeg_when_webp_missing`
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-23 — Phase 12 Stage 6: X-RateLimit-* and Retry-After headers (dce57c7)

- `backend/src/middleware/security_headers.rs` — rate-limit header middleware: `X-RateLimit-Limit` and `X-RateLimit-Reset` on all responses from rate-limited groups; `Retry-After` on 429 only; `X-RateLimit-Remaining` forwarded if already present, omitted otherwise
- Middleware wired into auth public routes (`auth.rs`) and global API router (`mod.rs`)
- Tests: `test_auth_endpoint_returns_ratelimit_headers`, `test_429_response_includes_retry_after`, `test_retry_after_value_is_positive_integer`
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-23 — Phase 12 Stage 5: JSON logging + /health endpoint (453dd19)

- `backend/Cargo.toml` — `tracing-subscriber` features updated to include `json`
- `backend/src/lib.rs` — `LOG_FORMAT` env switch: default JSON, `LOG_FORMAT=text` for human-readable local dev
- `backend/src/api/health.rs` — new handler; `HealthResponse { status, version, db, search }`; DB checked via `SELECT 1`; Meilisearch checked via existing search backend abstraction (`backend_name` + `is_available`); 503 only on DB degradation; search degraded does not affect HTTP status
- `backend/src/api/mod.rs` — `/health` route registered without auth middleware
- `config.example.toml` — `# LOG_FORMAT=text for human-readable output` comment added
- Tests: `test_health_returns_200_with_ok_status`, `test_health_includes_version_string`, `test_health_reports_search_disabled_when_meilisearch_not_configured`, `test_health_requires_no_auth`
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-23 — Phase 12 Stage 4: S3 range request support — audio + PDF streaming restored (4f9c764)

- `backend/src/storage.rs` — `GetRangeResult { bytes, content_range, total_length, partial }`; `get_range(path, Option<(u64,u64)>)` on `StorageBackend` trait; `get_bytes` kept as default wrapper
- `LocalFsStorage::get_range` — full, bounded, and open-ended ranges (`u64::MAX` clamped to file size); `tokio::fs::File` + seek + `read_exact`
- `S3Storage::get_range` — passes `Range: bytes=start-end` to `GetObject`; proxies `Content-Range` from S3 response; removed old "range unsupported" path
- `backend/src/api/books.rs` — `parse_range()` handles open-ended ranges; local no-range still uses `ServeFile`; local-with-range and all S3 paths use `get_range`; returns 206 + `Content-Range` + `Accept-Ranges: bytes`
- Tests: `test_local_storage_get_range_returns_partial_bytes`, `test_local_storage_get_range_open_end`, `test_local_storage_get_range_none_returns_full`, `test_s3_get_range_passes_range_header` (`#[ignore]`), `test_download_returns_206_for_range_request`, `test_stream_returns_206_for_range_request`
- `cargo test --workspace` + `cargo clippy -- -D warnings` passing

---

## 2026-04-23 — Phase 12 Stage 3: FR/DE/ES translation completion + i18n CI (dd602ce)

- Added missing `admin.scheduled_tasks` key to `fr`, `de`, `es` locale files
- Locale files confirmed at canonical path: `apps/web/public/locales/` (not `src/locales/`)
- `scripts/check-translations.ts` — recursive key diff against EN base; exits non-zero on missing keys; warns on orphaned keys (non-blocking)
- `package.json` root — `check:i18n` script (`tsx scripts/check-translations.ts`)
- `.github/workflows/i18n-check.yml` — blocking CI workflow; triggers on locale file changes
- `docs/CONTRIBUTING.md` — translation contribution guide (adding new locales, fixing existing, CI enforcement)
- Known orphan: `book.unarchive` exists in locale files but not in EN base — non-blocking warning, tracked for cleanup
- `pnpm check:i18n` → ✓ 100% coverage for fr, de, es

---

## 2026-04-23 — Phase 12 Stage 2: Deployment runbooks + backup/restore scripts (74232cc)

- `docs/DEPLOY.md` — comprehensive deployment runbook: Tier 1 (SQLite, single container), Tier 2 (MariaDB, multi-replica), Caddy + nginx TLS config, Meilisearch + S3 optional setup, upgrade procedure, troubleshooting table
- `scripts/backup.sh` — production backup script; `--db-only` / `--files-only` flags; auto-detects SQLite vs MariaDB from `DATABASE_URL`; rsync for book files
- `scripts/restore.sh` — restore from backup; handles both SQLite copy and MariaDB import via gunzip; confirms target before overwrite

---

## 2026-04-23 — Phase 12 Stage 1: Playwright E2E suite + reading-progress regression fix

- Playwright E2E suite under `apps/web/e2e/` — auth, library, reader, search, admin specs
- `apps/web/playwright.config.ts` — Chromium, `PLAYWRIGHT_BASE_URL` env, `webServer` wiring, sequential run
- `apps/web/e2e/helpers/auth.ts` — `createUser`, `login`, `loginAsAdmin` helpers; `storageState` pattern for reader spec
- `apps/web/e2e/fixtures/test.epub` — public-domain EPUB fixture (The Yellow Wallpaper, <200KB)
- `.github/workflows/e2e.yml` — CI workflow, non-blocking (`continue-on-error: true`), Playwright report artifact on failure
- `apps/web/package.json` — `@playwright/test` devDep; `test:e2e`, `test:e2e:headed`, `test:e2e:ui` scripts
- `apps/web/vite.config.ts` — dev server bound to loopback for sandbox/CI compatibility
- **Backend regression fix**: `GET /api/v1/books/:id/progress` route was missing from `backend/src/api/books.rs`; reading progress was never surfaced to the library list
- `packages/shared/src/client.ts` — switched to canonical progress route
- `backend/tests/test_books.rs` — `test_reading_progress_surfaces_in_library_list` regression test added
- Full browser-side suite: 21 tests passing in local run; Chromium sandbox blocked on macOS CI (environment constraint, not app issue)

---

## 2026-04-22 — Phase 11 Stage 3: S3-compatible storage backend

- `StorageBackend` trait made fully async (`put`, `delete`, `get_bytes`)
- `LocalFsStorage` updated with async trait impl; `get_bytes` via `tokio::fs::read`
- New `backend/src/storage_s3.rs` — `S3Storage` using `aws-sdk-s3`; endpoint_url override for MinIO/R2/B2; key sanitization; `resolve()` unsupported (returns `None`)
- Config: `[storage]` + `[storage.s3]` sections in `config.toml`; S3 secret redacted in debug output; backend validated at startup
- Runtime dispatch in file-serving handlers: `ServeFile` for local (range support), `get_bytes` + full response for S3 (range degraded, documented)
- Text extraction falls back to temp-file download when `resolve()` returns `None` (S3 path)
- Cargo deps: `aws-sdk-s3`, `tempfile` added
- `backend/tests/test_storage_s3.rs` — unit tests + `#[ignore]` roundtrip test for real S3/MinIO endpoint
- ARCHITECTURE.md updated: storage backend comparison table, local→S3 migration steps

---

## 2026-04-22 — Phase 11 Stage 2: 2FA/TOTP — setup, login flow, backup codes, admin disable

- Migration 0014: `ALTER TABLE users ADD COLUMN totp_secret TEXT / totp_enabled INTEGER`; `totp_backup_codes` table
- `backend/src/auth/totp.rs` — TOTP crypto (totp-rs), AES-256-GCM encrypted secret, HKDF key derivation, backup code hashing
- Backend routes: TOTP setup/confirm/disable, `POST /auth/totp/verify` for pending-token login step, admin disable at `DELETE /admin/users/:id/totp`
- `totp_pending` JWT enforced in `backend/src/middleware/auth.rs` — cannot access non-TOTP routes until verified
- DB queries in `backend/src/db/queries/totp.rs`
- Shared types/client updated in `packages/shared/src/client.ts` and `packages/shared/src/types.ts`
- Web UI: TOTP step in `LoginPage.tsx`, setup/disable flow in `ProfilePage.tsx`, admin disable in `UsersPage.tsx`
- Mobile: TOTP step added to `apps/mobile/src/app/login.tsx`
- 14 backend integration tests in `backend/tests/test_totp.rs`

---

## 2026-04-22 — Phase 11 Stage 1: Mobile search screen (FTS + semantic)

- Replaced stub `apps/mobile/src/app/(tabs)/search.tsx` with full search screen
- Debounced FTS search via shared API client; pagination support
- Semantic search tab gated by `GET /api/v1/llm/health` — grayed out when LLM disabled
- Result cards with cover/placeholder, title, author, semantic score badge
- Slide-up filter panel (language, format, sort, order) via `@gorhom/bottom-sheet`
- `useDebounce` hook at `apps/mobile/src/hooks/useDebounce.ts`
- `CoverPlaceholder` component at `apps/mobile/src/components/CoverPlaceholder.tsx`
- Test coverage in `apps/mobile/src/tests/SearchScreen.test.tsx`
- Type shims for NativeWind, expo-constants, react-test-renderer
- Supporting fixes: `apps/mobile/src/lib/db.ts`, `apps/mobile/src/app/reader/[id].tsx`, `packages/shared/src/client.ts`

---

## 2026-04-22 — Phase 10 Stage 5 + Stage 6 review fixes

- Review fixes from `/review` pass on Phase 10 Stages 5 and 6

---

## 2026-04-21 — Phase 10 Stage 5: Extended Format Support + RAG

- DJVU reader — server-side page extraction, `DjvuReader.tsx`
- Audio streaming — MP3/M4B/OGG support via range-request stream endpoint
- MOBI/AZW3 reader — server-side conversion to HTML
- RAG text extraction extended to DJVU and MOBI formats
- `document_type` CHECK constraint extended with `'audiobook'`

---

## 2026-04-21 — Phase 10 Stage 7: Scheduled Tasks UI + Update Checker

- `scheduled_tasks` table (migration 0013)
- Scheduler runs inside the Axum process — polls `next_run_at`
- Admin UI: scheduled tasks list, create/edit/delete, last/next run display
- In-app update checker: `GET /admin/system/updates` — compares against GitHub releases API
- Admin dashboard banner when a newer release is available

---

## 2026-04-20 — Phase 10 Stage 6: i18n Framework

- `react-i18next` integrated into web app
- Starter translations: EN (base), FR, DE, ES
- `GET /locale` endpoint — list available locales
- User locale preference stored in DB; falls back to browser `Accept-Language`
- Locale picker in profile settings

---

## 2026-04-20 — Phase 10 Stage 4: Merge Duplicates + Custom Columns UI

- Shared client/types in `packages/shared` for merge and custom columns
- `POST /admin/books/merge` — merge duplicate books (keep target, absorb source formats/tags)
- Custom columns browser in Admin panel
- Bulk metadata edit extended to support custom column values

---

## 2026-04-19 — Phase 10 Stage 3: Per-User Tag Restrictions + Proxy Auth

- `user_tag_restrictions` table (migration 0012)
- `GET/PUT /users/me/tag-restrictions` — per-user allow/block tag filters at browse time
- Proxy authentication: `X-Remote-User` header support with configurable trusted proxy list
- Proxy auth creates local user on first match (same flow as OAuth)

---

## 2026-04-18 — Phase 10 Stage 2: Extended OPDS Feeds

- OPDS browse feeds for publisher, language, and ratings (`/opds/publishers`, `/opds/languages`, `/opds/ratings/:rating`)
- Publisher stored in `books.flags` JSON column; accessed via `json_extract`
- All feeds OPDS-PS 1.2 compliant; download links remain token-gated

---

## 2026-04-17 — Phase 10 Stage 1: Per-User Read/Unread + Download History

- `book_user_state` table (migration 0010): per-user `is_read` + `is_archived`
- `download_history` table (migration 0011)
- `GET/PUT /books/:id/state` endpoints
- `GET /users/me/downloads` — paginated download history
- Library grid badge for unread status

---

## 2026-04-15 — Phase 7 Stage 3: Criterion Benchmarks

- Criterion benchmark suite for hot query paths
- `PERFORMANCE.md` documenting results
- Benchmark CI job (non-blocking)

---

## 2026-04-14 — Phase 7 Stage 2: OWASP Hardening

- CORS policy tightened
- CSP tuned for epub.js `unsafe-inline` requirement
- SSRF guard at startup for LLM endpoint configuration
- Audit log coverage expanded
- `SECURITY.md` documenting full OWASP findings

---

## 2026-04-13 — Phase 7 Stage 1: Multi-Arch Docker

- Multi-stage Dockerfile producing `linux/amd64`, `linux/arm64`, `linux/arm/v7` images
- `docker.yml` CI workflow for image builds
- Production-grade `docker-compose.yml` with Meilisearch + optional Caddy

---

## Earlier Phases (Phases 1–6 and Phase 9)

Full build plan and phase-by-phase feature list: [ARCHITECTURE.md](ARCHITECTURE.md)
Per-phase Codex commands: `CODEX_COMMANDS.md`, `CODEX_COMMANDS_PHASE2.md` through `CODEX_COMMANDS_PHASE10.md`
