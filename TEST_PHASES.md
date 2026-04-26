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

Test case specifications: `localProject/TEST_SPEC.md`

## Phase maintenance rule

Any change made during a build must be reflected back in the corresponding
phase file before committing. See `CLAUDE.md` Non-Negotiable Constraints.
