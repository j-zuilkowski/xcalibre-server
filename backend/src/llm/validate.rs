//! LLM-powered book metadata quality validation.
//!
//! Asks the "architect" LLM role to evaluate title, authors, language code, and
//! description quality, returning a severity rating and per-field issues list.
//!
//! # Fallback behaviour
//! If the LLM call fails (timeout, network error, bad JSON), [`validate_book`] returns
//! a benign `severity = "ok"` result with an empty issues list. Validation is advisory
//! only — it must never block normal library operations.
//!
//! # Error never surfaces to users
//! Callers receive a [`ValidationResult`] in all cases; the error is swallowed internally.

use crate::llm::chat::ChatClient;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// A single per-field metadata quality issue.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ValidationIssue {
    pub field: String,
    /// One of `"warning"` or `"error"`.
    pub severity: String,
    pub message: String,
    /// Optional suggested correction text from the LLM.
    pub suggestion: Option<String>,
}

/// Aggregated result of a metadata validation call.
///
/// `severity` is the worst-case across all issues: `"ok"`, `"warning"`, or `"error"`.
/// `model_id` records which model produced the result for audit/display.
#[derive(Clone, Debug, Serialize)]
pub struct ValidationResult {
    pub severity: String,
    pub issues: Vec<ValidationIssue>,
    pub model_id: String,
}

#[derive(Debug, Deserialize)]
struct RawValidationResult {
    severity: String,
    issues: Vec<ValidationIssue>,
}

/// Validate book metadata using the LLM and return a structured result.
///
/// Checks for missing/thin descriptions, missing authors, and dubious language codes.
/// On LLM error or unparseable response, returns `severity = "ok"` with no issues
/// (fail-open: bad metadata is not worse than blocking the ingest pipeline).
pub async fn validate_book(
    client: &ChatClient,
    title: &str,
    authors: &str,
    description: &str,
    language: Option<&str>,
) -> ValidationResult {
    let model_id = client.model_id().to_string();
    let language = language.unwrap_or_default();
    let user_message = format!(
        "Title: {title}\nAuthors: {authors}\nLanguage: {language}\nDescription: {description}\n\nValidate this metadata for:\n- missing/thin description\n- missing author\n- dubious language code\n\nReturn JSON only:\n{{\"severity\":\"ok|warning|error\",\"issues\":[{{\"field\":\"...\",\"severity\":\"warning|error\",\"message\":\"...\",\"suggestion\":\"...\"}}]}}\nNo prose, no markdown fences."
    );

    let completion = match client.complete(&user_message).await {
        Ok(content) => content,
        Err(_) => {
            return ValidationResult {
                severity: "ok".to_string(),
                issues: Vec::new(),
                model_id,
            };
        }
    };

    let parsed = parse_validation_result(&completion).unwrap_or(ValidationResult {
        severity: "ok".to_string(),
        issues: Vec::new(),
        model_id: model_id.clone(),
    });

    ValidationResult { model_id, ..parsed }
}

fn parse_validation_result(raw: &str) -> Option<ValidationResult> {
    if let Some(result) = parse_strict(raw) {
        return Some(result);
    }

    let json_block = extract_json_block(raw)?;
    parse_strict(json_block)
}

fn parse_strict(raw: &str) -> Option<ValidationResult> {
    let parsed: RawValidationResult = serde_json::from_str(raw).ok()?;

    let severity = normalize_severity(&parsed.severity);
    let issues = parsed
        .issues
        .into_iter()
        .filter_map(|issue| {
            let field = issue.field.trim().to_string();
            let message = issue.message.trim().to_string();
            if field.is_empty() || message.is_empty() {
                return None;
            }
            let severity = normalize_severity(&issue.severity);
            let suggestion = issue
                .suggestion
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            Some(ValidationIssue {
                field,
                severity,
                message,
                suggestion,
            })
        })
        .collect::<Vec<_>>();

    Some(ValidationResult {
        severity,
        issues,
        model_id: String::new(),
    })
}

fn normalize_severity(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "error" => "error".to_string(),
        "warning" => "warning".to_string(),
        _ => "ok".to_string(),
    }
}

/// Extract the first complete JSON object from `raw` using brace counting.
///
/// Used as a fallback when the LLM wraps its JSON in markdown fences or leading prose.
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
