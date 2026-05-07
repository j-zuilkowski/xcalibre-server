# Phase 22b — In-Place Self-Update Implementation

## Context
Rust 2021, Axum 0.7.
Phase 22a complete: failing tests in `backend/tests/test_self_update.rs`.
`sha2` and `hex` already in `Cargo.toml`. No new crate dependencies required.

---

## 1. Config — add `UpdaterSection` to `backend/src/config.rs`

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdaterSection {
    /// Master switch — when false, all update endpoints return 404.
    pub enabled: bool,
    /// When true, the background update-check task also applies the update.
    /// When false, only the explicit POST /apply endpoint triggers an update.
    pub auto_update: bool,
    /// Release channel — "stable" or "nightly". Determines which release tag to pull.
    pub channel: String,
    /// Shell command run before replacing the binary. Empty = skip.
    pub pre_update_hook: String,
    /// When true (default), a non-zero exit from `pre_update_hook` aborts the update.
    /// When false, hook failure is logged as a warning but the update proceeds.
    pub block_if_hook_fails: bool,
}

impl Default for UpdaterSection {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_update: false,
            channel: "stable".to_string(),
            pre_update_hook: String::new(),
            block_if_hook_fails: true,
        }
    }
}
```

Add `pub updater: UpdaterSection` to `AppConfig`.

---

## 2. Updater logic — `backend/src/updater.rs`

```rust
//! In-place binary self-update.
//!
//! Download procedure:
//!   1. GET  <download_url>/v{version}/backend-{target}          → binary
//!   2. GET  <download_url>/v{version}/backend-{target}.sha256   → expected hex digest
//!   3. Verify SHA-256 of downloaded bytes against expected digest
//!   4. Run `pre_update_hook` if configured; abort if exit ≠ 0 and block_if_hook_fails
//!   5. Rename current binary to <binary>.old, write new binary, chmod +x, send SIGTERM
//!      (systemd / Docker will restart; if not managed, the process exits cleanly)
//!
//! `dry_run = true` performs steps 1–4 only and returns results without replacing files.

use crate::config::UpdaterSection;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::process::Command;
use tracing::{info, warn};

fn download_base_url() -> String {
    std::env::var("XCS_RELEASES_DOWNLOAD_URL")
        .unwrap_or_else(|_| "https://github.com/xcalibre-server/xcalibre-server/releases/download".to_string())
}

fn target_triple() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { "x86_64-unknown-linux-musl" }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { "aarch64-unknown-linux-musl" }
    #[cfg(target_os = "macos")]
    { "aarch64-apple-darwin" }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    { "unknown" }
}

pub struct ApplyResult {
    pub checksum_ok: bool,
    pub hook_ok: bool,      // true if no hook configured or hook exited 0
    pub hook_exit_code: Option<i32>,
    pub applied: bool,      // false on dry_run or if aborted
}

pub async fn apply(cfg: &UpdaterSection, version: &str, dry_run: bool) -> Result<ApplyResult> {
    let base = download_base_url();
    let target = target_triple();
    let binary_name = format!("backend-{target}");

    let binary_url = format!("{base}/v{version}/{binary_name}");
    let checksum_url = format!("{base}/v{version}/{binary_name}.sha256");

    // Download binary
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    info!(%binary_url, "updater: downloading binary");
    let binary_bytes = http
        .get(&binary_url)
        .send()
        .await
        .context("download binary")?
        .error_for_status()
        .context("binary download status")?
        .bytes()
        .await
        .context("read binary bytes")?;

    // Download checksum
    let expected_hex = http
        .get(&checksum_url)
        .send()
        .await
        .context("download checksum")?
        .error_for_status()
        .context("checksum download status")?
        .text()
        .await
        .context("read checksum")?;
    let expected_hex = expected_hex.trim().to_lowercase();

    // Verify checksum
    let mut hasher = Sha256::new();
    hasher.update(&binary_bytes);
    let actual_hex = hex::encode(hasher.finalize());

    if actual_hex != expected_hex {
        return Ok(ApplyResult {
            checksum_ok: false,
            hook_ok: true,
            hook_exit_code: None,
            applied: false,
        });
    }

    // Run pre-update hook
    let (hook_ok, hook_exit_code) = if cfg.pre_update_hook.is_empty() {
        (true, None)
    } else {
        info!(hook = %cfg.pre_update_hook, "updater: running pre-update hook");
        let status = Command::new("sh")
            .arg("-c")
            .arg(&cfg.pre_update_hook)
            .status();
        match status {
            Ok(s) => {
                let code = s.code();
                let ok = s.success();
                if !ok {
                    warn!(exit_code = ?code, "updater: pre-update hook failed");
                }
                (ok, code)
            }
            Err(e) => {
                warn!(error = %e, "updater: could not execute pre-update hook");
                (false, Some(-1))
            }
        }
    };

    if !hook_ok && cfg.block_if_hook_fails {
        return Ok(ApplyResult {
            checksum_ok: true,
            hook_ok: false,
            hook_exit_code,
            applied: false,
        });
    }

    if dry_run {
        return Ok(ApplyResult {
            checksum_ok: true,
            hook_ok,
            hook_exit_code,
            applied: false,
        });
    }

    // Replace binary
    let current_exe = std::env::current_exe().context("current_exe")?;
    let old_path = current_exe.with_extension("old");

    // Rename current → .old
    std::fs::rename(&current_exe, &old_path).context("rename current binary")?;

    // Write new binary
    std::fs::write(&current_exe, &binary_bytes).context("write new binary")?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&current_exe)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&current_exe, perms)?;
    }

    info!(%version, "updater: binary replaced, sending SIGTERM for restart");

    // Signal the process to exit cleanly — systemd/Docker will restart it
    #[cfg(unix)]
    {
        let pid = std::process::id();
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    }

    Ok(ApplyResult {
        checksum_ok: true,
        hook_ok,
        hook_exit_code,
        applied: true,
    })
}
```

Add `pub mod updater;` to `backend/src/lib.rs`.

Add to `backend/Cargo.toml` if not already present:
```toml
libc = "0.2"
```

---

## 3. Route handlers — `backend/src/api/admin/self_update.rs`

```rust
use crate::{error::AppError, updater, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn feature_enabled(state: &AppState) -> Result<(), AppError> {
    if state.config.updater.enabled {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

// ── GET /api/v1/admin/system/update/status ────────────────────────────────────

pub async fn get_status(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    feature_enabled(&state)?;
    Ok(Json(json!({
        "enabled":            state.config.updater.enabled,
        "auto_update":        state.config.updater.auto_update,
        "channel":            state.config.updater.channel,
        "pre_update_hook":    state.config.updater.pre_update_hook,
        "block_if_hook_fails": state.config.updater.block_if_hook_fails,
        "current_version":    env!("CARGO_PKG_VERSION"),
    })))
}

// ── POST /api/v1/admin/system/update/apply ────────────────────────────────────

#[derive(Deserialize)]
pub struct ApplyRequest {
    pub version: String,
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn apply_update(
    State(state): State<AppState>,
    Json(body): Json<ApplyRequest>,
) -> Result<Json<Value>, AppError> {
    feature_enabled(&state)?;

    let result = updater::apply(&state.config.updater, &body.version, body.dry_run)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "updater: apply failed");
            AppError::InternalServerError
        })?;

    if !result.checksum_ok {
        return Err(AppError::UnprocessableEntity(
            "Checksum mismatch — binary may be corrupt or tampered".to_string(),
        ));
    }

    if !result.hook_ok && state.config.updater.block_if_hook_fails {
        let code = result.hook_exit_code.unwrap_or(-1);
        return Err(AppError::Conflict(format!(
            "pre-update hook exited with code {code}"
        )));
    }

    Ok(Json(json!({
        "checksum_ok":     result.checksum_ok,
        "hook_ok":         result.hook_ok,
        "hook_exit_code":  result.hook_exit_code,
        "applied":         result.applied,
        "dry_run":         body.dry_run,
        "version":         body.version,
    })))
}
```

Add `pub mod self_update;` to `backend/src/api/admin/mod.rs`.

---

## 4. Auto-update integration — `backend/src/api/admin/update_check.rs`

In the existing background update-check loop, after confirming `update_available = true`:

```rust
if state.config.updater.auto_update && state.config.updater.enabled {
    tracing::info!(version = %latest_version, "auto-update: applying");
    match updater::apply(&state.config.updater, &latest_version, false).await {
        Ok(r) if r.applied => tracing::info!("auto-update: applied, restarting"),
        Ok(r) => tracing::warn!(?r.hook_exit_code, "auto-update: apply skipped"),
        Err(e) => tracing::error!(error = %e, "auto-update: apply failed"),
    }
}
```

---

## 5. AppError additions — `backend/src/error.rs`

Add two new variants if not already present:

```rust
UnprocessableEntity(String),
Conflict(String),
```

And their `IntoResponse` arms:

```rust
AppError::UnprocessableEntity(msg) => (
    StatusCode::UNPROCESSABLE_ENTITY,
    Json(json!({ "error": msg, "checksum_ok": false })),
).into_response(),
AppError::Conflict(msg) => (
    StatusCode::CONFLICT,
    Json(json!({ "error": msg })),
).into_response(),
```

---

## 6. Router wiring — `backend/src/api/mod.rs`

```rust
// In the admin router (require_admin middleware):
.route("/admin/system/update/status", get(admin::self_update::get_status))
.route("/admin/system/update/apply",  post(admin::self_update::apply_update))
```

---

## 7. Update feature comparison

In `~/Documents/localProject/FEATURE_COMPARISON.md`:
```
| Self-update | ✓ built-in | ✓ stable/nightly | ✓ Tauri updater | ✓ configurable, hook-gated |
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_self_update 2>&1 | tail -20
cargo clippy -- -D warnings 2>&1 | grep "^error" | head -10
```
Expected: **all tests pass**, zero clippy errors.

## Commit
```bash
git add backend/src/updater.rs \
        backend/src/api/admin/self_update.rs \
        backend/src/api/admin/mod.rs \
        backend/src/api/admin/update_check.rs \
        backend/src/config.rs \
        backend/src/error.rs \
        backend/src/api/mod.rs \
        backend/src/lib.rs \
        backend/Cargo.toml
git commit -m "Phase 22b — in-place self-update with pre-update hook gate (configurable enable/disable)"
```
