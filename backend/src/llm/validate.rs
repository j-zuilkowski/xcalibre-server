use crate::llm::chat::ChatClient;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ValidationIssue {
    pub field: String,
    pub severity: String,
    pub message: String,
    pub suggestion: Option<String>,
}

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
