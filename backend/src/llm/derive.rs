//! LLM-powered per-book derivation: summary, related titles, discussion questions.
//!
//! Given a book's title, authors, and description, asks the LLM to generate:
//! - A one-paragraph summary
//! - 3–5 related title suggestions
//! - 3–5 discussion questions
//!
//! # Fallback behaviour
//! On LLM error or unparseable JSON, returns a [`DeriveResult`] with all fields empty.
//! Results are purely additive metadata and must never affect core library operations.

use crate::llm::chat::ChatClient;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Derived metadata produced from a single book's title, authors, and description.
///
/// Fields are empty strings / empty vecs when the LLM call fails or the feature is
/// disabled. `model_id` records which model produced the result.
#[derive(Clone, Debug, Serialize)]
pub struct DeriveResult {
    pub summary: String,
    pub related_titles: Vec<String>,
    pub discussion_questions: Vec<String>,
    pub model_id: String,
}

#[derive(Debug, Deserialize)]
struct RawDeriveResult {
    summary: String,
    related_titles: Vec<String>,
    discussion_questions: Vec<String>,
}

/// Ask the LLM to derive a summary, related titles, and discussion questions for a book.
///
/// Returns a [`DeriveResult`] with empty fields on any error.
pub async fn derive_book(
    client: &ChatClient,
    title: &str,
    authors: &str,
    description: &str,
) -> DeriveResult {
    let model_id = client.model_id().to_string();
    let user_message = format!(
        "Title: {title}\nAuthors: {authors}\nDescription: {description}\n\nReturn JSON only:\n{{\"summary\":\"one paragraph\",\"related_titles\":[\"...\"],\"discussion_questions\":[\"...\"]}}\nProvide 3-5 related_titles and 3-5 discussion_questions.\nNo prose outside JSON, no markdown fences."
    );

    let completion = match client.complete(&user_message).await {
        Ok(content) => content,
        Err(_) => {
            return DeriveResult {
                summary: String::new(),
                related_titles: Vec::new(),
                discussion_questions: Vec::new(),
                model_id,
            };
        }
    };

    let parsed = parse_derive_result(&completion).unwrap_or(DeriveResult {
        summary: String::new(),
        related_titles: Vec::new(),
        discussion_questions: Vec::new(),
        model_id: model_id.clone(),
    });

    DeriveResult { model_id, ..parsed }
}

fn parse_derive_result(raw: &str) -> Option<DeriveResult> {
    if let Some(result) = parse_strict(raw) {
        return Some(result);
    }

    let json_block = extract_json_block(raw)?;
    parse_strict(json_block)
}

fn parse_strict(raw: &str) -> Option<DeriveResult> {
    let parsed: RawDeriveResult = serde_json::from_str(raw).ok()?;

    let summary = parsed.summary.trim().to_string();
    let related_titles = parsed
        .related_titles
        .into_iter()
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .collect::<Vec<_>>();
    let discussion_questions = parsed
        .discussion_questions
        .into_iter()
        .map(|question| question.trim().to_string())
        .filter(|question| !question.is_empty())
        .collect::<Vec<_>>();

    Some(DeriveResult {
        summary,
        related_titles,
        discussion_questions,
        model_id: String::new(),
    })
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
