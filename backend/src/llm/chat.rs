use crate::config::AppConfig;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const LLM_TIMEOUT_SECS: u64 = 10;

#[derive(Clone)]
pub struct ChatClient {
    endpoint: String,
    model: String,
    system_prompt: String,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: [ChatMessage<'a>; 2],
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

impl ChatClient {
    pub fn new(config: &AppConfig) -> Option<Self> {
        if !config.llm.enabled {
            return None;
        }

        let endpoint = config.llm.librarian.endpoint.trim().to_string();
        if endpoint.is_empty() {
            return None;
        }

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(LLM_TIMEOUT_SECS))
            .build()
            .ok()?;

        Some(Self {
            endpoint,
            model: config.llm.librarian.model.trim().to_string(),
            system_prompt: config.llm.librarian.system_prompt.trim().to_string(),
            http,
        })
    }

    pub fn is_configured(&self) -> bool {
        !self.endpoint.is_empty() && !self.model.is_empty()
    }

    pub fn model_id(&self) -> &str {
        &self.model
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub async fn complete(&self, user_message: &str) -> anyhow::Result<String> {
        if !self.is_configured() {
            anyhow::bail!("chat endpoint/model is not configured");
        }

        let url = chat_completions_url(&self.endpoint);
        let response = self
            .http
            .post(url)
            .json(&ChatCompletionRequest {
                model: &self.model,
                messages: [
                    ChatMessage {
                        role: "system",
                        content: &self.system_prompt,
                    },
                    ChatMessage {
                        role: "user",
                        content: user_message,
                    },
                ],
            })
            .send()
            .await
            .context("request chat completion")?
            .error_for_status()
            .context("chat completion endpoint returned non-success status")?;

        let payload: ChatCompletionResponse = response
            .json()
            .await
            .context("parse chat completion response")?;
        let Some(choice) = payload.choices.into_iter().next() else {
            anyhow::bail!("chat completion response did not include choices[0]");
        };

        let Some(content) = choice
            .message
            .content
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            anyhow::bail!("chat completion response missing choices[0].message.content");
        };

        Ok(content)
    }
}

fn chat_completions_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    if trimmed.ends_with("/v1/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}
