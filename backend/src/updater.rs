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
use anyhow::{Context, Result};
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

    let binary_url = format!("{base}/releases/download/v{version}/{binary_name}");
    let checksum_url = format!("{base}/releases/download/v{version}/{binary_name}.sha256");

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
