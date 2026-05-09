# xcalibre-server — Frontend Test Implementation

Frontend RTL tests are now embedded directly in the development phase files,
co-located with the components they test (TDD).

| Component(s) | Phase file | Stage/Section |
|---|---|---|
| vitest + MSW setup, renderWithProviders | `docs/CODEX_COMMANDS_PHASE3.md` | Stage 8 |
| LoginPage, RegisterPage, ProtectedRoute | `docs/CODEX_COMMANDS_PHASE3.md` | Stage 8 |
| BookCard, LibraryPage | `docs/CODEX_COMMANDS_PHASE3.md` | Stage 8 |
| BookDetailPage | `docs/CODEX_COMMANDS_PHASE3.md` | Stage 8 |
| ReaderPage, EpubReader | `docs/CODEX_COMMANDS_PHASE3.md` | Stage 8 |
| UsersPage, ImportPage, JobsPage | `docs/CODEX_COMMANDS_PHASE3.md` | Stage 8 |
| ProfilePage, SearchPage | `docs/CODEX_COMMANDS_PHASE3.md` | Stage 8 |
| ShelvesPage | `docs/CODEX_COMMANDS_PHASE9.md` | Stage 1 |

## Backend Integration Tests (Phases 23–28)

| Test file | Phase | What it covers |
|---|---|---|
| `backend/tests/test_opds_enhancements.rs` | `docs/phase-23a-opds-enhancements-tests.md` | OPDS cover serving (3 variants), OSD, path-query search, /new, /hot, /stats, /discover, letter browsing (authors + series) |
| `backend/tests/test_kobo_mock_store.rs` | `docs/phase-24a-kobo-mock-store-tests.md` | 14 Kobo mock store endpoints (products, deals, analytics, user, wishlist, profile, image) |
| `backend/tests/test_shelf_reorder.rs` | `docs/phase-25a-shelf-reorder-inline-serve-tests.md` | PUT /api/v1/shelves/:id/order — happy path, 400 unknown IDs, 400 missing members, 403 non-owner, 404 missing shelf |
| `backend/tests/test_inline_serve.rs` | `docs/phase-25a-shelf-reorder-inline-serve-tests.md` | GET /api/v1/books/:id/view/:format — Content-Disposition: inline, correct Content-Type, 404 missing format |
| `backend/tests/test_oauth_linking.rs` | `docs/phase-26a-oauth-account-linking-tests.md` | GET /me/oauth/providers, OAuth link flow, link callback conflict, DELETE unlink, lockout guard |
| `backend/tests/test_book_merge.rs` | `docs/phase-27a-book-merge-tests.md` | POST merge/preview (counts, 404, 400, 403), POST merge (formats+annotations+shelves), 409 conflict + force, reading progress strategies |
| `backend/tests/test_admin_gaps.rs` | `docs/phase-28a-admin-gaps-tests.md` | Admin log viewer, metadata backup + conflict guard, cover regenerate enqueue, task cancel (200/404/409), domain CRUD + registration enforcement |

Test case specifications: `localProject/TEST_SPEC.md`

## Phase maintenance rule

Any change made during a build must be reflected back in the corresponding
phase file before committing. See `CLAUDE.md` Non-Negotiable Constraints.
