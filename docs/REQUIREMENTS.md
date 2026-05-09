# calibre-web AI Agent — Project Requirements

## Hardware & Infrastructure

- **Host machine**: M4 Mac Studio, 128GB RAM, macOS
- **Local LM Studio** (`localhost:1234`): Development and runtime models
- **Remote LM Studio** (`192.168.0.72:1234`): Librarian model (Phi-3-mini)

## Models

| Role | Model | Endpoint | Purpose |
|---|---|---|---|
| Architect / Reviewer | `nvidia/nemotron-3-super` (120B GGUF) | localhost:1234 | Plans tasks, reviews code |
| Coder | `qwen2.5-coder-7b-instruct-mlx` (7B MLX) | localhost:1234 | Executes coding tasks |
| Librarian (runtime) | `phi-3-mini-4k-instruct` (3.8B) | 192.168.0.72:1234 | Classification, tagging, metadata |

## Source Project

- **Project**: [calibre-web](https://github.com/janeczku/calibre-web) (open source, Python/Flask)
- **Local repo**: `~/Documents/localProject/calibre-web`
- **Active branch**: `lmstudio-agent`
- **Base branch**: `master`

## Agentic Development Tooling

- **opencode** CLI — primary interface; configured to use LM Studio via OpenAI-compatible API
- **orchestrator** (`tools/orchestrator.py`) — two-model loop: Nemotron plans → Qwen codes → Nemotron reviews
- **MCP server** (`tools/mcp_server.js`) — exposes orchestrator to opencode as tools (`orchestrate`, `plan`, `code_task`, `review`)

---

## Feature Requirements

### Prerequisite
- Code audit to establish baseline before any features are added (✅ Complete)

### Feature 1 — Library Organization
- Automatically organize the library by genre, author, series, date, or custom rules
- Supports all calibre-web file types
- Non-destructive — original files preserved

### Feature 2 — Book & Document Ingestion
- Ingest new books and documents into the library
- Auto-detect file type and extract metadata on import
- Supports all calibre-web supported formats

### Feature 3 — Metadata Validation (Internet)
- Validate and enrich metadata against internet sources (Open Library, Google Books, ISBN DBs)
- Flag mismatches for review
- Never overwrite without confirmation

### Feature 4 — Content Quality Validation
- Detect page order issues (pages out of sequence)
- Detect missing pages (gaps in pagination)
- Detect garbled text and OCR artifacts
- Report findings per book — do not auto-correct

### Feature 5 — Classification & Tagging
- Auto-classify books by subject, genre, reading level, topic
- Auto-generate tags from content
- All classification via Librarian model (Phi-3-mini @ 192.168.0.72:1234)
- Tags are suggestions — user confirms before saving

### Feature 6 — Graceful Degradation
- **All AI features must be optional**
- If LM Studio is unreachable (either endpoint), AI options are greyed out in the UI — no errors, no warnings visible to user
- Controlled by config flag `ENABLE_LLM_FEATURES` (default: `False`)
- All LLM calls must have a **10-second timeout** with silent fallback

### Feature 7 — Extensive Semantic Search
- Search across metadata, full text, and semantic similarity
- Results ranked by relevance
- Falls back to standard text search if LLM unavailable

### Feature 8 — Derived Works
- Create derivative documents from existing library content
- Copy large areas of text from one or more sources
- Include snippets from other texts to create composite works
- **Local only** — not published, not exported outside the system
- Copyright note: derivatives are personal/local use only

---

## Implementation Constraints (All Features)

- Do not modify files in `test/`
- Follow existing Flask blueprint and SQLAlchemy patterns in `cps/`
- Add config flag `ENABLE_LLM_FEATURES` (default `False`) in `cps/config.py`
- All LLM calls: 10-second timeout, silent fallback on failure
- UI must degrade gracefully — no LLM errors exposed to end users
- Each feature on its own `feature/<name>` git branch

## Implementation Order (from Phase 1A)

1. Graceful degradation infrastructure (Low) — must be first; all others depend on it
2. Library organization (Low)
3. Book/document ingestion (Low)
4. Classification and tagging (Med)
5. Metadata validation from internet (Med)
6. Page order / OCR artifact detection (Med)
7. Extensive semantic search (High)
8. Derived works (High)

---

## calibre-web Parity Checklist

Tracks feature parity between xcalibre-server and calibre-web (Python/Flask). Full gap analysis in `GAP.md`.

### Auth & User Management
- [x] Username + password login (Argon2id + JWT)
- [x] Persistent session (refresh token)
- [x] Magic link login (Phase 19)
- [x] TOTP 2FA + backup codes (Phase 17)
- [x] OAuth — Google + GitHub (Phase 9)
- [x] OAuth account linking/unlinking (Phase 26)
- [x] LDAP authentication (Phase 9)
- [x] API tokens — scoped + expiring (Phase 17)
- [x] Account lockout + brute-force protection (Phase 1)
- [x] Per-user roles (admin/user) (Phase 1)
- [ ] LDAP bulk user import
- [ ] Remote login token flow (replaced by magic links — intentional)

### Library Browsing
- [x] Browse by author, publisher, series, rating, format, language, tag (Phases 9–10)
- [x] Search (FTS5, semantic, hybrid) (Phase 4/15)
- [x] Advanced search with query params (Phase 1)
- [x] Download history (Phase 10)
- [x] Read/unread state per user (Phase 10)
- [x] Reading statistics + streaks (Phase 11)
- [ ] Table view + column preferences
- [ ] Hot/trending browse (non-OPDS)

### Book Management
- [x] Upload (single + bulk) (Phase 1/9)
- [x] Cover upload + auto-extract (Phase 1)
- [x] Metadata editing (single + bulk) (Phase 1/9)
- [x] Delete book / delete specific format (Phase 1)
- [x] Multiple formats per book (Phase 9)
- [x] Book merge + simulation/preview (Phase 27)
- [x] Inline serve without download (Phase 25)
- [ ] Format conversion (requires Calibre binary — out of scope)
- [ ] Sort-value endpoint
- [ ] Bulk display-status toggle

### Shelves
- [x] Create, list, add/remove, delete shelf (Phase 9)
- [x] Public shelves (Phase 9)
- [x] Shelf reordering (Phase 25)
- [ ] Shelf configuration/edit
- [ ] Simple/read-only shelf view

### OPDS
- [x] Root catalog + author/series/publisher/language/rating/tag feeds (Phases 9–10)
- [x] Cover serving (original + thumb + large) with LRU cache (Phase 23)
- [x] OpenSearch descriptor (Phase 23)
- [x] Letter-based browsing — authors + series (Phase 23)
- [x] Hot/trending feed (Phase 23)
- [x] New releases feed (Phase 23)
- [x] Stats + discover feeds (Phase 23)
- [x] Path-based search variant (Phase 23)

### Kobo Sync
- [x] Device registration, library sync, reading state, bookmarks, tags (Phase 9)
- [x] Mock store endpoints — firmware compatibility (Phase 24)
- [x] Kobo image proxy (Phase 24)

### Admin
- [x] User management API (Phase 1)
- [x] Scheduled tasks (Phase 10)
- [x] In-app update checker + apply (Phase 22)
- [x] Admin log viewer API (Phase 28)
- [x] Metadata backup (Phase 28)
- [x] Cover regeneration queue (Phase 28)
- [x] Task cancellation (Phase 28)
- [x] Domain allowlist/blocklist (Phase 28)
- [ ] Admin HTML UI (intentional — API-only)
- [ ] Log file download
- [ ] Scheduled task HTML form

### Metadata
- [x] Open Library + Google Books enrichment (Phase 9)
- [x] Enrichment result cache (Phase 9)
- [x] LLM-assisted classification + tagging (Phase 5)
- [ ] Runtime metadata provider switching (config.toml only)
