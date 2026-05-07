use crate::{error::AppError, updater, AppState};
use axum::{extract::State, Json};
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

#[derive(Serialize)]
pub struct UpdateStatusResponse {
    pub enabled: bool,
    pub auto_update: bool,
    pub channel: String,
    pub pre_update_hook: String,
    pub block_if_hook_fails: bool,
    pub current_version: &'static str,
}

pub async fn get_status(State(state): State<AppState>) -> Result<Json<UpdateStatusResponse>, AppError> {
    feature_enabled(&state)?;
    Ok(Json(UpdateStatusResponse {
        enabled: state.config.updater.enabled,
        auto_update: state.config.updater.auto_update,
        channel: state.config.updater.channel.clone(),
        pre_update_hook: state.config.updater.pre_update_hook.clone(),
        block_if_hook_fails: state.config.updater.block_if_hook_fails,
        current_version: env!("CARGO_PKG_VERSION"),
    }))
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

    if body.version.trim().is_empty() {
        return Err(AppError::UnprocessableEntity(
            "version field is required".to_string(),
        ));
    }

    let result = updater::apply(&state.config.updater, &body.version, body.dry_run)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "updater: apply failed");
            AppError::Internal
        })?;

    if !result.checksum_ok {
        return Err(AppError::UnprocessableEntity(
            "Checksum mismatch — binary may be corrupt or tampered".to_string(),
        ));
    }

    if !result.hook_ok && state.config.updater.block_if_hook_fails {
        let code = result.hook_exit_code.unwrap_or(-1);
        return Err(AppError::ConflictMessage(format!(
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
