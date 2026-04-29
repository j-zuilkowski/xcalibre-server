# GitHub Publish — xcalibre-server

Run this when you are ready to push the repository to GitHub and enable
automated CI/CD. All local phases (1–21) should be complete and all tests
passing before starting this.

> Paste each stage prompt into Codex one at a time.

---

## Pre-flight checklist

Before running any stage here, verify locally:

- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] `cargo audit` — zero CVEs
- [ ] `pnpm --filter @xs/web test run` — all vitest tests pass
- [ ] `pnpm --filter @xs/web exec playwright test` — all E2E tests pass
- [ ] `git log --oneline -5` — confirm v2.0.0 tag is present
- [ ] `docs/SECURITY.md` — `<org>` placeholder replaced with real GitHub path
- [ ] `config.example.toml` — no real secrets or credentials
- [ ] `.gitignore` includes: `library.db`, `test_e2e.db`, `storage/`, `storage_e2e/`, `.env`, `config.toml`
- [ ] `config.toml` (local dev config) is NOT committed — only `config.example.toml`

---

## STAGE 1 — Create GitHub repository and push

**Paste this into Codex:**

```
Check the current git state and prepare the repository for GitHub.

─────────────────────────────────────────
STEP 1 — Verify .gitignore
─────────────────────────────────────────

Read .gitignore. Confirm it includes all of the following. Add any that
are missing:

  library.db
  library.db-wal
  library.db-shm
  test_e2e.db
  test_e2e.db-wal
  test_e2e.db-shm
  storage/
  storage_e2e/
  .env
  config.toml
  target/
  node_modules/
  .pnpm-store/
  apps/web/dist/
  apps/web/playwright-report/
  apps/web/test-results/

─────────────────────────────────────────
STEP 2 — Audit for secrets
─────────────────────────────────────────

Run:
  git diff --cached --name-only
  git status

Check that config.toml (with real credentials) is NOT staged. Only
config.example.toml should be tracked.

Grep for common secret patterns in tracked files:
  git grep -i "jwt_secret\s*=\s*\"[^\"]\+" -- '*.toml' '*.env'
  git grep -i "secret_key\s*=\s*\"[^\"]\+" -- '*.toml'

If any real secrets are found in tracked files, STOP and fix before
proceeding. Never push credentials to GitHub.

─────────────────────────────────────────
STEP 3 — Create GitHub repo and push
─────────────────────────────────────────

Use the gh CLI:

  gh repo create xcalibre-server \
    --private \
    --source=. \
    --remote=origin \
    --push \
    --description "Self-hosted ebook library server (Rust/Axum)"

Note: --private keeps it private until you decide to make it public.
To make it public immediately, replace --private with --public.

After push completes, verify:
  gh repo view --web

─────────────────────────────────────────
STEP 4 — Push tags
─────────────────────────────────────────

  git push origin --tags

Verify all version tags (v1.0.0 through v2.0.0+) are visible on GitHub:
  gh release list

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Confirm the repository is visible at https://github.com/<your-username>/xcalibre-server
and the main branch shows the latest commit.
```

---

## STAGE 2 — GitHub Actions CI pipeline

**Paste this into Codex:**

```
Create the GitHub Actions CI workflow files.

Read docker/Dockerfile and docker/docker-compose.yml to understand the
build process. Check .cargo/audit.toml for existing CVE suppressions.

─────────────────────────────────────────
DELIVERABLE 1 — .github/workflows/ci.yml
─────────────────────────────────────────

Create .github/workflows/ci.yml:

  name: CI
  on:
    push:
      branches: [main]
    pull_request:
      branches: [main]

  env:
    CARGO_TERM_COLOR: always

  jobs:
    rust:
      name: Rust (test + lint + audit)
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: dtolnay/rust-toolchain@stable
          with:
            components: clippy
        - uses: Swatinem/rust-cache@v2
        - name: cargo test
          run: cargo test --workspace --locked
        - name: cargo clippy
          run: cargo clippy --workspace --locked -- -D warnings
        - name: cargo audit
          run: |
            cargo install cargo-audit --locked
            cargo audit

    frontend:
      name: Frontend (typecheck + vitest)
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: pnpm/action-setup@v3
          with:
            version: 9
        - uses: actions/setup-node@v4
          with:
            node-version: 20
            cache: pnpm
        - run: pnpm install --frozen-lockfile
        - name: typecheck
          run: pnpm --filter @xs/web exec tsc --noEmit
        - name: vitest
          run: pnpm --filter @xs/web test run

    e2e:
      name: E2E (Playwright)
      runs-on: ubuntu-latest
      needs: [rust, frontend]
      steps:
        - uses: actions/checkout@v4
        - uses: pnpm/action-setup@v3
          with:
            version: 9
        - uses: actions/setup-node@v4
          with:
            node-version: 20
            cache: pnpm
        - uses: dtolnay/rust-toolchain@stable
        - uses: Swatinem/rust-cache@v2
        - run: pnpm install --frozen-lockfile
        - name: Install Playwright browsers
          run: pnpm --filter @xs/web exec playwright install --with-deps chromium
        - name: Run migrations
          run: cargo run -p backend -- migrate
          env:
            XCS_DB_URL: sqlite://test_e2e.db
        - name: Run E2E tests
          run: pnpm --filter @xs/web exec playwright test
          env:
            CI: true
            XCS_DB_URL: sqlite://test_e2e.db
            XCS_LLM_ENABLED: "false"
            XCS_STORAGE_PATH: ./storage_e2e

    docker:
      name: Docker build (amd64 dry-run)
      runs-on: ubuntu-latest
      needs: [rust, frontend]
      steps:
        - uses: actions/checkout@v4
        - uses: docker/setup-buildx-action@v3
        - name: Build amd64
          uses: docker/build-push-action@v5
          with:
            context: .
            file: docker/Dockerfile
            platforms: linux/amd64
            push: false
            tags: xcalibre-server:ci

─────────────────────────────────────────
DELIVERABLE 2 — .github/workflows/release.yml
─────────────────────────────────────────

Create .github/workflows/release.yml:

  name: Release
  on:
    push:
      tags: ['v*.*.*']

  jobs:
    publish:
      runs-on: ubuntu-latest
      permissions:
        contents: read
        packages: write
      steps:
        - uses: actions/checkout@v4
        - uses: docker/setup-qemu-action@v3
        - uses: docker/setup-buildx-action@v3
        - uses: docker/login-action@v3
          with:
            registry: ghcr.io
            username: ${{ github.actor }}
            password: ${{ secrets.GITHUB_TOKEN }}
        - name: Extract metadata
          id: meta
          uses: docker/metadata-action@v5
          with:
            images: ghcr.io/${{ github.repository }}
            tags: |
              type=semver,pattern={{version}}
              type=semver,pattern={{major}}.{{minor}}
              type=raw,value=latest
        - uses: docker/build-push-action@v5
          with:
            context: .
            file: docker/Dockerfile
            platforms: linux/amd64,linux/arm64,linux/arm/v7
            push: true
            tags: ${{ steps.meta.outputs.tags }}

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Commit and push:
  git add .github/workflows/
  git commit -m "ci: GitHub Actions CI pipeline and release workflow"
  git push origin main

Then open the Actions tab on GitHub and confirm all four CI jobs start.
Watch for failures — the most common first-run issues are:
  - cargo audit failing on a new advisory → add to .cargo/audit.toml
  - pnpm cache miss on first run (normal, subsequent runs are fast)
  - Playwright browser install taking >10min → consider caching browsers

Fix any failures, push, and confirm all four jobs pass (green checkmarks).
```

---

## STAGE 3 — Branch protection and repository settings

**Paste this into Codex:**

```
Configure GitHub branch protection and repository settings using the gh CLI.

─────────────────────────────────────────
BRANCH PROTECTION — main
─────────────────────────────────────────

Enable branch protection on main so that the CI must pass before merging:

  gh api repos/{owner}/{repo}/branches/main/protection \
    --method PUT \
    --field required_status_checks='{"strict":true,"contexts":["Rust (test + lint + audit)","Frontend (typecheck + vitest)","Docker build (amd64 dry-run)"]}' \
    --field enforce_admins=false \
    --field required_pull_request_reviews=null \
    --field restrictions=null

Note: replace {owner}/{repo} with the actual values, or use:
  REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
  gh api repos/$REPO/branches/main/protection --method PUT ...

E2E is intentionally omitted from required_status_checks for now because
Playwright can be flaky in CI on first setup. Add it after 2 clean CI runs:
  gh api repos/$REPO/branches/main/protection --method PUT \
    --field required_status_checks='{"strict":true,"contexts":["Rust (test + lint + audit)","Frontend (typecheck + vitest)","E2E (Playwright)","Docker build (amd64 dry-run)"]}'

─────────────────────────────────────────
REPOSITORY SETTINGS
─────────────────────────────────────────

  # Enable vulnerability alerts (Dependabot)
  gh api repos/{owner}/{repo}/vulnerability-alerts --method PUT

  # Enable automated security fixes
  gh api repos/{owner}/{repo}/automated-security-fixes --method PUT

  # Update SECURITY.md placeholder with real repo URL
  # Edit docs/SECURITY.md — replace <org>/xcalibre-server with the actual path
  # Commit and push

─────────────────────────────────────────
GITHUB RELEASE — v2.0.0
─────────────────────────────────────────

Create a GitHub Release from the v2.0.0 tag:

  gh release create v2.0.0 \
    --title "v2.0.0 — E2E, Metadata Enrichment, Emby-style UI" \
    --notes-file docs/CHANGELOG.md \
    --latest

This publishes the release page on GitHub with the CHANGELOG as the body.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

  gh release view v2.0.0
  gh repo view --json defaultBranchRef -q '.defaultBranchRef.branchProtectionRule'

Confirm:
  - v2.0.0 release is visible on the GitHub releases page
  - Branch protection shows required status checks
  - Dependabot alerts are enabled (Settings → Security tab)
```

---

## Post-publish checklist

- [ ] Repository visible at `https://github.com/<username>/xcalibre-server`
- [ ] All tags pushed (`git tag | wc -l` matches `gh release list | wc -l`)
- [ ] CI Actions tab shows all four jobs green on latest commit
- [ ] v2.0.0 GitHub Release created with CHANGELOG notes
- [ ] Branch protection on `main` requires CI pass before merge
- [ ] Dependabot vulnerability alerts enabled
- [ ] `docs/SECURITY.md` — `<org>` placeholder replaced with real GitHub path
- [ ] No secrets in any tracked file (`git grep -i "jwt_secret" -- '*.toml'` returns nothing)
- [ ] `config.toml` absent from tracked files (`git ls-files config.toml` returns nothing)
