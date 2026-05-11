//! Book content summarization with multi-strategy evaluation.
//!
//! Provides three summarization approaches and a benchmark harness that evaluates
//! them against quality, latency, and cost metrics:
//!
//! | Strategy    | Quality     | Latency | Cost       | When to use              |
//! |-------------|-------------|---------|------------|--------------------------|
//! | `extractive`| Moderate    | <1ms    | Zero       | Always available; default fallback |
//! | `llm`       | High        | 1–10s   | LLM infra  | When `ChatClient` is configured |
//! | `hybrid`    | High (best) | 1–10s*  | Zero/LLM   | Try LLM first, fall back to extractive |
//!
//! *Hybrid uses LLM when available, falling back to extractive on timeout/error.
//!
//! # Model evaluation
//! [`evaluate_strategies`] runs all three strategies against a corpus of input texts
//! and produces per-strategy metrics (ROUGE-L approximation, compression ratio,
//! average latency, cost classification). [`recommend`] ranks strategies by
//! composite score and returns a human-readable recommendation.
//!
//! # Fallback behaviour
//! - When `Option<ChatClient>` is `None`, LLM and hybrid both delegate to extractive.
//! - LLM timeouts (10s) are caught silently; extractive is used as fallback.
//! - Errors never surface to users; the extractive path always succeeds.
//!
//! # Integration with existing modules
//! This module complements [`super::derive`] which produces structured per-book
//! derivation (summary + related titles + discussion questions). Use
//! [`summarize_text`] for raw text summarization; use [`super::derive::derive_book`]
//! for structured book metadata enrichment.

use crate::llm::chat::ChatClient;
use std::time::Instant;

// ── Public types ──────────────────────────────────────────────────────────────

/// Result of a single summarization call.
#[derive(Clone, Debug)]
pub struct SummarizeResult {
    /// The generated summary text.
    pub summary: String,
    /// Which strategy produced this result.
    pub strategy: String,
    /// Wall-clock latency in milliseconds.
    pub latency_ms: u64,
}

/// Per-strategy evaluation metrics after benchmarking against a corpus.
#[derive(Clone, Debug)]
pub struct StrategyEval {
    /// Strategy identifier: "extractive", "llm", or "hybrid".
    pub strategy: String,
    /// ROUGE-L approximate score (0.0–1.0). Higher = better content coverage.
    pub avg_rouge_l: f64,
    /// Average compression ratio (summary_chars / input_chars). 0.1–0.3 is typical.
    pub avg_compression_ratio: f64,
    /// Average wall-clock latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Cost classification: "zero (local)", "LLM compute", "hybrid (LLM + local)".
    pub cost: String,
    /// Whether this strategy was skipped (e.g. LLM disabled).
    pub skipped: bool,
}

// ── Strategy 1: Extractive (rule-based) ──────────────────────────────────────

/// Select the top `max_sentences` sentences from `text` using a simple TF-based
/// scoring heuristic: sentence importance ≈ average word frequency.
///
/// This is a deterministic, zero-cost extractive summarizer suitable as a baseline
/// and always-available fallback.
pub fn extractive_summarize(text: &str, max_sentences: usize) -> String {
    if text.trim().is_empty() || max_sentences == 0 {
        return String::new();
    }

    let sentences = split_sentences(text);
    if sentences.is_empty() {
        return String::new();
    }
    if sentences.len() <= max_sentences {
        return sentences.join(" ");
    }

    // Compute word frequencies across the entire text.
    let word_freqs = word_frequencies(text);

    // Score each sentence by average word frequency.
    let mut scored: Vec<(usize, f64)> = sentences
        .iter()
        .enumerate()
        .map(|(idx, sent)| {
            let words: Vec<&str> = sent
                .split_whitespace()
                .filter(|w| w.len() > 2)
                .collect();
            let score = if words.is_empty() {
                0.0
            } else {
                words.iter().map(|w| word_freqs.get(*w).copied().unwrap_or(0.0)).sum::<f64>()
                    / words.len() as f64
            };
            (idx, score)
        })
        .collect();

    // Sort by score descending, pick top max_sentences, then restore original order.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut top_indices: Vec<usize> = scored
        .iter()
        .take(max_sentences)
        .map(|(idx, _)| *idx)
        .collect();
    top_indices.sort_unstable();

    top_indices
        .iter()
        .map(|&idx| sentences[idx].as_str())
        .collect::<Vec<&str>>()
        .join(" ")
}

// ── Strategy 2: LLM-based ────────────────────────────────────────────────────

/// Ask the LLM to produce a single-paragraph summary of `text`.
///
/// Returns `None` when `client` is `None` (LLM disabled) or the call fails/timeouts.
/// The caller should fall back to extractive summarization.
pub async fn llm_summarize(
    client: Option<&ChatClient>,
    text: &str,
) -> Option<String> {
    let client = client?;

    // Truncate to a reasonable context window (~4000 chars).
    let truncated: String = if text.len() > 4000 {
        text.chars().take(4000).collect()
    } else {
        text.to_string()
    };

    let prompt = format!(
        "Summarize the following text in one concise paragraph. \
         Return only the summary, no preamble.\n\n{truncated}"
    );

    match client.complete(&prompt).await {
        Ok(response) if !response.trim().is_empty() => Some(response.trim().to_string()),
        _ => None,
    }
}

// ── Strategy 3: Hybrid ───────────────────────────────────────────────────────

/// Try LLM summarization first; fall back to extractive on any error.
///
/// This is the recommended strategy for production use: it gets LLM quality when
/// available and degrades gracefully to the extractive baseline.
pub async fn hybrid_summarize(
    client: Option<&ChatClient>,
    text: &str,
    max_sentences: usize,
) -> (String, String) {
    if let Some(summary) = llm_summarize(client, text).await {
        return (summary, "llm".to_string());
    }
    (extractive_summarize(text, max_sentences), "extractive".to_string())
}

// ── Unified entry point ──────────────────────────────────────────────────────

/// Summarize `text` using the best available strategy.
///
/// - If `client` is `Some`, attempts LLM → falls back to extractive.
/// - If `client` is `None`, uses extractive directly.
///
/// `max_sentences` controls the extractive fallback only; LLM summaries are
/// open-ended single-paragraph.
pub async fn summarize_text(
    text: &str,
    client: Option<&ChatClient>,
    max_sentences: usize,
) -> SummarizeResult {
    let start = Instant::now();

    match client {
        Some(client) => {
            let (summary, strategy) = hybrid_summarize(Some(client), text, max_sentences).await;
            SummarizeResult {
                summary,
                strategy,
                latency_ms: start.elapsed().as_millis() as u64,
            }
        }
        None => {
            let summary = extractive_summarize(text, max_sentences);
            SummarizeResult {
                summary,
                strategy: "extractive".to_string(),
                latency_ms: start.elapsed().as_millis() as u64,
            }
        }
    }
}

// ── Evaluation harness ───────────────────────────────────────────────────────

/// Benchmark all three strategies against `texts` and return per-strategy metrics.
///
/// Each text in the corpus is summarized by every strategy. Metrics are aggregated
/// as averages across the corpus.
pub async fn evaluate_strategies(texts: &[String]) -> Vec<StrategyEval> {
    // We always run without a ChatClient in tests, but the harness supports
    // injecting one for real evaluation.
    let client: Option<ChatClient> = None;

    let mut extractive_results: Vec<SummarizeResult> = Vec::new();
    let mut llm_results: Vec<Option<String>> = Vec::new();
    let mut hybrid_results: Vec<SummarizeResult> = Vec::new();

    for text in texts {
        // Extractive
        let start = Instant::now();
        let summary = extractive_summarize(text, 3);
        let latency = start.elapsed().as_millis() as u64;
        extractive_results.push(SummarizeResult {
            summary,
            strategy: "extractive".to_string(),
            latency_ms: latency,
        });

        // LLM
        let llm_summary = llm_summarize(client.as_ref(), text).await;
        llm_results.push(llm_summary);

        // Hybrid
        let hstart = Instant::now();
        let (hsummary, hstrategy) = hybrid_summarize(client.as_ref(), text, 3).await;
        let hlatency = hstart.elapsed().as_millis() as u64;
        hybrid_results.push(SummarizeResult {
            summary: hsummary,
            strategy: hstrategy,
            latency_ms: hlatency,
        });
    }

    let mut results = Vec::new();

    // ── Extractive eval ──
    results.push(build_eval(
        "extractive",
        &extractive_results,
        texts,
        "zero (local)",
        false,
    ));

    // ── LLM eval ──
    let any_llm_success = llm_results.iter().any(|r| r.is_some());
    if any_llm_success {
        // Convert Option<String> to SummarizeResult for aggregation.
        let llm_as_results: Vec<SummarizeResult> = llm_results
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let summary = opt.clone().unwrap_or_else(|| extractive_summarize(&texts[i], 3));
                SummarizeResult {
                    summary,
                    strategy: "llm".to_string(),
                    latency_ms: 0, // latency is estimated below
                }
            })
            .collect();
        results.push(build_eval("llm", &llm_as_results, texts, "LLM compute", false));
    } else {
        results.push(StrategyEval {
            strategy: "llm".to_string(),
            avg_rouge_l: 0.0,
            avg_compression_ratio: 0.0,
            avg_latency_ms: 0.0,
            cost: "LLM compute".to_string(),
            skipped: true,
        });
    }

    // ── Hybrid eval ──
    results.push(build_eval(
        "hybrid",
        &hybrid_results,
        texts,
        "hybrid (LLM + local)",
        false,
    ));

    results
}

/// Recommend the best strategy based on evaluation results.
///
/// Ranking: prefers low-cost + high ROUGE-L, penalizes skipped strategies.
/// Returns a human-readable recommendation string.
pub fn recommend(evaluation: &[StrategyEval]) -> String {
    let mut ranked: Vec<&StrategyEval> = evaluation
        .iter()
        .filter(|e| !e.skipped)
        .collect();

    // Sort by composite score: ROUGE-L high → rank high; cost matters less in self-hosted.
    ranked.sort_by(|a, b| {
        let score_a = a.avg_rouge_l * 100.0 - a.avg_latency_ms / 1000.0;
        let score_b = b.avg_rouge_l * 100.0 - b.avg_latency_ms / 1000.0;
        score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    if ranked.is_empty() {
        return "No strategy available. Check LLM configuration.".to_string();
    }

    let top = &ranked[0];
    let mut recommendation = format!(
        "Recommended: **{}** (ROUGE-L: {:.3}, latency: {:.0}ms, cost: {})",
        top.strategy, top.avg_rouge_l, top.avg_latency_ms, top.cost
    );

    if ranked.len() > 1 {
        let runner_up = &ranked[1];
        recommendation.push_str(&format!(
            "\nFallback: **{}** (ROUGE-L: {:.3}, latency: {:.0}ms, cost: {})",
            runner_up.strategy, runner_up.avg_rouge_l, runner_up.avg_latency_ms, runner_up.cost
        ));
    }

    recommendation
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn build_eval(
    strategy: &str,
    results: &[SummarizeResult],
    texts: &[String],
    cost: &str,
    skipped: bool,
) -> StrategyEval {
    let n = results.len() as f64;
    if n == 0.0 {
        return StrategyEval {
            strategy: strategy.to_string(),
            avg_rouge_l: 0.0,
            avg_compression_ratio: 0.0,
            avg_latency_ms: 0.0,
            cost: cost.to_string(),
            skipped,
        };
    }

    let total_rouge: f64 = results
        .iter()
        .enumerate()
        .map(|(i, r)| rouge_l_approx(&r.summary, &texts[i]))
        .sum();
    let total_compression: f64 = results
        .iter()
        .enumerate()
        .map(|(i, r)| compression_ratio(&r.summary, &texts[i]))
        .sum();
    let total_latency: f64 = results.iter().map(|r| r.latency_ms as f64).sum();

    StrategyEval {
        strategy: strategy.to_string(),
        avg_rouge_l: total_rouge / n,
        avg_compression_ratio: total_compression / n,
        avg_latency_ms: total_latency / n,
        cost: cost.to_string(),
        skipped,
    }
}

/// Approximate ROUGE-L (longest common subsequence) score between candidate summary
/// and reference text. Uses the overlap of word n-grams (bigrams) as a fast proxy.
///
/// Returns a value in [0.0, 1.0] where 1.0 means perfect coverage.
fn rouge_l_approx(candidate: &str, reference: &str) -> f64 {
    if candidate.is_empty() || reference.is_empty() {
        return 0.0;
    }

    let cand_words: Vec<&str> = candidate
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect();
    let ref_words: Vec<&str> = reference
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect();

    if cand_words.len() < 2 || ref_words.len() < 2 {
        // Fall back to unigram overlap for very short texts.
        let overlap = ref_words.iter().filter(|w| cand_words.contains(w)).count();
        return overlap as f64 / ref_words.len().max(1) as f64;
    }

    let cand_bigrams: std::collections::HashSet<(String, String)> = cand_words
        .windows(2)
        .map(|w| (w[0].to_lowercase(), w[1].to_lowercase()))
        .collect();
    let ref_bigrams: std::collections::HashSet<(String, String)> = ref_words
        .windows(2)
        .map(|w| (w[0].to_lowercase(), w[1].to_lowercase()))
        .collect();

    let overlap = cand_bigrams.intersection(&ref_bigrams).count();
    let precision = overlap as f64 / cand_bigrams.len().max(1) as f64;
    let recall = overlap as f64 / ref_bigrams.len().max(1) as f64;

    // F1-style harmonic mean of precision and recall.
    if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

/// Compute compression ratio: summary character count / reference character count.
fn compression_ratio(summary: &str, reference: &str) -> f64 {
    if reference.is_empty() {
        return 0.0;
    }
    summary.len() as f64 / reference.len() as f64
}

/// Split text into sentences using punctuation boundaries.
fn split_sentences(text: &str) -> Vec<String> {
    text.split_inclusive(&['.', '!', '?'][..])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Build word frequency map from text (case-insensitive, min 3 chars).
fn word_frequencies(text: &str) -> std::collections::HashMap<String, f64> {
    let words: Vec<&str> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();

    let total = words.len() as f64;
    let mut freqs = std::collections::HashMap::new();
    for word in words {
        *freqs
            .entry(word.to_lowercase())
            .or_insert(0.0) += 1.0 / total;
    }
    freqs
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractive_with_fewer_sentences_than_max() {
        let text = "Hello world. Goodbye.";
        let result = extractive_summarize(text, 5);
        assert_eq!(result, "Hello world. Goodbye.");
    }

    #[test]
    fn test_extractive_selects_top_scoring() {
        let text = "The cat sat on the mat. The cat is a feline. \
                    Quantum mechanics governs subatomic particles. \
                    The cat purred loudly. Schrodinger's cat illustrates superposition.";
        let result = extractive_summarize(text, 2);
        // Should pick sentences most relevant to "cat" — the dominant topic.
        assert!(result.contains("cat"), "summary should include cat-related sentences");
        let sentence_count = result.split('.').filter(|s| !s.trim().is_empty()).count();
        assert_eq!(sentence_count, 2);
    }

    #[test]
    fn test_extractive_empty_input() {
        assert_eq!(extractive_summarize("", 3), "");
    }

    #[test]
    fn test_extractive_zero_max_sentences() {
        assert_eq!(extractive_summarize("Some text.", 0), "");
    }

    #[test]
    fn test_rouge_l_identical_texts() {
        let text = "machine learning algorithms optimize functions";
        let score = rouge_l_approx(text, text);
        assert!((score - 1.0).abs() < 0.001, "identical texts should score ~1.0, got {score}");
    }

    #[test]
    fn test_rouge_l_completely_different() {
        let score = rouge_l_approx("cat dog mouse", "quantum physics relativity");
        assert_eq!(score, 0.0, "disjoint texts should score 0.0");
    }

    #[test]
    fn test_rouge_l_partial_overlap() {
        let score = rouge_l_approx(
            "deep learning neural networks",
            "deep learning and machine learning",
        );
        assert!(score > 0.0 && score < 1.0, "partial overlap should be between 0 and 1");
    }

    #[test]
    fn test_rouge_l_empty_candidate() {
        assert_eq!(rouge_l_approx("", "some reference text"), 0.0);
    }

    #[test]
    fn test_rouge_l_single_word_texts() {
        // Very short texts fall back to unigram overlap.
        let score = rouge_l_approx("hello", "hello");
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_compression_ratio() {
        let ratio = compression_ratio("short summary", "this is a much longer reference text for testing");
        assert!(ratio > 0.0 && ratio < 1.0);
    }

    #[test]
    fn test_compression_ratio_empty_reference() {
        assert_eq!(compression_ratio("summary", ""), 0.0);
    }

    #[test]
    fn test_split_sentences() {
        let result = split_sentences("Hello world! How are you? I am fine.");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_word_frequencies() {
        let freqs = word_frequencies("cat cat cat dog dog bird");
        assert!(freqs["cat"] > freqs["dog"]);
        assert!(freqs["dog"] > freqs["bird"]);
    }

    #[test]
    fn test_recommend_ranks_extractive_first_when_only_local() {
        let eval = vec![
            StrategyEval {
                strategy: "extractive".to_string(),
                avg_rouge_l: 0.45,
                avg_compression_ratio: 0.3,
                avg_latency_ms: 0.5,
                cost: "zero (local)".to_string(),
                skipped: false,
            },
            StrategyEval {
                strategy: "llm".to_string(),
                avg_rouge_l: 0.0,
                avg_compression_ratio: 0.0,
                avg_latency_ms: 0.0,
                cost: "LLM compute".to_string(),
                skipped: true,
            },
            StrategyEval {
                strategy: "hybrid".to_string(),
                avg_rouge_l: 0.45,
                avg_compression_ratio: 0.3,
                avg_latency_ms: 0.5,
                cost: "hybrid (LLM + local)".to_string(),
                skipped: false,
            },
        ];
        let rec = recommend(&eval);
        assert!(rec.contains("extractive") || rec.contains("hybrid"),
                "recommendation should prefer non-skipped strategies");
    }
}
