//! LLM-powered prose quality scoring for book descriptions.
//!
//! Sends the book title and description to the LLM and asks for a `0.0–1.0` quality
//! score plus a list of formatting/content/style issues.
//!
//! # Fallback behaviour
//! On any LLM error, returns `score = 0.5` with an empty issues list (neutral score,
//! not "bad"). Quality checks are advisory — they must never block operations.
//! LLM errors are silently swallowed; callers never see them.

use crate::llm::chat::ChatClient;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// A single prose quality issue reported by the LLM.
///
/// `issue_type` is one of `"formatting"`, `"content"`, `"style"`, or `"other"`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QualityIssue {
    pub issue_type: String,
    /// `"warning"` or `"error"`.
    pub severity: String,
    pub message: String,
}

/// Result of a prose quality check.
///
/// `score` is clamped to `[0.0, 1.0]`; non-finite values fall back to `0.5`.
/// `model_id` records which model produced the result for audit/display.
#[derive(Clone, Debug, Serialize)]
pub struct QualityResult {
    pub score: f32,
    pub issues: Vec<QualityIssue>,
    pub model_id: String,
}

#[derive(Debug, Deserialize)]
struct RawQualityResult {
    score: f32,
    issues: Vec<QualityIssue>,
}

/// Ask the LLM to score the prose quality of a book's title + description.
///
/// Returns a [`QualityResult`] in all cases; errors produce `score = 0.5` and no issues.
pub async fn check_quality(client: &ChatClient, title: &str, description: &str) -> QualityResult {
    let model_id = client.model_id().to_string();
    let user_message = format!(
        "Title: {title}\nDescription: {description}\n\nScore prose quality from 0.0 to 1.0 and list formatting/content issues.\nReturn JSON only:\n{{\"score\":0.0,\"issues\":[{{\"issue_type\":\"formatting|content|style|other\",\"severity\":\"warning|error\",\"message\":\"...\"}}]}}\nNo prose, no markdown fences."
    );

    let completion = match client.complete(&user_message).await {
        Ok(content) => content,
        Err(_) => {
            return QualityResult {
                score: 0.5,
                issues: Vec::new(),
                model_id,
            };
        }
    };

    let parsed = parse_quality_result(&completion).unwrap_or(QualityResult {
        score: 0.5,
        issues: Vec::new(),
        model_id: model_id.clone(),
    });

    QualityResult { model_id, ..parsed }
}

fn parse_quality_result(raw: &str) -> Option<QualityResult> {
    if let Some(result) = parse_strict(raw) {
        return Some(result);
    }

    let json_block = extract_json_block(raw)?;
    parse_strict(json_block)
}

fn parse_strict(raw: &str) -> Option<QualityResult> {
    let parsed: RawQualityResult = serde_json::from_str(raw).ok()?;

    let score = if parsed.score.is_finite() {
        parsed.score.clamp(0.0, 1.0)
    } else {
        0.5
    };

    let issues = parsed
        .issues
        .into_iter()
        .filter_map(|issue| {
            let issue_type = issue.issue_type.trim().to_string();
            let message = issue.message.trim().to_string();
            if issue_type.is_empty() || message.is_empty() {
                return None;
            }
            let severity = normalize_issue_severity(&issue.severity);
            Some(QualityIssue {
                issue_type,
                severity,
                message,
            })
        })
        .collect::<Vec<_>>();

    Some(QualityResult {
        score,
        issues,
        model_id: String::new(),
    })
}

fn normalize_issue_severity(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "error" => "error".to_string(),
        _ => "warning".to_string(),
    }
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
