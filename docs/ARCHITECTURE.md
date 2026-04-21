# calibre-web Rewrite — Architecture Document

_Status: Draft — decisions in progress_
_Last updated: 2026-04-20_

---

## Vision

A full rewrite of calibre-web in Rust, replacing the Python/Flask stack with a modern, performant, self-hosted ebook library system. The web browser is the primary UI target. Native mobile apps (iOS, Android) are a planned extension, designed in from the start. The system must run on Raspberry Pi, NAS hardware, standard computers, and in Docker — all from the same codebase.

---

## Decisions Log

| # | Decision | Status | Notes |
|---|---|---|---|
| 1 | Calibre DB: read-only first, migrate later | ✅ Decided | Migration tool is a first-class feature |
| 2 | Multi-user support | ✅ Decided | Full auth layer required |
| 3 | OPDS catalog support | ✅ Decided | Out of scope |
| 4 | Cover/thumbnail pipeline at ingest time | ✅ Decided | Not on-request |
| 5 | SQLx migrations from day one | ✅ Decided | Never alter schema manually |
| 6 | Primary UI target: web browser | ✅ Decided | Browser-based; Axum serves the SPA as static files |
| 7 | Synology NAS deployment: migrate, not mount | ✅ Decided | `calibre-migrate` is a first-class CLI tool |
| A | Database engine | ✅ Decided | SQLite (default) + MariaDB (optional) — same codebase, config-driven |

---

## Open Decisions

| # | Question | Options | Notes |
|---|---|---|---|
| B | Book file storage | ✅ Decided | Filesystem (default) via `StorageBackend` trait — S3-compatible as future option |
| C | LLM endpoints | ✅ Decided | Fully configurable per role — endpoint, model, timeout, system prompt |
| D | Mobile offline depth | ✅ Decided | Read-only cache + reading progress sync; `last_modified` on all mutable entities |
| E | Admin UI | ✅ Decided | Same SPA, role-gated at `/admin/*`; upload permissions configurable per role |
| F | Initial scope | ✅ Decided | See v1.0 scope table below |
| G | React framework | ✅ Decided | Vite SPA + TanStack Router + vite-plugin-pwa — no Node.js server in production |

---

## v1.0 Scope

> A self-hosted web app that browses, reads, searches, and manages a book library with multi-user support, single/bulk upload, Calibre migration, and optional LLM classification — running in Docker on any hardware.

### Core Features

| Feature | v1.0 | Notes |
|---|---|---|
| Browse library (grid + list) | ✅ Must | |
| Book detail page | ✅ Must | |
| Read epub in browser | ✅ Must | epub.js |
| Read PDF in browser | ✅ Must | react-pdf |
| Read CBZ/comics | ✅ Should | |
| Download book file | ✅ Must | |
| Basic search | ✅ Must | Title, author, tag |
| Metadata editing (single book) | ✅ Must | |
| Cover management | ✅ Must | Upload + auto-extract |
| Multi-user + roles | ✅ Must | |
| Single book upload | ✅ Must | |
| Bulk import | ✅ Should | Admin quality-of-life |
| Reading progress (web) | ✅ Must | |
| Reading progress (mobile sync) | ✅ Should | Already designed |
| Shelves / reading lists | ✅ Should | Personal curation |
| Bulk metadata edit | ✅ Should | Needed after bulk import |
| Send to Kindle / email | ⏳ Phase 2 | Low priority for local-first |
| Book conversion (Calibre engine) | ⏳ Revisit | Detect via `CALIBRE_BINARY` env var; enable only when present |

### LLM Features

| Feature | v1.0 | Notes |
|---|---|---|
| `calibre-migrate` CLI | ✅ Must | No one can switch without it |
| Prompt eval framework | ✅ Must | Required if any LLM ships in v1 |
| LLM classification + tagging | ✅ Should | Core differentiator |
| Semantic search | ✅ Should | Core differentiator |
| **Text extraction API** | ✅ Should | Foundational for agentic RAG — chapter-level EPUB/PDF text |
| Metadata validation | ⏳ Phase 2 | Useful but not blocking |
| Content quality check | ⏳ Phase 2 | Useful but not blocking |
| Library organization rules | ⏳ Phase 2 | |
| Derived works | ⏳ Phase 2 | Complex, niche |

### Platform

| Target | v1.0 | Notes |
|---|---|---|
| Web browser | ✅ Must | Primary target |
| Docker (amd64) | ✅ Must | |
| Docker (arm64 / Pi) | ✅ Must | |
| Synology NAS | ✅ Must | Docker Compose + migrate |
| iOS app | ⏳ Phase 2 | |
| Android app | ⏳ Phase 2 | |

---

## Technology Stack

### Backend

| Layer | Technology | Rationale |
|---|---|---|
| Language | **Rust** | Performance, memory safety, single binary, ARM support |
| Web framework | **Axum** | Async, composable, tower ecosystem, strong community |
| Database | **SQLite** (default) or **MariaDB** (config-driven) | SQLite for Pi/NAS/single-user; MariaDB for larger or multi-instance deployments |
| Database ORM | **sqlx** | Compile-time query checking, async, supports both SQLite and MariaDB |
| Search | **Meilisearch** | Rust-native, single binary, low RAM, full-text + typo tolerance |
| Semantic search | **sqlite-vec** | SQLite extension for vector storage — no separate vector DB |
| File serving | **tower-http ServeFile** | Native HTTP range request support (streaming for large files) |
| Auth tokens | **JWT + refresh tokens** | Short-lived access + long-lived refresh, stored in DB |
| Password hashing | **argon2** | Industry standard, Rust-native |
| File storage | **LocalFs** (`StorageBackend` trait) | Filesystem default; trait allows S3-compatible backend in future |
| Image processing | **image crate** | Cover resizing, thumbnail generation at ingest |
| LLM client | **reqwest** | Async HTTP, replaces Python `requests` — same LM Studio API |
| LLM config | **TOML config file** | Per-role: endpoint, model (auto-discover if blank), timeout, system prompt |
| Input validation | **`validator`** crate | API input validation before DB access |
| Rate limiting | **`tower-governor`** | Per-IP and per-user rate limits on auth and LLM routes |
| Security headers | **`tower-http`** middleware | CSP, X-Frame-Options, Referrer-Policy, etc. on all responses |
| Job queue | **SQLite-backed queue** | Long-running LLM tasks (classify library, reindex) run async |

### Frontend — Web

| Layer | Technology | Rationale |
|---|---|---|
| Framework | **React** | Shared component logic with mobile via React Native |
| Build tool | **Vite** | Fast HMR, static output served directly by Axum — no Node.js in production |
| Router | **TanStack Router** | File-based, fully type-safe, integrates cleanly with TanStack Query |
| PWA | **vite-plugin-pwa** | Service worker for offline browsing — bridges gap before native mobile apps |
| State — server | **TanStack Query** | Caching, background refetch, optimistic updates |
| State — local UI | **Zustand** | Reader position, sidebar, preferences |
| Styling | **Tailwind CSS + shadcn/ui** | Unstyled components you own, consistent design system |
| Epub reader | **epub.js** | Mature, handles reflowable + fixed layout |
| PDF reader | **react-pdf** | PDF rendering in browser |

### Frontend — Mobile

| Layer | Technology | Rationale |
|---|---|---|
| Framework | **Expo (React Native)** | iOS + Android from one codebase, Expo Router for navigation |
| Styling | **NativeWind** | Tailwind syntax for React Native — consistent with web layer |
| Local storage | **Expo SQLite** | Mirrors library subset for offline use |
| File storage | **expo-file-system** | Downloaded books for offline reading |
| Secure storage | **Expo SecureStore** | JWT tokens in Keychain (iOS) / Keystore (Android) |
| Epub reader | **foliojs-port** | React Native epub rendering |
| PDF reader | **expo-pdf** | Native PDF rendering |

### Shared (Web + Mobile)

| Layer | Technology |
|---|---|
| Language | TypeScript |
| API client | `packages/shared/api/` — fetch wrappers for all Axum routes |
| Types | `packages/shared/types/` — Book, Author, Tag, SearchResult, User, etc. |
| Hooks | `packages/shared/hooks/` — useBooks, useSearch, useLLM, useReader |

---

## Repository Structure

```
calibre-web-rs/
├── Cargo.toml                  # Rust workspace root
├── package.json                # pnpm workspace root
├── turbo.json                  # Turborepo pipeline
│
├── backend/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs
│   │   ├── api/               # Axum route handlers
│   │   │   ├── auth.rs
│   │   │   ├── books.rs
│   │   │   ├── search.rs
│   │   │   ├── llm.rs
│   │   │   └── admin.rs
│   │   ├── db/                # sqlx models + queries
│   │   ├── llm/               # LM Studio client, job queue
│   │   ├── ingest/            # File parsing, cover extraction
│   │   └── migrate/           # calibre-migrate CLI tool
│   └── migrations/            # sqlx migration files
│
├── packages/
│   └── shared/                # TypeScript — shared by web + mobile
│       ├── api/
│       ├── hooks/
│       └── types/
│
├── apps/
│   ├── web/                   # React SPA (served by Axum as static files)
│   └── mobile/                # Expo app (iOS + Android)
│
├── evals/
│   ├── fixtures/              # TOML fixture files — one per test case
│   └── results/               # Local result cache (also stored in DB)
│
├── docker/
│   ├── Dockerfile             # Multi-stage: Rust build + static frontend
│   ├── docker-compose.yml     # Full stack: app + Meilisearch + optional Caddy
│   └── Caddyfile              # Caddy reverse proxy config (HTTPS via Let's Encrypt)
│
└── docs/
    ├── ARCHITECTURE.md        # This file
    ├── DECISIONS.md           # Expanded decision log
    └── SCHEMA.md              # Database schema (TBD)
```

---

## Deployment Targets

| Target | Approach |
|---|---|
| Raspberry Pi / ARM NAS | Docker Compose — `linux/arm64` and `linux/arm/v7` builds |
| Synology NAS | Docker Compose — run `calibre-migrate` once to import from Calibre |
| Mac / Windows / Linux | Docker or run Axum binary directly, open browser |
| iOS | Expo EAS Build → App Store |
| Android | Expo EAS Build → Play Store / sideload |

### Docker Image Strategy

- Multi-stage build: Stage 1 compiles Rust + builds frontend; Stage 2 is minimal runtime
- Target image size: ~20–40MB (vs ~500MB+ for Python)
- `docker-compose.yml` brings up: app container + Meilisearch + optional Caddy container
- Calibre library directory mounted read-only during migration; app owns storage after
- Axum binds to `127.0.0.1` by default — not reachable directly from outside; Caddy or nginx sits in front

### HTTPS / Reverse Proxy

The app does not handle TLS directly — a reverse proxy terminates HTTPS.

**Recommended: Caddy** — automatic HTTPS via Let's Encrypt, zero config, single binary.

```
# Caddyfile (minimal)
library.yourdomain.com {
    reverse_proxy app:8083
}
```

Docker Compose ships with an optional Caddy service — uncomment to enable. LAN-only users leave it commented out and access over HTTP on the local network.

nginx is supported as an alternative for users who already run it on their NAS.

The `Secure` cookie flag is set automatically when `APP_BASE_URL` starts with `https://`.

---

## Authentication

- Local user accounts stored in app DB (not Calibre's user table)
- Passwords hashed with **argon2** — work factor configurable in `config.toml`
- Argon2 defaults: memory `65536 KiB`, iterations `3`, parallelism `4` (minimum enforced at startup)
- JWT access tokens (15 min TTL) + refresh tokens (30 day TTL, stored in DB)
- Web: httpOnly cookies (not localStorage)
- Mobile: Expo SecureStore
- Roles: Admin, User (extensible — OIDC/LDAP noted for future, not in scope)
- Upload permission is configurable per role — Admin always can; User upload is toggled in role config
- **Account lockout** after N failed logins (default 10, configurable) — resets after 15 min
- **Session list** in user profile — view and revoke active sessions

---

## Security

### HTTP Security Headers

Set by Axum middleware on every response:

| Header | Value |
|---|---|
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `DENY` |
| `Referrer-Policy` | `strict-origin-when-cross-origin` |
| `Content-Security-Policy` | `default-src 'self'` (tuned for epub.js inline script needs) |
| `Permissions-Policy` | Disable camera, microphone, geolocation |

### Input Validation
- All API inputs validated with the **`validator`** crate before touching the DB
- File uploads: magic byte detection (not just extension) — reject files that don't match claimed format
- Upload size limit configurable in `config.toml` (default 500MB)
- Book file paths stored relative to storage root — `../` stripped at ingest to prevent path traversal

### SQL Injection
- sqlx compile-time checked queries with bound parameters — no string interpolation into SQL

### File Serving
- Book files served from a dedicated storage directory outside the web root
- Axum validates every requested path is within the storage root before serving
- Directory listing disabled

### Rate Limiting
- Auth endpoints (`/auth/login`, `/auth/refresh`): 10 req/min per IP — via `tower-governor`
- LLM classify/validate/quality/derive: 30 req/min per user
- Bulk import / migration: 1 concurrent job per admin
- Global fallback: 200 req/min per IP (configurable)

### Secrets
- `config.toml` holds sensitive values (DB credentials, JWT secret)
- JWT secret: minimum 256-bit random value — generated automatically on first run if not set
- Config file permissions checked on startup — warning logged if world-readable
- Docker: secrets passed via environment variables, not baked into the image

### Dependency Auditing
- `cargo audit` runs in CI on every push — blocks merge on known CVEs
- Equivalent of the current Python `pip-audit` workflow, automated

### Not in v1.0 Scope
- 2FA / TOTP (Phase 2 — straightforward to add post-launch)
- WAF (out of scope for a personal self-hosted app)

---

## Admin UI

Lives at `/admin/*` in the same SPA. All routes server-side guarded by `Admin` role — frontend gate is UX only.

| Section | Content |
|---|---|
| Users | Create, edit, delete users; assign roles; force password reset; configure upload permission per role |
| LLM Config | Edit endpoints, models, timeouts, system prompts per role |
| Prompt Evals | Run eval suite, view model compatibility matrix, promote prompt versions |
| Migration | Run `calibre-migrate`, view import log, re-run failed records |
| Jobs | LLM job queue — pending, running, failed, completed |
| System | App version, DB stats, storage usage, Meilisearch status |

---

## Resource Management (Books)

### Upload Permissions

Upload capability is configurable per role in admin. Admin role always has full access.

### Single Book Upload

Available to any role with upload permission enabled:

- Drag-and-drop or file picker — epub, PDF, mobi, CBZ, and all calibre-web supported formats
- Auto-extract metadata from file (title, author, ISBN, cover, description)
- Manual metadata entry form pre-filled from extraction — user corrects before saving
- LLM-assisted classification runs automatically on ingest if `llm.enabled = true` — tags and genre suggested, user confirms before saving
- Duplicate detection — warns if ISBN or title+author combination already exists, user decides whether to proceed

### Bulk Import

Admin role only:

- Upload a zip archive or point to a local folder path (server-side)
- All files processed through the same ingest pipeline as single upload
- Metadata extraction runs per file; failures flagged for manual review
- LLM classification queued as background jobs (job queue) — does not block import completion
- Import report on completion: succeeded / failed / duplicates / queued for review

### Bulk Metadata Edit

Available to any role with upload permission:

- Select multiple books from library grid
- Apply shared fields to all: genre, tags, series, language, reading level
- Per-field choice: overwrite existing / append / skip if already set
- LLM re-classification available as a bulk action (queued as background jobs)

### Ingest Pipeline (server-side, all formats)

```
File received
    → Format detection (magic bytes)
    → Metadata extraction (title, author, ISBN, cover, description)
    → Cover resize + thumbnail generation (image crate)
    → Duplicate check (ISBN + title/author)
    → Document type classification (LLM if enabled → 'unknown' fallback)
    → Write to DB + StorageBackend
    → Meilisearch index update
    → LLM tag classification job queued (if enabled)
    → Response to client
```

**Document type** is set synchronously at ingest from a single fast LLM call (separate from tag classification, which is queued async). Valid values: `novel`, `textbook`, `reference`, `magazine`, `datasheet`, `comic`, `unknown`. When LLM is disabled, all books ingest as `unknown` and can be reclassified later. Document type becomes a first-class filter for RAG agents — an agent can scope retrieval to `document_type = 'textbook'` before running semantic search.

---

## Agentic RAG Integration

calibre-web-rs is designed to function as a **tool provider** for external agentic AI systems (LangGraph, smolagents, custom agents), in addition to its primary role as a library management UI.

### The Two-Tier Retrieval Model

An agent orchestrating RAG against this library has two distinct retrieval surfaces:

| Tier | Mechanism | Use case |
|---|---|---|
| **Structured** | REST API → SQLite metadata | Author lookup, tag filter, series navigation, exact match |
| **Semantic** | REST API → sqlite-vec embeddings | Meaning-based similarity, concept search across content |

The metadata tier is always faster and scopes the corpus before semantic search runs — always filter by structured metadata first to reduce hallucination surface.

### Agent Tool Surface

The following routes are designed to be consumed directly as agent tools. They are available to any authenticated client, including external agent frameworks:

| Tool | Route | Description |
|---|---|---|
| `search_books` | `GET /api/v1/books?q=&author=&tags=` | Structured metadata query with filters |
| `get_book_metadata` | `GET /api/v1/books/:id` | Full metadata record including authors, tags, series |
| `list_chapters` | `GET /api/v1/books/:id/chapters` | Table of contents: chapter indices + titles + word counts |
| `get_book_text` | `GET /api/v1/books/:id/text?chapter=N` | Extracted plain text — full book or one chapter |
| `semantic_search` | `GET /api/v1/search?q=&mode=semantic` | Vector similarity search across embedded chunks |

### Text Extraction Pipeline

Exposing book content as plain text is the foundational capability for RAG. EPUB and PDF formats are extracted server-side:

```
EPUB: unzip → parse OPF manifest → identify spine items (chapters)
      → extract HTML per chapter → strip tags → normalize whitespace → return clean text

PDF:  parse structure → extract text per page → group into logical sections → return clean text
```

**Chunking strategy:**
- EPUB: OPF spine items are the natural chunk boundary — each spine item is one chapter
- PDF: page groups (default 5 pages) when no logical chapter structure is present
- `?chapter=N` requests a single chunk; omitting returns all chapters concatenated with `\n\n---\n\n`

**Text extraction is not gated behind `llm.enabled`** — it is a content API, available whenever the server is running. No LLM is involved in extraction.

### How Classification Enriches RAG

The LLM features (Phase 5) represent calibre-web-rs calling the LLM. The agentic RAG surface represents an external agent calling calibre-web-rs. These are complementary:

- LLM classification enriches metadata → better structured retrieval tier for agents
- Text extraction enables agents to retrieve actual book content for synthesis
- Semantic search (Phase 4) provides the vector similarity tier

A typical agentic query: user question → agent calls `search_books` (filter by author/tag) → agent calls `get_book_text?chapter=N` on matching books → agent synthesizes across retrieved passages.

---

## LLM Integration (Graceful Degradation)

Carried forward from current Python implementation. All constraints unchanged:

- Controlled by `ENABLE_LLM_FEATURES` config flag (default: `false`)
- All LLM calls: 10-second timeout, silent fallback — no errors surfaced to users
- `Option<LlmClient>` in Axum state — `None` when disabled; all routes check before calling
- Fully configurable via `config.toml` — no hardcoded endpoints or model names
- Per-role config: endpoint, model (auto-discover if blank), timeout, system prompt
- System prompts tunable without code changes — users can adjust LLM behavior for their library/use case
- Model auto-discovery via `/v1/models` when `model = ""`

```toml
[llm]
enabled = false

[llm.librarian]
endpoint = "http://192.168.0.72:1234/v1"
model = ""                          # auto-discover if empty
timeout_secs = 10
system_prompt = """
You are a librarian assistant. Classify books accurately by genre, subject, and reading level.
Be concise. Return structured data only.
"""

[llm.architect]
endpoint = "http://localhost:1234/v1"
model = ""
timeout_secs = 10
system_prompt = """
You are a library metadata expert. Validate and enrich book metadata.
Flag issues clearly. Never invent data.
"""
```
- Long-running jobs (classify library, semantic reindex) dispatched to SQLite job queue

### System Prompt Evaluation Framework

Trial-and-error prompt tuning is replaced by a structured eval system. Prompts are tested against known fixtures before being used in production, with per-model pass/fail results stored and queryable.

#### How It Works

1. Write a fixture: input + expected output criteria
2. Run `calibre eval` against one or more models
3. Results recorded in DB — pass/fail per prompt version per model
4. Promote a prompt version to active only after it passes

#### Fixture Format (`evals/fixtures/`)

Each fixture is a TOML file:

```toml
# evals/fixtures/classify_fiction.toml
name = "classify_fiction"
role = "librarian"
description = "Should classify a clearly fictional novel correctly"

[input]
title = "The Great Gatsby"
author = "F. Scott Fitzgerald"
description = "A story of the fabulously wealthy Jay Gatsby and his love for Daisy Buchanan."

[[expect]]
type = "json_valid"                     # response must parse as JSON

[[expect]]
type = "contains_field"
field = "genre"                         # response JSON must have a "genre" key

[[expect]]
type = "field_matches"
field = "genre"
pattern = "(?i)fiction|literary"        # case-insensitive regex

[[expect]]
type = "array_min_length"
field = "tags"
min = 3                                 # at least 3 tags returned

[[expect]]
type = "latency_ms"
max = 10000                             # must respond within timeout
```

#### Evaluator Types

| Type | Description |
|---|---|
| `json_valid` | Response parses as valid JSON |
| `contains_field` | JSON response contains a required key |
| `field_matches` | Field value matches a regex pattern |
| `array_min_length` | Array field has at least N items |
| `not_contains` | Response must not contain a string (hallucination guard) |
| `latency_ms` | Round-trip must complete within N ms |
| `llm_judge` | Secondary LLM call scores the response (0–1) against a rubric |

#### CLI Usage

```bash
# Run all fixtures against the configured librarian model
calibre eval --role librarian

# Run against a specific model (overrides config)
calibre eval --role librarian --model phi-3-mini-4k-instruct

# Run against multiple models and compare
calibre eval --role librarian --model phi-3-mini --model llama-3.2-3b

# Run a single fixture
calibre eval --fixture classify_fiction

# Show stored results for a model
calibre eval --results --model phi-3-mini-4k-instruct
```

#### Results Storage

Results are stored in the app DB (`llm_eval_results` table):

| Column | Type | Notes |
|---|---|---|
| `id` | uuid | |
| `fixture_name` | text | Links to fixture file |
| `model_id` | text | Exact model string from `/v1/models` |
| `prompt_hash` | text | SHA256 of system prompt — tracks prompt versions |
| `passed` | bool | Overall pass/fail |
| `results_json` | json | Per-evaluator pass/fail + details |
| `latency_ms` | int | Actual response time |
| `run_at` | timestamp | |

#### Model Compatibility Matrix

The admin UI shows a live matrix of fixture results per model — immediately shows which models pass which tasks:

```
Fixture                    │ phi-3-mini │ llama-3.2 │ gemma-3 │
───────────────────────────┼────────────┼───────────┼─────────┤
classify_fiction           │ ✅ PASS    │ ✅ PASS   │ ✅ PASS │
classify_technical         │ ✅ PASS    │ ❌ FAIL   │ ✅ PASS │
validate_metadata_isbn     │ ❌ FAIL    │ ✅ PASS   │ ✅ PASS │
semantic_search_relevance  │ ✅ PASS    │ ✅ PASS   │ ❌ FAIL │
```

#### Prompt Versioning

- Each unique system prompt gets a SHA256 hash stored alongside results
- Changing a prompt in `config.toml` produces a new hash — old results preserved for comparison
- Admin UI shows result history per prompt version per model
- `calibre eval --promote` marks a prompt version as active after it passes all fixtures

### LLM Features (all from current implementation)

| Feature | Route | LLM Required |
|---|---|---|
| Health check | `GET /api/v1/llm/health` | No |
| Library organization | `POST /api/v1/organize` | Yes |
| Classification & tagging | `GET /api/v1/books/:id/classify` | Yes |
| Metadata validation | `GET /api/v1/books/:id/validate` | Yes |
| Content quality check | `GET /api/v1/books/:id/quality` | Yes |
| Semantic search | `GET /api/v1/search?mode=semantic` | Embedding only |
| Derived works | `GET /api/v1/books/:id/derive` | Yes |
| **Chapter listing** | `GET /api/v1/books/:id/chapters` | **No — content API** |
| **Text extraction** | `GET /api/v1/books/:id/text?chapter=N` | **No — content API** |

---

## Migration Tool (`calibre-migrate`)

First-class CLI binary, not a script. Reads Calibre's SQLite DB (read-only) and imports into the new schema.

**Scope:**
- Books, authors, series, tags, identifiers, formats, covers
- User accounts (Calibre Web users — passwords cannot be migrated, must be reset)
- Reading progress (if stored in Calibre Web DB)
- Custom columns (flagged for manual review — schema varies)

**Approach:**
- `--dry-run` mode reports what would be imported without writing
- Idempotent — safe to re-run, skips already-imported records
- Progress output to stdout; errors logged to file
- Run once post-install; Calibre DB untouched throughout

---

## Phased Build Plan

### Phase 1 — Foundation (Backend) ✅ Complete
- [x] Cargo workspace + sqlx setup
- [x] Initial schema design + migrations
- [x] Auth routes (register, login, refresh, logout)
- [x] Books CRUD API (read from new DB)
- [x] File serving with range request support
- [x] Cover pipeline (ingest → resize → store)
- [x] HTTP security headers middleware
- [x] Account lockout logic
- [x] Config file permission check on startup
- [x] `cargo audit` in CI
- [x] Docker build + docker-compose + Caddyfile

### Phase 2 — Migration ✅ Complete
- [x] `calibre-migrate` CLI: books, authors, tags, covers
- [x] Dry-run mode + idempotency
- [x] Validation report output

### Phase 3 — Web Frontend ✅ Complete
- [x] Monorepo setup (pnpm + Turborepo)
- [x] `packages/shared` — types + API client
- [x] React SPA: library grid, book detail, search
- [x] Auth UI (login, user management for admins)
- [x] Epub reader integration (epub.js)
- [x] PDF reader integration
- [x] Admin panel

### Phase 4 — Search ✅ Complete
- [x] FTS5 full-text search (SQLite virtual table + sync triggers)
- [x] Meilisearch optional tier with graceful fallback to FTS5
- [x] LLM semantic search (sqlite-vec embeddings, gated behind llm.enabled)
- [x] Frontend search wiring (SearchPage, SearchBar, semantic tab)

### Phase 5 — LLM Features + Agentic RAG Surface ✅ Complete
- [x] ChatClient + classify pipeline + confirm/reject API (4/4 tests)
- [x] validate / quality / derive / organize routes (13/13 tests)
- [x] Text extraction API — `GET /books/:id/chapters` and `GET /books/:id/text?chapter=N`
- [x] Document type classification at ingest (novel/textbook/reference/magazine/datasheet/comic/unknown)
- [x] Job runner extended for classify and organize job types
- [x] Admin Jobs API — list/detail/cancel (5/5 tests)
- [x] Frontend AI panel on BookDetailPage (Classify/Validate/Derive tabs)
- [x] AdminJobsPage with real-time polling

### Phase 6 — Mobile
- [ ] Expo project setup + Expo Router + NativeWind
- [ ] Auth screens + SecureStore token handling
- [ ] Library browse (grid) + book detail screen
- [ ] Offline: Expo SQLite sync + expo-file-system downloads
- [ ] EPUB reader (foliojs-port) + PDF reader (expo-pdf)
- [ ] Reading progress tracking + server sync
- [ ] Expo EAS build configuration (iOS + Android)

### Phase 7 — Hardening
- [ ] Multi-architecture Docker builds (amd64, arm64, armv7)
- [ ] Synology deployment documentation
- [ ] Performance testing on Raspberry Pi 4/5
- [ ] Security audit (OWASP top 10 review)

### Phase 8 — MCP Server
Expose the library as a first-class tool provider for external agentic AI systems. The REST API built in Phases 1–5 is the foundation; Phase 8 adds an MCP transport layer on top of the already-stable routes.

- [ ] Implement MCP server in Rust (stdio + SSE transports) alongside the existing Axum server
- [ ] Expose agent tool surface as MCP tools:
  - `search_books(query, filters)` — wraps `GET /api/v1/books`
  - `get_book_metadata(book_id)` — wraps `GET /api/v1/books/:id`
  - `list_chapters(book_id)` — wraps `GET /api/v1/books/:id/chapters`
  - `get_book_text(book_id, chapter?)` — wraps `GET /api/v1/books/:id/text`
  - `semantic_search(query)` — wraps `GET /api/v1/search?mode=semantic`
- [ ] Auth: MCP tools require a configured API token (separate from JWT — long-lived, admin-generated)
- [ ] Register MCP server in `claude mcp add` for Claude Code + Claude Desktop integration
- [ ] Documentation: how to connect LangGraph, smolagents, and Claude Desktop to the library

---

## Notes & Constraints

- Do not break Calibre DB compatibility during read-only phase — never write to it
- shadcn/ui components are copied into the repo (not a runtime dependency) — own your UI
- NativeWind must stay in sync with Tailwind version used in web app
- All API responses include `Content-Type: application/json` and proper HTTP status codes
- Meilisearch is optional — app degrades to SQLite FTS5 if unavailable (same pattern as LLM)
- No telemetry, no analytics, no external calls except LM Studio endpoints
