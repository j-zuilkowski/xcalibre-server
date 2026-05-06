//! Google Drive storage backend.
//!
//! Uses the Drive v3 REST API via `reqwest`. OAuth2 tokens are refreshed
//! automatically using the configured `refresh_token`.

use crate::config::GoogleDriveSection;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct GoogleDriveBackend {
    cfg: GoogleDriveSection,
    http: Client,
    /// Cached access token. Refreshed on 401 or expiry.
    token: Arc<Mutex<Option<String>>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct AboutResponse {
    #[serde(rename = "storageQuota")]
    pub storage_quota: StorageQuota,
}

#[derive(Deserialize, Serialize)]
pub struct StorageQuota {
    pub limit: Option<String>,
    pub usage: Option<String>,
}

impl GoogleDriveBackend {
    pub fn new(cfg: GoogleDriveSection) -> Self {
        Self {
            cfg,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("build reqwest client"),
            token: Arc::new(Mutex::new(None)),
        }
    }

    /// Fetch or refresh the access token.
    pub async fn access_token(&self) -> Result<String> {
        let mut guard = self.token.lock().await;
        if let Some(t) = guard.as_ref() {
            return Ok(t.clone());
        }
        let resp = self
            .http
            .post(&self.cfg.token_endpoint)
            .form(&[
                ("client_id", self.cfg.client_id.as_str()),
                ("client_secret", self.cfg.client_secret.as_str()),
                ("refresh_token", self.cfg.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .context("token endpoint request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("token refresh failed: {status} — {body}");
        }

        let token_resp: TokenResponse = resp.json().await.context("parse token response")?;
        *guard = Some(token_resp.access_token.clone());
        Ok(token_resp.access_token)
    }

    /// Invalidate cached token (call after 401 from Drive API).
    pub async fn invalidate_token(&self) {
        *self.token.lock().await = None;
    }

    /// Upload a file to the configured Drive folder. Returns the Drive file ID.
    pub async fn upload_file(
        &self,
        local_path: &std::path::Path,
        file_name: &str,
    ) -> Result<DriveFile> {
        let token = self.access_token().await?;
        let data = tokio::fs::read(local_path)
            .await
            .context("read local file")?;

        let metadata = serde_json::json!({
            "name": file_name,
            "parents": [self.cfg.folder_id]
        });

        let upload_url = format!(
            "{}/upload/drive/v3/files?uploadType=multipart",
            self.cfg.drive_endpoint.trim_end_matches('/')
        );

        let form = reqwest::multipart::Form::new()
            .text("metadata", metadata.to_string())
            .part(
                "file",
                reqwest::multipart::Part::bytes(data).file_name(file_name.to_string()),
            );

        let resp = self
            .http
            .post(&upload_url)
            .bearer_auth(&token)
            .multipart(form)
            .send()
            .await
            .context("Drive upload request")?;

        if resp.status() == 401 {
            self.invalidate_token().await;
            anyhow::bail!("Drive upload: unauthorized (token may have been revoked)");
        }

        let file: DriveFile = resp.json().await.context("parse Drive upload response")?;
        Ok(file)
    }

    /// Get quota information from Drive About endpoint.
    pub async fn get_about(&self) -> Result<AboutResponse> {
        let token = self.access_token().await?;
        let url = format!(
            "{}/drive/v3/about?fields=storageQuota",
            self.cfg.drive_endpoint.trim_end_matches('/')
        );
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .context("Drive about request")?;

        if resp.status() == 401 {
            self.invalidate_token().await;
            anyhow::bail!("Drive about: unauthorized");
        }

        resp.json().await.context("parse Drive about response")
    }
}
