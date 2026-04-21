use crate::llm::chat::ChatClient;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TagSuggestion {
    pub name: String,
    pub confidence: f32,
}

#[derive(Clone, Debug)]
pub struct ClassifyResult {
    pub suggestions: Vec<TagSuggestion>,
    pub model_id: String,
}

#[derive(Debug, Deserialize)]
struct ClassifyResponse {
    tags: Vec<TagSuggestion>,
}

pub async fn classify_book(
    client: &ChatClient,
    title: &str,
    authors: &str,
    description: &str,
) -> ClassifyResult {
    let model_id = client.model_id().to_string();
    let user_message = format!(
        "Title: {title}\nAuthors: {authors}\nDescription: {description}\n\nClassify this book. Return JSON only:\n{{\"tags\": [{{\"name\": \"...\", \"confidence\": 0.0-1.0}}]}}\nReturn 3-8 tags. No prose, no markdown fences."
    );

    let completion = match client.complete(&user_message).await {
        Ok(content) => content,
        Err(_) => {
            return ClassifyResult {
                suggestions: Vec::new(),
                model_id,
            };
        }
    };

    let suggestions = parse_tag_suggestions(&completion).unwrap_or_default();

    ClassifyResult {
        suggestions,
        model_id,
    }
}

fn parse_tag_suggestions(raw: &str) -> Option<Vec<TagSuggestion>> {
    if let Some(suggestions) = parse_strict(raw) {
        return Some(suggestions);
    }

    let json_block = extract_json_block(raw)?;
    parse_strict(json_block)
}

fn parse_strict(raw: &str) -> Option<Vec<TagSuggestion>> {
    let parsed: ClassifyResponse = serde_json::from_str(raw).ok()?;
    let suggestions = parsed
        .tags
        .into_iter()
        .filter_map(|suggestion| {
            let name = suggestion.name.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let confidence = if suggestion.confidence.is_finite() {
                suggestion.confidence.clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some(TagSuggestion { name, confidence })
        })
        .collect();
    Some(suggestions)
}

fn extract_json_block(raw: &str) -> Option<&str> {
    static JSON_START_REGEX: OnceLock<Regex> = OnceLock::new();
    let start_regex =
        JSON_START_REGEX.get_or_init(|| Regex::new(r"\{").expect("valid JSON start regex"));
    let start = start_regex.find(raw)?.start();

    let mut depth = 0_i32;
    let mut end = None;
    for (offset, ch) in raw[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + offset + ch.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }

    let end = end?;
    Some(&raw[start..end])
}
