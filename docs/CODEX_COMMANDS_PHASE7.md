# Codex Desktop App — calibre-web-rs Phase 7: Hardening

## What Phase 7 Builds

Production-readiness work across four areas:

- **Multi-arch Docker** — `linux/amd64`, `linux/arm64`, `linux/arm/v7` images built and
  published via GitHub Actions; cross-compilation in the Rust builder stage
- **Security hardening** — OWASP Top 10 review and fixes; CORS config; CSP tuned for
  epub.js; SSRF guard on LLM endpoint; audit log coverage verified
- **Performance benchmarks** — criterion benchmarks for key backend paths with Pi 4
  thresholds documented; `cargo bench` wired into CI as informational
- **Deployment documentation** — `docs/DEPLOY.md` covering Synology NAS, generic Docker,
  and bare metal; production docker-compose; environment variable reference; backup/restore

## Key Facts

- Existing Dockerfile has three stages (Rust builder, Node web-builder, Debian runtime) —
  Phase 7 extends the builder stage for cross-compilation, does not restructure it
- CI currently runs a single `ubuntu-latest` job — Phase 7 adds a separate `docker.yml`
  workflow for multi-arch image builds triggered on push to main
- `cargo audit` already runs in CI but `continue-on-error: false` is set — confirm it stays
  blocking after adding new dependencies
- Security headers middleware already exists — Phase 7 reviews coverage and fills gaps
- SSRF risk: LLM endpoint URL is admin-configurable in config.toml — must validate it
  does not point to loopback or RFC 1918 addresses (unless explicitly allowed)
- Pi 4 target: Cortex-A72 quad-core 1.5GHz, 4GB RAM — app must stay under 128MB RSS
  at idle with a 1000-book library

## Reference Files

Read these before starting each stage:
- `docker/Dockerfile` — existing multi-stage build
- `.github/workflows/ci.yml` — existing CI pipeline
- `docker/docker-compose.yml` — existing dev compose file
- `docs/ARCHITECTURE.md` — security section, deployment targets, Docker strategy

---

## STAGE 1 — Multi-arch Docker + CI

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docker/Dockerfile and .github/workflows/ci.yml. Now do Stage 1 of Phase 7.

Update the Docker build for multi-architecture support and add a GitHub Actions
workflow that builds and pushes multi-arch images.

Deliverables:

docker/Dockerfile — update the builder stage for cross-compilation:
  Add ARG TARGETARCH at the top of the builder stage.
  Install cross-compilation toolchains conditionally:
    amd64: no change (native)
    arm64: gcc-aarch64-linux-gnu + libc6-dev-arm64-cross
    arm/v7: gcc-arm-linux-gnueabihf + libc6-dev-armhf-cross
  Add the appropriate Rust target via rustup:
    arm64: aarch64-unknown-linux-gnu
    arm/v7: armv7-unknown-linux-gnueabihf
  Set CARGO_TARGET_* linker env vars per TARGETARCH before cargo build.
  Set CARGO_BUILD_TARGET to the cross target when not amd64.
  Runtime stage: no changes needed — Debian bookworm-slim is already multi-arch.

.github/workflows/docker.yml — new workflow:
  name: docker
  on:
    push:
      branches: [main]
      tags: ["v*"]
    pull_request:
      branches: [main]
  jobs:
    build:
      runs-on: ubuntu-latest
      permissions: { contents: read, packages: write }
      steps:
        - uses: actions/checkout@v4
        - uses: docker/setup-qemu-action@v3
        - uses: docker/setup-buildx-action@v3
        - uses: docker/login-action@v3
          with:
            registry: ghcr.io
            username: ${{ github.actor }}
            password: ${{ secrets.GITHUB_TOKEN }}
          if: github.event_name != 'pull_request'
        - uses: docker/build-push-action@v5
          with:
            context: .
            file: docker/Dockerfile
            platforms: linux/amd64,linux/arm64,linux/arm/v7
            push: ${{ github.event_name != 'pull_request' }}
            tags: |
              ghcr.io/${{ github.repository }}:latest
              ghcr.io/${{ github.repository }}:${{ github.sha }}
            cache-from: type=gha
            cache-to: type=gha,mode=max

docker/docker-compose.production.yml — new file:
  services:
    app:
      image: ghcr.io/${GITHUB_REPOSITORY:-calibre-web-rs}:latest
      restart: unless-stopped
      ports: ["127.0.0.1:8083:8083"]   # bind loopback only — Caddy sits in front
      volumes:
        - ./config.toml:/app/config.toml:ro
        - library_data:/app/storage
      environment:
        APP_DATABASE__URL: sqlite:///app/storage/library.db
        RUST_LOG: warn
      healthcheck:
        test: ["CMD", "curl", "-f", "http://localhost:8083/api/v1/llm/health"]
        interval: 30s
        timeout: 5s
        retries: 3
      depends_on:
        meilisearch:
          condition: service_healthy

    meilisearch:
      image: getmeili/meilisearch:v1.7
      restart: unless-stopped
      environment:
        MEILI_MASTER_KEY: ${MEILI_MASTER_KEY}   # required — no default in production
        MEILI_NO_ANALYTICS: "true"
        MEILI_ENV: production
      volumes:
        - meili_data:/meili_data
      healthcheck:
        test: ["CMD", "curl", "-f", "http://localhost:7700/health"]
        interval: 10s
        timeout: 3s
        retries: 5

    caddy:
      image: caddy:2-alpine
      restart: unless-stopped
      ports: ["80:80", "443:443"]
      volumes:
        - ./docker/Caddyfile:/etc/caddy/Caddyfile:ro
        - caddy_data:/data
      depends_on: [app]

  volumes:
    library_data:
    meili_data:
    caddy_data:

Verify locally (do not push — just confirm the build command parses):
  docker buildx build --platform linux/amd64,linux/arm64 \
    --file docker/Dockerfile . --dry-run 2>&1 | tail -5

Then run:
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 2 — Security Hardening (OWASP Top 10)

**Model: GPT-5.3-Codex**

**Note: after Codex output, run `/security-review` skill in Claude Code before approving.**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md (security section) and backend/src/middleware/.
Now do Stage 2 of Phase 7.

Perform an OWASP Top 10 review of the backend and fix all findings.

Review each item and either confirm it is covered or implement a fix:

A01 — Broken Access Control
  Audit every route in api/mod.rs — confirm all non-public routes require a valid JWT.
  Confirm Admin-only routes reject non-admin tokens with 403 (not 404).
  Confirm file serving routes check book ownership/download permission.

A02 — Cryptographic Failures
  Confirm argon2 params: memory >= 65536 KB, iterations >= 3, parallelism >= 4.
    If below: update AppConfig defaults and document the change.
  Confirm JWT secret is >= 256 bits — add a startup check that logs ERROR and exits
    if jwt_secret has fewer than 32 bytes after base64 decode.
  Confirm Secure cookie flag is set when APP_BASE_URL starts with https://.

A03 — Injection
  Confirm all DB queries use sqlx parameterised macros — grep for any string
    interpolation into SQL and fix if found.
  Confirm book file paths are validated against storage_path before serving.

A05 — Security Misconfiguration
  CORS: add tower-http CorsLayer to the router.
    Allowed origins: value of APP_BASE_URL from config (not wildcard).
    Allowed methods: GET, POST, PATCH, DELETE, OPTIONS.
    Allowed headers: Authorization, Content-Type.
    Max age: 3600.
  CSP: review current header value in security_headers.rs.
    epub.js requires: script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'
    Add: img-src 'self' data: blob: (covers loaded as data URIs by epub.js)
    Add: worker-src 'self' blob: (epub.js uses web workers)
  Confirm RUST_LOG defaults to "warn" in production (not "debug").

A06 — Vulnerable Components
  Run cargo audit. Fix any HIGH or CRITICAL advisories.
    MEDIUM advisories: document but do not block.

A07 — Identification and Authentication Failures
  Confirm refresh token is rotated on every /auth/refresh call (one-time use).
  Confirm /auth/login rate limit is enforced (10 req/min per IP via tower-governor).
  Confirm account lockout resets only on successful login, not on token refresh.

A09 — Security Logging and Monitoring
  Confirm audit_log rows are written for: login success, login failure, password change,
    book delete, role change. If any are missing: add them.
  Confirm audit_log entries never include password hashes or raw tokens.

A10 — Server-Side Request Forgery (SSRF)
  The LLM endpoint URL in config.toml is admin-configurable.
  Add validate_llm_endpoint(url: &str) -> Result<(), AppError> in config.rs:
    Parse the URL. Reject if scheme is not http or https.
    Resolve hostname. Reject if it resolves to:
      127.0.0.0/8 (loopback), 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
    unless config.llm.allow_private_endpoints = true (default false).
    Call this validator on startup for each configured LLM endpoint.
    Log WARNING (do not exit) if private endpoint detected and allow flag is false —
      the user may intentionally be pointing at a LAN model server.
  Note: for a home NAS + local LLM setup, private endpoints are the norm —
    make the warning clear but do not block.

After all fixes, run:
  cargo test --workspace 2>&1 | tail -20
  cargo clippy --workspace -- -D warnings 2>&1
  cargo audit 2>&1
  git diff --stat
```

**Paste output here → run `/security-review` → proceed if approved.**

---

## STAGE 3 — Performance Benchmarks

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read backend/src/db/queries/books.rs and backend/src/api/books.rs.
Now do Stage 3 of Phase 7.

Add criterion benchmarks for the key backend paths and document Pi 4 thresholds.

Deliverables:

backend/Cargo.toml — add to [dev-dependencies]:
  criterion = { version = "0.5", features = ["async_tokio"] }
  Add [[bench]] section:
    name = "api_benchmarks"
    harness = false

backend/benches/api_benchmarks.rs — new file:
  Use criterion::Criterion + tokio runtime.
  Benchmark group "database":
    bench_list_books_1000 — seed 1000 books in in-memory SQLite, measure list_books()
      with no filters, page_size=30. Target: < 10ms mean on developer hardware.
    bench_search_fts5 — seed 1000 books, measure FTS5 search for a 2-word query.
      Target: < 20ms mean.
    bench_get_book — seed 1 book with 3 authors + 5 tags, measure get_book() by id.
      Target: < 5ms mean.
  Benchmark group "ingest":
    bench_insert_book — measure single book INSERT with authors + tags in a transaction.
      Target: < 5ms mean.
  All benchmarks use in-memory SQLite (":memory:") — not the test DB file.
  Benchmarks are informational — they do not assert thresholds (thresholds are documented
    below, not enforced in code).

docs/PERFORMANCE.md — new file:
  # Performance Targets

  ## Hardware Reference: Raspberry Pi 4B (4-core Cortex-A72, 1.5GHz, 4GB RAM)

  These targets apply to a library of ~1000 books under 1–5 concurrent users.
  All measurements are p99 latency under light concurrent load.

  | Operation | Target | Notes |
  |---|---|---|
  | GET /books (page 30, no filter) | < 50ms | FTS5 disabled — pure SQL |
  | GET /books?q=search | < 100ms | FTS5 MATCH query |
  | GET /books/:id | < 20ms | Joins: authors, tags, formats, identifiers |
  | POST /books (upload, no LLM) | < 500ms | Cover extraction + DB write + meili index |
  | GET /books/:id/text (EPUB chapter) | < 200ms | EPUB unzip + HTML strip |
  | GET /search?mode=semantic | < 300ms | sqlite-vec ANN search, 1000 embeddings |
  | Memory at idle | < 64MB RSS | After startup, no active requests |
  | Memory under load | < 128MB RSS | 5 concurrent users, mix of reads |

  ## How to Benchmark Locally
  ```bash
  cargo bench --bench api_benchmarks 2>&1 | grep "time:"
  ```

  ## Notes on ARM Cross-Compilation
  The arm64 and armv7 images are cross-compiled on amd64 CI runners using
  QEMU emulation for the final stages. Performance of the cross-compiled binary
  is equivalent to native — cross-compilation only affects build time, not runtime.

.github/workflows/ci.yml — add bench job (informational, non-blocking):
  bench:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo bench --bench api_benchmarks -- --output-format bencher 2>&1
        continue-on-error: true   # informational only

TDD BUILD LOOP — do not stop until workspace tests pass:

  LOOP:
    cargo test --workspace 2>&1 | tail -20

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file.
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo bench --bench api_benchmarks 2>&1 | grep "time:"
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 4 — Deployment Documentation

**Model: GPT-5.4-Mini**

**Note: after this stage, run `/engineering:deploy-checklist` skill in Claude Code.**

**Paste this into Codex:**

```
Read docker/docker-compose.production.yml and docs/ARCHITECTURE.md (deployment section).
Now do Stage 4 of Phase 7.

Write deployment documentation covering Synology NAS, generic Docker, and bare metal.
This is a documentation stage — produce accurate, complete Markdown files.

Deliverables:

docs/DEPLOY.md — complete deployment guide:

  # Deployment Guide

  Three deployment paths: Synology NAS (Docker), Generic Docker + Caddy, Bare Metal.

  ## Prerequisites (all paths)
  - A domain name or static LAN IP
  - calibre-migrate run once to import your Calibre library (see Migration section)
  - A generated MEILI_MASTER_KEY: `openssl rand -hex 32`

  ## Path 1: Synology NAS (recommended for home users)

  ### Requirements
  - DSM 7.2+ with Container Manager installed
  - At least 2GB free RAM
  - Your Calibre library folder accessible on the NAS

  ### Steps
  1. Install Container Manager from the Synology Package Center
  2. Open Container Manager → Project → Create
  3. Set project path to a folder on your NAS (e.g. /docker/calibre-web-rs)
  4. Paste the contents of docker-compose.production.yml into the editor
  5. Set environment variables:
     - MEILI_MASTER_KEY: your generated key
     - GITHUB_REPOSITORY: your fork (or use the official image)
  6. Create a config.toml (see Configuration section below) and place it in the project folder
  7. Start the project. App is available at http://<NAS-IP>:8083
  8. Run calibre-migrate (see Migration section) to import your library
  9. For HTTPS: enable Synology's built-in reverse proxy at Control Panel → Login Portal →
     Advanced → Reverse Proxy, point yourdomain.com → localhost:8083

  ### Upgrading on Synology
  Container Manager → Project → your project → Update (pulls latest image, restarts).
  No data is lost — all data lives in named volumes.

  ## Path 2: Generic Docker + Caddy (VPS or home server)

  ### Steps
  1. Clone the repo or download docker-compose.production.yml
  2. Set APP_DOMAIN in your shell: `export APP_DOMAIN=library.yourdomain.com`
  3. Edit docker/Caddyfile: replace `{$APP_DOMAIN:localhost}` with your domain
  4. `docker compose -f docker-compose.production.yml up -d`
  5. Caddy obtains a Let's Encrypt certificate automatically on first start
  6. Run calibre-migrate to import your library

  ## Path 3: Bare Metal (advanced)

  ### Requirements
  - Rust 1.77+ (`rustup update stable`)
  - SQLite 3.35+ (`sqlite3 --version`)
  - Optional: Meilisearch binary (`curl -L https://install.meilisearch.com | sh`)

  ### Steps
  1. `cargo build --release -p backend`
  2. Copy `target/release/backend` to your server
  3. Create `config.toml` (see Configuration)
  4. Run as a systemd service (see below)
  5. Put Caddy or nginx in front for HTTPS

  ### Systemd Service
  ```ini
  [Unit]
  Description=calibre-web-rs
  After=network.target

  [Service]
  Type=simple
  User=calibre
  WorkingDirectory=/opt/calibre-web-rs
  ExecStart=/opt/calibre-web-rs/backend
  Restart=on-failure
  Environment=RUST_LOG=warn
  Environment=APP_DATABASE__URL=sqlite:///opt/calibre-web-rs/storage/library.db

  [Install]
  WantedBy=multi-user.target
  ```

  ## Configuration Reference

  All settings go in `config.toml`. Environment variables with prefix `APP_` override
  file values (double underscore for nested keys: `APP_DATABASE__URL`).

  | Key | Default | Required | Description |
  |---|---|---|---|
  | app.base_url | — | Yes | Full URL the app is served at (e.g. https://library.example.com) |
  | app.storage_path | ./storage | No | Where book files and covers are stored |
  | database.url | sqlite://./library.db | No | SQLite path or MariaDB connection string |
  | auth.jwt_secret | auto-generated | No | Min 256-bit random value; auto-generated if blank |
  | auth.access_token_ttl_mins | 15 | No | JWT lifetime in minutes |
  | auth.refresh_token_ttl_days | 30 | No | Refresh token lifetime in days |
  | auth.max_login_attempts | 10 | No | Failed attempts before lockout |
  | llm.enabled | false | No | Enable LLM features |
  | llm.librarian.endpoint | — | If llm.enabled | LM Studio or Ollama base URL |
  | llm.librarian.model | auto | No | Model name; auto-discovered from /v1/models if blank |
  | limits.upload_max_bytes | 524288000 | No | Max upload size (default 500MB) |

  ## Migration (calibre-migrate)

  Run once after first install to import from an existing Calibre library:
  ```bash
  # Dry run first — shows what would be imported without writing
  ./calibre-migrate --calibre-db /path/to/metadata.db --dry-run

  # Import
  ./calibre-migrate --calibre-db /path/to/metadata.db \
    --storage-path /app/storage \
    --db-url sqlite:///app/storage/library.db
  ```
  Migration is idempotent — safe to re-run. Skips already-imported records.

  ## Backup and Restore

  ### What to back up
  - `library.db` — all metadata, users, reading progress, LLM job history
  - `storage/` — book files and cover images
  - `config.toml` — configuration (keep JWT secret safe)

  ### Backup
  ```bash
  sqlite3 library.db ".backup library.db.bak"   # hot backup — safe while running
  tar czf storage.tar.gz storage/
  ```

  ### Restore
  ```bash
  cp library.db.bak library.db
  tar xzf storage.tar.gz
  ```

  ### Upgrade procedure
  1. Back up library.db and storage/
  2. Pull new image: `docker compose -f docker-compose.production.yml pull`
  3. Restart: `docker compose -f docker-compose.production.yml up -d`
  4. Check logs: `docker compose logs app --tail=50`
  5. If migration fails: restore from backup and report the issue

When done, run:
  git diff --stat
```

**Paste output here → run `/engineering:deploy-checklist` → proceed if approved.**

---

## Review Checkpoints

| After stage | What to verify |
|---|---|
| Stage 1 | `docker buildx build --platform linux/amd64,linux/arm64` parses without error; production compose binds port to 127.0.0.1; MEILI_MASTER_KEY has no default value |
| Stage 2 | CORS origin is APP_BASE_URL not wildcard; CSP includes worker-src blob:; SSRF validator logs WARNING not hard-fail for private endpoints; cargo audit passes |
| Stage 3 | `cargo bench` runs without compile errors; PERFORMANCE.md documents Pi 4 targets; bench CI job has `continue-on-error: true` |
| Stage 4 | DEPLOY.md covers all three paths; backup section includes sqlite3 .backup command; env var table is complete |

## If Codex Gets Stuck or a Test Fails

```
The following is failing. Diagnose the root cause and fix it.
Do not work around it — fix the underlying issue.

[paste error output]
```

## Commit Sequence

```bash
# After Stage 1
git add -A && git commit -m "Phase 7 Stage 1: multi-arch Dockerfile, docker.yml CI, production compose"

# After Stage 2
git add -A && git commit -m "Phase 7 Stage 2: OWASP hardening — CORS, CSP, SSRF guard, audit log coverage"

# After Stage 3
git add -A && git commit -m "Phase 7 Stage 3: criterion benchmarks, PERFORMANCE.md, bench CI job"

# After Stage 4
git add -A && git commit -m "Phase 7 Stage 4: DEPLOY.md — Synology, Docker, bare metal, backup/restore"
```
