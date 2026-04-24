use crate::config::AppConfig;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const LLM_TIMEOUT_SECS: u64 = 10;

#[derive(Clone)]
pub struct EmbeddingClient {
    endpoint: String,
    model: String,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
    input: &'a str,
    model: &'a str,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl EmbeddingClient {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(LLM_TIMEOUT_SECS))
            .build()
            .context("build embeddings http client")?;

        Ok(Self {
            endpoint: config.llm.librarian.endpoint.trim().to_string(),
            model: config.llm.librarian.model.trim().to_string(),
            http,
        })
    }

    pub fn is_configured(&self) -> bool {
        !self.endpoint.is_empty() && !self.model.is_empty()
    }

    pub fn model_id(&self) -> &str {
        &self.model
    }

    pub async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        if !self.is_configured() {
            anyhow::bail!("embedding endpoint/model is not configured");
        }

        if self.endpoint.starts_with("mock://") {
            let _ = text;
            return Ok(vec![0.1, 0.2, 0.3]);
        }

        let url = embeddings_url(&self.endpoint);
        let response = self
            .http
            .post(url)
            .json(&EmbeddingRequest {
                input: text,
                model: &self.model,
            })
            .send()
            .await
            .context("request embeddings")?
            .error_for_status()
            .context("embedding endpoint returned non-success status")?;

        let payload: EmbeddingResponse =
            response.json().await.context("parse embeddings response")?;
        let Some(first) = payload.data.into_iter().next() else {
            anyhow::bail!("embedding response did not include data[0]");
        };

        if first.embedding.is_empty() {
            anyhow::bail!("embedding response returned empty vector");
        }

        Ok(first.embedding)
    }
}

fn embeddings_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/embeddings")
    } else {
        format!("{trimmed}/v1/embeddings")
    }
}
