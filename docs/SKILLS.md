# calibre-web-rs — Skills Reference

Skills are Claude Code slash commands that invoke specialized agents.
This file documents which skills to use at each stage of development and why.

For skills to be automatically available in a session, the workflow below is
summarized in `CLAUDE.md` of the `calibre-web-rs` repo. Refer to this file
for full detail on each skill's purpose and inputs.

---

## Skills Workflow by Phase

### Phase 1 — Backend Foundation

| After stage | Skill | Command | Purpose |
|---|---|---|---|
| Stage 1 (scaffold) | Code Review | `/review` | Verify structure, deps, test harness shape |
| Stage 2 (config+DB) | Code Review | `/review` | Confirm migrations match SCHEMA.md exactly |
| Stage 3 (auth) | Code Review | `/review` | Auth logic, JWT handling, lockout correctness |
| Stage 3 (auth) | Simplify | `/simplify` | Tighten auth code before books builds on it |
| Stage 4 (books) | Code Review | `/review` | CRUD logic, audit log, permission checks |
| Stage 4 (books) | Simplify | `/simplify` | Remove Codex verbosity before file serving builds on it |
| Stage 5 (files) | Code Review | `/review` | Path traversal, range requests, cover pipeline |
| Stage 6 (security) | Code Review | `/review` | Header correctness, rate limit wiring |
| Stage 6 (security) | Security Review | `/security-review` | Full OWASP pass on auth + file serving + headers |
| Stage 7 (docker) | Code Review | `/review` | Image size, non-root user, secrets handling |
| Stage 7 (docker) | Deploy Checklist | `/engineering:deploy-checklist` | Pre-ship verification |

### On Failing Tests (any stage)

```
/engineering:debug
```
Paste the failing test output + stack trace. Faster diagnosis than re-running Codex blind.

### After Phase 1 Complete

```
/init
```
Generate or update `CLAUDE.md` for `calibre-web-rs` to reflect the actual implemented state.

```
/anthropic-skills:consolidate-memory
```
Consolidate all session memories into clean, non-redundant entries.

---

## Skill Details

### `/review` — `engineering:code-review`

**Input**: `git diff` output from a Codex checkpoint.

**What it checks**:
- Acceptance criteria coverage — does the diff address what was asked?
- Scope creep — did Codex touch files it shouldn't have?
- Security: injection, improper auth, hardcoded secrets, path traversal
- N+1 queries, missing error handling, incomplete stubs (TODO/FIXME left in)
- Naming consistency with the rest of the codebase

**How to invoke**: After each Codex stage, run `git diff` and paste output when invoking the skill.

**Output**: PASSED / NEEDS REVIEW / ISSUES FOUND with file:line references.

---

### `/security-review` — Full security audit

**Input**: All changed files on the current branch since `master`.

**What it checks** (OWASP-focused):
- Authentication: JWT validation, token storage, refresh token rotation
- Authorization: role checks on every protected route
- Injection: SQL (sqlx parameterized), path traversal, command injection
- File handling: magic byte validation, storage path confinement
- Security headers: all 5 required headers present and correct
- Secrets: no hardcoded values, config file permissions checked
- Rate limiting: auth endpoints protected

**When**: After Stage 6 (security middleware complete) and before Phase 1 is declared done.

---

### `/simplify` — `simplify`

**Input**: Recently changed code (reads from git diff automatically).

**What it checks**:
- Redundant abstractions (wrapper types that add no value)
- Duplicated logic that should be a shared helper
- Over-engineered solutions for simple problems
- Dead code paths
- Unnecessary `.clone()` or `.unwrap()` calls

**When**: After Stage 3 (auth) and Stage 4 (books) — these produce the most code and later stages build on them. Clean foundations prevent compounding complexity.

---

### `/engineering:debug`

**Input**: Error message, stack trace, or failing test output.

**What it does**: Structured reproduce → isolate → diagnose → fix workflow.

**When**: Any checkpoint where `cargo test` doesn't pass. Don't paste failing output back into Codex without diagnosis first — Codex tends to work around failures rather than fix root causes.

---

### `/engineering:deploy-checklist`

**Input**: Current branch state.

**What it checks**:
- CI passing (cargo test, clippy, audit)
- Docker image builds and starts cleanly
- Migrations run on fresh DB
- Health endpoint responds
- No secrets in image layers
- Non-root user in container
- Config example file present and accurate

**When**: Stage 7 (Docker), before Phase 1 is considered shippable.

---

### `/init`

**What it does**: Reads the codebase and generates or updates `CLAUDE.md` with accurate project instructions.

**When**: After Phase 1 is complete and the `calibre-web-rs` repo has real implemented code. The scaffolded `CLAUDE.md` will be a template — `/init` makes it reflect reality.

---

### `/anthropic-skills:consolidate-memory`

**What it does**: Reads all memory files, merges duplicates, removes stale entries, rewrites cleanly.

**When**: After a major phase completes (Phase 1, Phase 2, etc.) to keep memory lean and accurate.

---

## Phase 2+ Skills Preview

These become relevant in later phases:

| Phase | Skill | When |
|---|---|---|
| Phase 3 (frontend) | `/review` | After each Codex frontend stage |
| Phase 3 (frontend) | `/simplify` | After React component scaffolding |
| Phase 5 (LLM) | `/security-review` | LLM prompt injection is a real attack surface |
| Phase 6 (mobile) | `/engineering:testing-strategy` | Mobile test strategy differs from web |
| Phase 7 (hardening) | `/security-review` | Full final audit before public release |
| Any phase | `/engineering:architecture` | When a mid-build decision needs an ADR |
| Any phase | `/engineering:incident-response` | If something breaks in a local test deployment |
