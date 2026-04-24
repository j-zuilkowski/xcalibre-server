use crate::config::AppConfig;
use anyhow::Context;
use base64::Engine;
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

        if let Some(response) = mock_completion_response(&self.endpoint, user_message) {
            return Ok(response);
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

    pub async fn complete_with_image(
        &self,
        user_message: &str,
        image_bytes: &[u8],
        mime_type: &str,
    ) -> anyhow::Result<String> {
        if !self.is_configured() {
            anyhow::bail!("chat endpoint/model is not configured");
        }

        if self.endpoint.starts_with("mock://") {
            let _ = (user_message, image_bytes, mime_type);
            return Ok("diagram description".to_string());
        }

        let image_data = base64::engine::general_purpose::STANDARD.encode(image_bytes);
        let image_url = format!("data:{mime_type};base64,{image_data}");
        let url = chat_completions_url(&self.endpoint);
        let response = self
            .http
            .post(url)
            .json(&serde_json::json!({
                "model": self.model,
                "messages": [
                    {
                        "role": "system",
                        "content": self.system_prompt,
                    },
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "text",
                                "text": user_message,
                            },
                            {
                                "type": "image_url",
                                "image_url": {
                                    "url": image_url,
                                }
                            }
                        ]
                    }
                ]
            }))
            .send()
            .await
            .context("request chat completion with image")?
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

    pub async fn supports_vision(&self) -> anyhow::Result<bool> {
        if !self.is_configured() {
            anyhow::bail!("chat endpoint/model is not configured");
        }

        if self.endpoint.starts_with("mock://") {
            return Ok(true);
        }

        let response = self
            .http
            .get(models_url(&self.endpoint))
            .send()
            .await
            .context("request models list")?
            .error_for_status()
            .context("models endpoint returned non-success status")?;

        let payload: serde_json::Value = response
            .json()
            .await
            .context("parse models response")?;
        Ok(models_response_supports_vision(&payload, &self.model))
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

fn mock_completion_response(endpoint: &str, user_message: &str) -> Option<String> {
    if !endpoint.starts_with("mock://") {
        return None;
    }

    if user_message.contains("Classify this book.") {
        return Some(
            serde_json::json!({
                "tags": [{
                    "name": "Science Fiction",
                    "confidence": 0.92
                }]
            })
            .to_string(),
        );
    }

    if user_message.contains("Validate this metadata") {
        return Some(
            serde_json::json!({
                "severity": "ok",
                "issues": []
            })
            .to_string(),
        );
    }

    Some(String::new())
}

fn models_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    if trimmed.ends_with("/v1/models") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}

fn models_response_supports_vision(payload: &serde_json::Value, model_id: &str) -> bool {
    let Some(data) = payload.get("data") else {
        return value_supports_vision(payload, model_id);
    };
    let Some(entries) = data.as_array() else {
        return value_supports_vision(payload, model_id);
    };

    entries.iter().any(|entry| {
        model_matches(entry, model_id) && value_supports_vision(entry, model_id)
    })
}

fn model_matches(value: &serde_json::Value, model_id: &str) -> bool {
    value
        .get("id")
        .and_then(|id| id.as_str())
        .map(|id| id == model_id)
        .unwrap_or(false)
        || value
            .get("model")
            .and_then(|id| id.as_str())
            .map(|id| id == model_id)
            .unwrap_or(false)
        || value
            .get("name")
            .and_then(|id| id.as_str())
            .map(|id| id == model_id)
            .unwrap_or(false)
}

fn value_supports_vision(value: &serde_json::Value, model_id: &str) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(capabilities) = map.get("capabilities") {
                if value_supports_vision(capabilities, model_id) {
                    return true;
                }
            }

            if map
                .get("modalities")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .any(|item| item.eq_ignore_ascii_case("image"))
                })
                .unwrap_or(false)
            {
                return true;
            }

            for (key, nested) in map {
                if matches!(
                    key.as_str(),
                    "image_input" | "imageInput" | "vision" | "vision_enabled" | "supports_images"
                ) && nested.as_bool().unwrap_or(false)
                {
                    return true;
                }

                if value_supports_vision(nested, model_id) {
                    return true;
                }
            }
            false
        }
        serde_json::Value::Array(items) => items.iter().any(|item| value_supports_vision(item, model_id)),
        serde_json::Value::String(value) => {
            value.eq_ignore_ascii_case("image_input") || value.eq_ignore_ascii_case("vision")
        }
        serde_json::Value::Bool(value) => *value && !model_id.is_empty(),
        _ => false,
    }
}
