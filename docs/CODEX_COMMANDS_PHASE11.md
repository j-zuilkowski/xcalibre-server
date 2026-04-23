# Codex Desktop App — autolibre Phase 11: Open Items

## What Phase 11 Builds

Closes the three remaining open items identified after Phase 10 completes:

- **Stage 1** — Mobile search screen (stub placeholder → full FTS + semantic search on iOS/Android)
- **Stage 2** — 2FA/TOTP (per-user opt-in two-factor auth for web + mobile login)
- **Stage 3** — S3-compatible storage backend (trait implementation + config-driven switching)

## Key Design Decisions

**Mobile Search:**
- The shared API client in `packages/shared/api/` is already wired for both web and mobile — no new fetch logic needed
- NativeWind mirrors web design language; use `FlatList` (not CSS grid) for results
- Filter panel uses a bottom sheet (`@gorhom/bottom-sheet`) matching mobile pattern from DESIGN.md
- Semantic tab grayed out when `llm.enabled = false`, same as web
- Search input lives in the tab screen; no separate header — the existing tab bar handles navigation

**2FA/TOTP:**
- `totp-rs` crate (RFC 6238 compliant, widely used in Rust ecosystem)
- Per-user opt-in — no global enforcement flag; each user independently enables/disables
- Login flow with TOTP enabled: password check passes → `{ totp_required: true }` response (no tokens yet) → client POSTs 6-digit code to `/auth/totp/verify` → tokens issued on success
- Backup codes: 8 single-use codes generated at setup, SHA-256-hashed in `totp_backup_codes` table
- Admin can disable TOTP for any user via admin panel (lockout recovery path)
- QR code rendered client-side from the `otpauth://` URI — no server-side image generation needed (`qrcode.react` on web, `react-native-qrcode-svg` on mobile)

**S3 Storage Backend:**
- `StorageBackend` trait extended with an async `get_bytes` method for streaming reads; `LocalFsStorage` implements it trivially with `tokio::fs::read`
- File serving updated to use `get_bytes` when `backend = "s3"` (proxy streaming — preserves per-request auth enforcement, no presigned URL leakage)
- S3-compatible endpoint override (`endpoint_url` config field) enables MinIO, Backblaze B2, Cloudflare R2
- `put` / `delete` made async throughout (breaking change internal to the crate only — no public API change)
- Covers stored under same relative paths in S3 bucket as on local filesystem — migration path is a one-time `aws s3 sync`
- `[storage]` section added to `config.toml`; defaults to `backend = "local"` — fully backward compatible

## Key Schema Facts (new tables this phase)

```sql
-- Stage 2 — 2FA/TOTP
ALTER TABLE users ADD COLUMN totp_secret TEXT;  -- NULL = TOTP not enabled; encrypted at rest
ALTER TABLE users ADD COLUMN totp_enabled INTEGER NOT NULL DEFAULT 0;

CREATE TABLE totp_backup_codes (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash   TEXT NOT NULL,    -- SHA-256 of the 8-char backup code
    used_at     TEXT,             -- NULL = unused
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_totp_backup_user ON totp_backup_codes(user_id);
```

## Reference Files

Read before starting each stage:
- `docs/ARCHITECTURE.md` — overall design constraints
- `docs/SCHEMA.md` — current schema
- `apps/mobile/src/app/(tabs)/search.tsx` — the stub being replaced (Stage 1)
- `apps/web/src/features/search/SearchPage.tsx` — web implementation to port (Stage 1)
- `packages/shared/api/` — shared API client already used by mobile (Stage 1)
- `backend/src/middleware/auth.rs` — auth flow to extend for TOTP (Stage 2)
- `backend/src/api/auth.rs` — login handler to extend for TOTP (Stage 2)
- `backend/src/storage.rs` — `StorageBackend` trait to extend (Stage 3)
- `backend/src/config.rs` — config loading pattern to follow (Stage 3)

---

## STAGE 1 — Mobile Search Screen

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read apps/mobile/src/app/(tabs)/search.tsx,
apps/web/src/features/search/SearchPage.tsx,
packages/shared/api/, apps/mobile/src/app/(tabs)/library.tsx,
and apps/mobile/src/lib/sync.ts.
Now replace the mobile search stub with a fully functional search screen.

─────────────────────────────────────────
DELIVERABLE 1 — Full-Text Search Results
─────────────────────────────────────────

apps/mobile/src/app/(tabs)/search.tsx — replace the placeholder with a real screen:

  State:
    const [query, setQuery] = useState("")
    const [debouncedQuery] = useDebounce(query, 300)
    const [activeTab, setActiveTab] = useState<"fts" | "semantic">("fts")
    const [page, setPage] = useState(1)

  Layout (NativeWind):
    - Search input at top (TextInput, rounded, zinc-800 background, magnifying glass icon)
    - Two tab pills below: "Library" | "AI Semantic" (semantic grayed if !semanticEnabled)
    - FlatList of book result cards below
    - Pagination row at bottom (Previous / Next, current page / total pages)
    - Empty state: "No results" with a subtitle "Try a different search term"
    - Loading state: ActivityIndicator centred

  Search API call (FTS tab):
    Use the shared API client from packages/shared/api/ — call the same
    GET /api/v1/search?q={query}&page={page}&page_size=20 endpoint used by the web app.
    Trigger on debouncedQuery change (skip if query is empty — show an "Enter a search
    term" prompt instead).

  Book card in FlatList:
    - Cover image (GET /api/v1/books/:id/cover) or CoverPlaceholder (port the web
      component logic: deterministic colour from title hash, first letter of title)
    - Title (bold, zinc-50, single line with ellipsis)
    - Author(s) (zinc-400, single line)
    - Tapping a card navigates to the book detail screen:
      router.push({ pathname: "/book/[id]", params: { id: book.id } })

  Shared types: import BookSummary from packages/shared/types/ — already defined.

─────────────────────────────────────────
DELIVERABLE 2 — Semantic Search Tab
─────────────────────────────────────────

  Semantic tab availability: call GET /api/v1/llm/health on mount.
  Store result in state: const [semanticEnabled, setSemanticEnabled] = useState(false).
  The "AI Semantic" tab pill is rendered with opacity-40 and disabled if !semanticEnabled.
  A Pressable on the grayed tab shows a Toast/Alert: "Semantic search requires the AI
  features to be enabled on your server."

  When active tab is "semantic" and query is non-empty:
    Call GET /api/v1/search/semantic?q={query}&limit=20
    Results include a score field — show a small "score: 92%" badge on each card.
    Semantic search does not paginate (limit-based) — hide pagination row.

  Tab switching resets page to 1 and clears results.

─────────────────────────────────────────
DELIVERABLE 3 — Filter Bottom Sheet
─────────────────────────────────────────

  A filter icon button in the top-right corner of the search screen opens a bottom sheet.
  Add @gorhom/bottom-sheet to apps/mobile/package.json if not already present.

  Filter options in the bottom sheet:
    - Language: text input
    - Format: selector (EPUB / PDF / MOBI / Any)
    - Sort: selector (Title / Author / Date added / Rating)
    - Order: toggle (A→Z / Z→A)

  Filters are appended to the FTS search query params. Apply button closes the sheet
  and re-runs the search. Reset button clears all filters.

  Bottom sheet follows the mobile slide-up panel pattern from DESIGN.md.

─────────────────────────────────────────
UTILITIES
─────────────────────────────────────────

  apps/mobile/src/hooks/useDebounce.ts — add if not already present:
    export function useDebounce<T>(value: T, delay: number): T {
      const [debounced, setDebounced] = useState(value)
      useEffect(() => {
        const timer = setTimeout(() => setDebounced(value), delay)
        return () => clearTimeout(timer)
      }, [value, delay])
      return debounced
    }

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

  apps/mobile/src/__tests__/SearchScreen.test.tsx:
    test_search_input_triggers_api_call_after_debounce
    test_empty_query_shows_prompt_not_results
    test_semantic_tab_grayed_when_llm_disabled
    test_result_card_navigates_to_book_detail
    test_pagination_next_increments_page

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
pnpm --filter @autolibre/mobile tsc --noEmit
pnpm --filter @autolibre/mobile test
git add apps/mobile/ packages/
git commit -m "Phase 11 Stage 1: mobile search screen (FTS + semantic)"
```

---

## STAGE 2 — 2FA / TOTP

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, backend/src/api/auth.rs,
backend/src/middleware/auth.rs, backend/src/db/queries/auth.rs (or equivalent),
apps/web/src/features/auth/LoginPage.tsx, and apps/web/src/features/auth/.
Now implement Stage 2 of Phase 11: per-user opt-in TOTP two-factor authentication.

─────────────────────────────────────────
SCHEMA
─────────────────────────────────────────

backend/migrations/sqlite/0014_totp.sql:
  ALTER TABLE users ADD COLUMN totp_secret TEXT;
    -- NULL = TOTP not set up; non-NULL = TOTP configured (encrypted, see below)
  ALTER TABLE users ADD COLUMN totp_enabled INTEGER NOT NULL DEFAULT 0;
    -- 0 = disabled (even if secret exists — allows pre-setup without enforcing)

  CREATE TABLE totp_backup_codes (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash  TEXT NOT NULL,    -- SHA-256 hex of the 8-char alphanumeric code
    used_at    TEXT,             -- NULL = unused; set on consumption
    created_at TEXT NOT NULL
  );
  CREATE INDEX idx_totp_backup_user ON totp_backup_codes(user_id);

backend/migrations/mariadb/0013_totp.sql — equivalent MariaDB DDL.

─────────────────────────────────────────
DEPENDENCIES
─────────────────────────────────────────

backend/Cargo.toml — add:
  totp-rs = { version = "5", features = ["gen_secret", "qr"] }

─────────────────────────────────────────
DELIVERABLE 1 — TOTP Setup Flow
─────────────────────────────────────────

backend/src/api/auth.rs — add routes:

  GET /auth/totp/setup  (requires valid JWT, user must NOT already have totp_enabled=1)
    1. Generate a random 20-byte TOTP secret: totp_rs::Secret::generate_secret()
    2. Encode as base32 for storage (never raw bytes in DB)
    3. Encrypt the base32 secret with AES-256-GCM using a key derived from the
       app's jwt_secret via HKDF-SHA256 (label: "totp-encryption-key").
       Store the ciphertext (base64) in users.totp_secret.
       Do NOT store the plaintext secret.
    4. Compute the otpauth:// URI:
         otpauth://totp/{issuer}:{email}?secret={base32}&issuer={issuer}&algorithm=SHA1&digits=6&period=30
       where issuer = config.app.library_name (or "autolibre" if unset)
    5. Return:
         { "secret_base32": "JBSWY3DPEHPK3PXP", "otpauth_uri": "otpauth://totp/..." }
       The client renders the QR code from otpauth_uri using qrcode.react.
    6. IMPORTANT: at this point totp_enabled remains 0 — the setup is not confirmed yet.

  POST /auth/totp/confirm  (requires valid JWT)
    Body: { "code": "123456" }
    1. Decrypt users.totp_secret to get the base32 secret
    2. Validate the 6-digit code using totp_rs with a ±1 step window (30-second tolerance)
    3. On success:
       a. SET totp_enabled = 1 on the user
       b. Generate 8 backup codes:
          - Each code: 8 random alphanumeric characters (use rand::distributions::Alphanumeric)
          - Hash each with SHA-256, store in totp_backup_codes table
       c. Return: { "backup_codes": ["ABCD1234", "EFGH5678", ...] }
          (only returned ONCE — the user must save these; never retrievable again)
    4. On invalid code: 422 { "error": "invalid_totp", "message": "Invalid or expired code" }

  POST /auth/totp/disable  (requires valid JWT + current password)
    Body: { "password": "current_password" }
    1. Verify the current password against users.password_hash (argon2 verify)
    2. If valid: SET totp_enabled = 0, SET totp_secret = NULL,
                 DELETE FROM totp_backup_codes WHERE user_id = ?
    3. Return 204 No Content

backend/src/api/admin.rs — add route (admin only):
  POST /admin/users/:id/totp/disable
    No body required. Disables TOTP for the given user (lockout recovery).
    Same DB operations as self-disable above. Returns 204.

─────────────────────────────────────────
DELIVERABLE 2 — Login Flow with TOTP
─────────────────────────────────────────

backend/src/api/auth.rs — modify POST /auth/login handler:

  Current flow: validate password → issue access + refresh tokens → return 200.

  New flow:
    1. Validate username + password as before.
    2. If password is invalid: return 401 (unchanged — do not leak TOTP status).
    3. If password is valid AND users.totp_enabled = 0: return 200 with tokens (unchanged).
    4. If password is valid AND users.totp_enabled = 1:
       a. Issue a short-lived (5-minute) "TOTP pending" JWT with a special claim:
          { "sub": user_id, "totp_pending": true, "exp": now + 300 }
          This token is NOT an access token — it only works with /auth/totp/verify.
       b. Return 200:
          { "totp_required": true, "totp_token": "<pending JWT>" }
          Do NOT return an access token or refresh token at this stage.

  New route: POST /auth/totp/verify
    Header: Authorization: Bearer <totp_token>
    Body: { "code": "123456" }

    Middleware: validate the Bearer token; reject if it is NOT a totp_pending token.
    Handler:
      1. Decrypt users.totp_secret → base32 secret
      2. Validate the 6-digit code (±1 step window)
      3. If invalid: 422 { "error": "invalid_totp" }
         Increment the standard failed-login counter (reuse lockout logic).
      4. If valid: issue real access + refresh tokens and return 200 — same
         response shape as POST /auth/login success.
         Reset the failed-login counter.

  New route: POST /auth/totp/verify-backup
    Header: Authorization: Bearer <totp_token>
    Body: { "code": "ABCD1234" }

    Handler:
      1. SHA-256 hash the submitted code
      2. SELECT * FROM totp_backup_codes WHERE user_id = ? AND code_hash = ? AND used_at IS NULL
      3. If no row found: 422 { "error": "invalid_backup_code" }
      4. If found: SET used_at = now() on that row, then issue tokens (same as verify success)

─────────────────────────────────────────
DELIVERABLE 3 — Web UI
─────────────────────────────────────────

apps/web/src/features/auth/LoginPage.tsx — extend login flow:
  After POST /auth/login:
    If response.totp_required === true:
      - Transition to a TOTP step UI (same page, second panel — no navigation):
          - "Two-factor authentication" heading
          - 6-digit code input (auto-focus, numeric keyboard hint)
          - "Use a backup code instead" link → shows an 8-char text input
          - "Verify" button → POST /auth/totp/verify with totp_token from login response
          - On success: store access + refresh tokens, redirect to library
          - On failure: show "Invalid code" inline error

apps/web/src/features/profile/ProfilePage.tsx (or equivalent) — add TOTP section:
  If totp_enabled === false:
    "Enable two-factor authentication" button
    → calls GET /auth/totp/setup
    → renders QR code from otpauth_uri using qrcode.react (add to apps/web package.json)
    → shows manual entry code (secret_base32) below the QR as fallback
    → 6-digit confirmation input + "Confirm" button → POST /auth/totp/confirm
    → on success: shows backup codes in a copy-to-clipboard list with warning
      "Save these backup codes. They will not be shown again."
    → "Done" button dismisses the setup flow

  If totp_enabled === true:
    "Two-factor authentication: Enabled ✓" status badge
    "Disable 2FA" button → confirmation dialog → prompts current password
    → POST /auth/totp/disable

apps/web/src/features/admin/UsersPage.tsx — add per-row action:
  "Disable 2FA" button (appears only if user has totp_enabled = 1)
  → POST /admin/users/:id/totp/disable

─────────────────────────────────────────
DELIVERABLE 4 — Mobile Login Flow
─────────────────────────────────────────

apps/mobile/src/app/login.tsx — extend login flow:
  Same pattern as web — if totp_required === true, show a TOTP code step
  before storing tokens:
    - Full-screen second step (or push a new screen: /totp-verify)
    - 6-digit numeric TextInput (keyboardType="number-pad")
    - "Use backup code" link → switches to a text input
    - "Verify" button → POST /auth/totp/verify → on success, store tokens in SecureStore

  Add apps/mobile/src/app/totp-verify.tsx if a separate screen is cleaner.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_totp.rs:
  test_setup_generates_valid_otpauth_uri
  test_confirm_with_valid_code_enables_totp
  test_confirm_with_invalid_code_returns_422
  test_confirm_returns_8_backup_codes
  test_login_with_totp_disabled_returns_tokens_directly
  test_login_with_totp_enabled_returns_totp_required
  test_verify_with_valid_code_returns_tokens
  test_verify_with_invalid_code_returns_422
  test_verify_backup_code_marks_used
  test_verify_used_backup_code_returns_422
  test_access_token_rejected_as_totp_token
  test_totp_token_rejected_as_access_token
  test_admin_can_disable_totp_for_any_user
  test_self_disable_requires_correct_password

─────────────────────────────────────────
SECURITY NOTES
─────────────────────────────────────────

- Never store the TOTP secret in plaintext — encrypt with AES-256-GCM as described.
  The encryption key is derived from jwt_secret via HKDF; never hard-code a key.
- The "TOTP pending" JWT must be rejected by all routes except /auth/totp/verify and
  /auth/totp/verify-backup. Check for the totp_pending claim in middleware and
  return 403 Forbidden if it is presented to any other route.
- TOTP failure must increment the same lockout counter as password failures.
- Backup codes must be single-use — consuming a code sets used_at immediately in the
  same transaction as the token issuance. No race condition.
- Do not reveal in the login response whether a user has TOTP enabled until after the
  password check succeeds — prevents user enumeration.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @autolibre/web build
pnpm --filter @autolibre/mobile tsc --noEmit
git add backend/ apps/
git commit -m "Phase 11 Stage 2: 2FA/TOTP — setup, login flow, backup codes, admin disable"
```

---

## STAGE 3 — S3-Compatible Storage Backend

**Priority: Low (LocalFs covers all self-hosted deployments; S3 needed for cloud/multi-instance)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/storage.rs, backend/src/config.rs, backend/src/api/books.rs,
backend/src/state.rs, and backend/Cargo.toml.
Now implement Stage 3 of Phase 11: S3-compatible storage backend.

─────────────────────────────────────────
DESIGN OVERVIEW
─────────────────────────────────────────

The existing StorageBackend trait has three methods:
  fn put(&self, relative_path: &str, bytes: &[u8]) -> anyhow::Result<()>
  fn delete(&self, relative_path: &str) -> anyhow::Result<()>
  fn resolve(&self, relative_path: &str) -> anyhow::Result<PathBuf>

`resolve` returns a local filesystem path — this is correct for LocalFsStorage
but meaningless for S3. The approach:

  1. Extend the trait with an async `get_bytes` method (returns raw bytes).
     LocalFsStorage implements it via tokio::fs::read.
     S3Storage implements it via the S3 GetObject API.

  2. Make `put` async as well (S3 PutObject is inherently async).
     Update the trait signature and all call sites.

  3. `delete` becomes async too for consistency.

  4. The Axum file serving handlers (download, stream, cover) are updated:
     - If backend is LocalFs: continue using `resolve` → ServeFile (unchanged behavior)
     - If backend is S3: call `get_bytes` → stream the bytes directly from the handler

  5. `resolve` is kept for LocalFsStorage (used in text extraction, ingest pipeline).
     S3Storage's `resolve` returns Err("resolve not supported for S3 backend") — callers
     that need file contents must use `get_bytes` instead.

─────────────────────────────────────────
DEPENDENCIES
─────────────────────────────────────────

backend/Cargo.toml — add:
  aws-sdk-s3 = { version = "1", features = ["behavior-version-latest"] }
  aws-config  = { version = "1", features = ["behavior-version-latest"] }
  aws-credential-types = "1"

─────────────────────────────────────────
TRAIT EXTENSION
─────────────────────────────────────────

backend/src/storage.rs — rewrite the trait as async-capable:

  use bytes::Bytes;

  #[async_trait::async_trait]
  pub trait StorageBackend: Send + Sync {
    async fn put(&self, relative_path: &str, bytes: Bytes) -> anyhow::Result<()>;
    async fn delete(&self, relative_path: &str) -> anyhow::Result<()>;
    async fn get_bytes(&self, relative_path: &str) -> anyhow::Result<Bytes>;

    // Only meaningful for LocalFs — S3 returns Err.
    // Retained so existing ingest/text extraction code compiles unchanged.
    fn resolve(&self, relative_path: &str) -> anyhow::Result<std::path::PathBuf>;
  }

Add async_trait to backend/Cargo.toml if not already present:
  async-trait = "0.1"

Update LocalFsStorage to implement the new async trait:
  - put: tokio::fs::write (create parent dirs first)
  - delete: tokio::fs::remove_file (ignore NotFound)
  - get_bytes: tokio::fs::read → Ok(Bytes::from(vec))
  - resolve: unchanged (existing sanitize_relative_path logic)

─────────────────────────────────────────
CONFIGURATION
─────────────────────────────────────────

backend/src/config.rs — add new [storage] section:

  [storage]
  backend = "local"   -- "local" | "s3"

  [storage.s3]
  bucket       = ""
  region       = "us-east-1"
  endpoint_url = ""   -- override for S3-compatible APIs (MinIO, B2, Cloudflare R2)
                      -- leave empty to use AWS standard endpoint
  access_key   = ""
  secret_key   = ""
  key_prefix   = ""   -- optional path prefix within the bucket, e.g. "autolibre/"

Add StorageSection and S3Section structs. Include StorageSection in AppConfig.

validate_config():
  If storage.backend == "s3":
    - Verify bucket, access_key, secret_key are non-empty; bail with clear error if any are blank
    - Log: "Storage backend: S3 — bucket={bucket}, region={region}"
  If storage.backend == "local":
    - Log: "Storage backend: local filesystem — path={storage_path}"

Redact secret_key in the Debug impl (same pattern as jwt_secret).

─────────────────────────────────────────
S3STORAGE IMPLEMENTATION
─────────────────────────────────────────

backend/src/storage_s3.rs — new file:

  use aws_sdk_s3::{Client, Config, primitives::ByteStream};
  use aws_credential_types::Credentials;

  pub struct S3Storage {
    client: Client,
    bucket: String,
    key_prefix: String,
  }

  impl S3Storage {
    pub async fn new(cfg: &S3Section) -> anyhow::Result<Self> {
      let creds = Credentials::new(
        &cfg.access_key, &cfg.secret_key, None, None, "autolibre-config"
      );
      let mut builder = Config::builder()
        .credentials_provider(creds)
        .region(aws_sdk_s3::config::Region::new(cfg.region.clone()))
        .behavior_version_latest();
      if !cfg.endpoint_url.is_empty() {
        builder = builder
          .endpoint_url(&cfg.endpoint_url)
          .force_path_style(true);  // required for MinIO/R2/B2
      }
      let client = Client::from_conf(builder.build());
      Ok(Self {
        client,
        bucket: cfg.bucket.clone(),
        key_prefix: cfg.key_prefix.trim_end_matches('/').to_string(),
      })
    }

    fn s3_key(&self, relative_path: &str) -> String {
      // Sanitize: strip leading slashes and .. components
      let clean: String = relative_path
        .split('/')
        .filter(|s| !s.is_empty() && *s != "..")
        .collect::<Vec<_>>()
        .join("/");
      if self.key_prefix.is_empty() {
        clean
      } else {
        format!("{}/{}", self.key_prefix, clean)
      }
    }
  }

  #[async_trait::async_trait]
  impl StorageBackend for S3Storage {
    async fn put(&self, relative_path: &str, bytes: Bytes) -> anyhow::Result<()> {
      let key = self.s3_key(relative_path);
      self.client
        .put_object()
        .bucket(&self.bucket)
        .key(&key)
        .body(ByteStream::from(bytes))
        .send()
        .await
        .with_context(|| format!("S3 PutObject {key}"))?;
      Ok(())
    }

    async fn delete(&self, relative_path: &str) -> anyhow::Result<()> {
      let key = self.s3_key(relative_path);
      match self.client.delete_object().bucket(&self.bucket).key(&key).send().await {
        Ok(_) => Ok(()),
        Err(e) => {
          // S3 DeleteObject on a non-existent key returns success; this handles
          // edge cases from other S3-compatible APIs.
          let svc_err = e.into_service_error();
          if svc_err.is_no_such_key() { Ok(()) } else { Err(svc_err.into()) }
        }
      }
    }

    async fn get_bytes(&self, relative_path: &str) -> anyhow::Result<Bytes> {
      let key = self.s3_key(relative_path);
      let resp = self.client
        .get_object()
        .bucket(&self.bucket)
        .key(&key)
        .send()
        .await
        .with_context(|| format!("S3 GetObject {key}"))?;
      let bytes = resp.body.collect().await?.into_bytes();
      Ok(bytes)
    }

    fn resolve(&self, relative_path: &str) -> anyhow::Result<std::path::PathBuf> {
      anyhow::bail!(
        "resolve() is not supported for the S3 backend (path: {relative_path}). \
         Use get_bytes() to retrieve file contents."
      )
    }
  }

─────────────────────────────────────────
STARTUP WIRING
─────────────────────────────────────────

backend/src/state.rs or backend/src/lib.rs (wherever AppState is built):
  Match on config.storage.backend:
    "local" => Arc::new(LocalFsStorage::new(&config.app.storage_path)) as Arc<dyn StorageBackend>
    "s3"    => Arc::new(S3Storage::new(&config.storage.s3).await?) as Arc<dyn StorageBackend>
    other   => anyhow::bail!("Unknown storage backend: {}", config.storage.backend)

─────────────────────────────────────────
UPDATING FILE SERVING HANDLERS
─────────────────────────────────────────

backend/src/api/books.rs — update download_format and stream_format handlers:

  Current behavior: resolve path → ServeFile → native range request support.

  New behavior (select at runtime, not compile time):
    let path_result = state.storage.resolve(&format_file.path);
    match path_result {
      Ok(local_path) => {
        // LocalFs path — continue using ServeFile for full range request support
        use tower_http::services::ServeFile;
        let serve = ServeFile::new(&local_path);
        ...
      }
      Err(_) => {
        // S3 backend — get_bytes and stream manually
        let bytes = state.storage.get_bytes(&format_file.path).await
          .map_err(|e| AppError::Internal(e.to_string()))?;
        // Build response with appropriate Content-Type and Content-Disposition
        // Note: S3 serving does NOT support range requests in this implementation.
        // Range request support for S3 is a future enhancement.
        Response::builder()
          .status(200)
          .header("Content-Type", mime_type)
          .header("Content-Disposition", disposition)
          .header("Content-Length", bytes.len().to_string())
          .body(axum::body::Body::from(bytes))
          .map_err(|e| AppError::Internal(e.to_string()))
      }
    }

  Add a note in the code comment: "S3 serving does not support range requests.
  Readers that depend on range requests (epub.js, react-pdf, HTML5 audio) will
  fall back to full-file loading. This is acceptable for files < 50MB. Large
  PDF streaming is degraded. Full range support requires S3 presigned URLs or
  a streaming proxy — deferred."

  Update cover serving handler the same way.

─────────────────────────────────────────
UPDATING INGEST PIPELINE
─────────────────────────────────────────

backend/src/ingest/ — update all callers of storage.put() and storage.delete()
  to await the now-async calls. Update function signatures to async where needed.

backend/src/ingest/text.rs — text extraction uses storage.resolve() to get a
  local path for file reading. This works correctly for LocalFs and correctly
  returns an error for S3.

  For S3: before calling list_chapters or extract_text, check if resolve succeeds.
  If it fails (S3 backend), use get_bytes to write the file to a temporary path
  first, then call the existing extraction logic on the temp path:

    let file_path = match state.storage.resolve(&format_file.path) {
      Ok(p) => p,
      Err(_) => {
        // S3 backend: download to a temp file for extraction
        let bytes = state.storage.get_bytes(&format_file.path).await?;
        let tmp = tempfile::NamedTempFile::new()?;
        tokio::fs::write(tmp.path(), &bytes).await?;
        tmp.into_temp_path().keep()?.into()
      }
    };

  This ensures text extraction (and therefore embeddings and RAG) works correctly
  for both backends.

─────────────────────────────────────────
CONFIG EXAMPLE
─────────────────────────────────────────

config.example.toml — add a new section below [llm]:

  [storage]
  backend = "local"   # "local" or "s3"

  # Only required when backend = "s3"
  [storage.s3]
  bucket       = "my-autolibre-library"
  region       = "us-east-1"
  endpoint_url = ""   # Override for MinIO / Backblaze B2 / Cloudflare R2
                      # Example (MinIO): "http://minio.local:9000"
                      # Example (R2):    "https://<account_id>.r2.cloudflarestorage.com"
  access_key   = ""
  secret_key   = ""
  key_prefix   = ""   # Optional. Example: "autolibre/" stores all files under that prefix

─────────────────────────────────────────
DOCUMENTATION NOTE
─────────────────────────────────────────

After completing the implementation, update the "Notes & Constraints" section in
docs/ARCHITECTURE.md to add:

  ### Storage Backends

  | Backend | Config | Notes |
  |---|---|---|
  | Local filesystem (default) | `backend = "local"` | Full range request support for streaming |
  | S3-compatible | `backend = "s3"` | Works with AWS S3, MinIO, Cloudflare R2, Backblaze B2; range request streaming degraded (full-file load) |

  **Migrating from local to S3:**
  1. Stop the server
  2. `aws s3 sync {storage_path}/ s3://{bucket}/{key_prefix}/ --delete`
  3. Update config.toml: set backend = "s3" and fill in S3 credentials
  4. Restart the server
  5. Verify by downloading a book

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_storage_s3.rs:

  test_local_storage_resolve_returns_path
    - LocalFsStorage with a temp dir
    - put "books/test.epub" → verify file exists at resolved path

  test_local_storage_get_bytes_returns_content
    - put "books/test.epub" with known bytes → get_bytes → assert content matches

  test_local_storage_delete_removes_file
    - put then delete → resolve → path does not exist

  test_local_storage_delete_missing_file_is_ok
    - delete "nonexistent/file.epub" → assert Ok(()) (not an error)

  test_s3_resolve_returns_error (unit test, no real S3 needed)
    - Create S3Storage with dummy config
    - assert resolve("any/path").is_err()

  test_s3_key_strips_traversal (unit test)
    - s3_key("../../etc/passwd") → assert result does not contain ".."

  test_s3_key_applies_prefix (unit test)
    - S3Storage with key_prefix = "autolibre"
    - s3_key("covers/ab/book.jpg") → assert == "autolibre/covers/ab/book.jpg"

  Integration tests (require real S3/MinIO — mark #[ignore]):
    test_s3_put_get_delete_roundtrip
      -- Run with: cargo test -- --ignored
      -- Requires S3_TEST_BUCKET, S3_TEST_REGION, S3_TEST_ENDPOINT env vars

─────────────────────────────────────────
SECURITY NOTES
─────────────────────────────────────────

- secret_key must be redacted in Debug output and never written to logs.
  Add a custom Debug impl for S3Section (same pattern as jwt_secret in AppConfig).
- The s3_key() sanitizer strips ".." and leading slashes before constructing the
  S3 key. This is the S3 equivalent of the path traversal guard in LocalFsStorage.
- endpoint_url is config-file-only (not admin-changeable at runtime).
  No SSRF risk beyond what is already noted for smtp_host.
- Access credentials live only in config.toml (with file permission check at startup).
  Never log credentials, never return them in any API response.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @autolibre/web build
git add backend/ docs/
git commit -m "Phase 11 Stage 3: S3-compatible storage backend (LocalFs unchanged, S3 trait impl)"
```

---

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/review` — verify shared API client usage, no duplicate fetch logic, correct tab routing |
| Stage 2 | `/review` + `/security-review` — TOTP token isolation, backup code single-use, secret encryption at rest, lockout counter integration |
| Stage 3 | `/review` + `/security-review` — path traversal in s3_key(), secret_key redaction, temp file cleanup in S3 text extraction path |

Run `/engineering:deploy-checklist` after Stage 3 before merging to main.
