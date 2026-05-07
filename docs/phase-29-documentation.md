# Phase 29 — Documentation Pass

## Context
This phase is documentation-only (no Rust implementation).

Working dir: `~/Documents/localProject/xcalibre-server`

Scope: update project docs after Phases 23–28 are implemented.

---

## 1. Update `GAP.md`

Mark the following entries as `✅` in status columns and priority checklist sections:
- OPDS cover serving
- OPDS OSD
- OPDS letter browsing
- OPDS hot/new/stats/discover
- Kobo mock store endpoints
- Shelf reordering
- Inline serve endpoint
- OAuth account linking/unlinking
- Book merge + preview
- Admin log viewer
- Metadata backup endpoint
- Thumbnail regeneration endpoint
- Task cancellation endpoint
- Domain allowlist/blocklist management

Ensure the route summary and high/medium/low gap lists are updated to reflect closed items.

---

## 2. Update `docs/ARCHITECTURE.md`

Add or revise sections:
- OPDS section:
  - `/opds/cover/:book_id` + variants
  - `/opds/osd`
  - `/opds/new`, `/opds/hot`, `/opds/stats`, `/opds/discover`
  - `/opds/authors/letter/:char`, `/opds/series/letter/:char`
- Auth section:
  - post-login OAuth link flow
  - signed state (`user_id + nonce + timestamp`, HMAC verified)
  - unlink lockout guard
- Admin section:
  - `/api/v1/admin/logs` and `log.file`
  - `/api/v1/admin/backup` and `backup.dir`
  - `/api/v1/admin/domains*` registration enforcement behavior
- Background tasks section:
  - `TaskKind::CoverRegenerate`
  - cancellation protocol (`tasks.status = cancelled` polling)

Keep versioning and phase status metadata consistent with current phase numbering.

---

## 3. Update `docs/REQUIREMENTS.md`

In calibre-web parity checklist:
- Check off every item closed by Phases 23–28.
- Add newly surfaced parity tasks if discovered during implementation.

Document any remaining parity gaps explicitly.

---

## 4. Update `docs/CHANGELOG.md`

Add release entry:

`## [2.4.0] — 2026-05-07`

Include bullets for:
- OPDS enhancements
- Kobo mock store compatibility endpoints
- Shelf reorder + inline serve
- OAuth link/unlink
- Book merge preview + merge execution
- Admin log/backup/cover-regenerate/task-cancel/domain-management additions
- Documentation consolidation for parity closure

---

## 5. Update `TEST_PHASES.md`

Add rows:
- `test_opds_enhancements.rs` → `phase-23a`
- `test_kobo_mock_store.rs` → `phase-24a`
- `test_shelf_reorder.rs` → `phase-25a`
- `test_inline_serve.rs` → `phase-25a`
- `test_oauth_linking.rs` → `phase-26a`
- `test_book_merge.rs` → `phase-27a`
- `test_admin_gaps.rs` → `phase-28a`

---

## 6. Consistency pass requirements

Before commit:
- Ensure no phase doc references implementation details that diverge from actual shipped code.
- Ensure Phase 23b–28b verify blocks include both:
  - `cargo clippy -- -D warnings`
  - `cargo audit`
- Ensure implementation phase docs contain no `unwrap()` references.

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
rg -n "phase-23|phase-24|phase-25|phase-26|phase-27|phase-28" TEST_PHASES.md
rg -n "\[2\.4\.0\]" docs/CHANGELOG.md
rg -n "opds/cover|opds/osd|oauth.*link|admin/logs|admin/backup|admin/domains" docs/ARCHITECTURE.md docs/API.md GAP.md
```
Expected: documentation reflects all Phase 23–28 closures and release notes are present.

## Commit
```bash
git add GAP.md \
        docs/ARCHITECTURE.md \
        docs/REQUIREMENTS.md \
        docs/CHANGELOG.md \
        TEST_PHASES.md
git commit -m "Phase 29 — documentation pass"
```
