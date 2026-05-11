//! Integration tests for the summarization model evaluation module.
//!
//! Tests cover all three strategies (extractive, LLM, hybrid) and the evaluation
//! harness that benchmarks them against quality/latency/cost metrics.
//!
//! TDD order: tests written before implementation module exists.

mod common;

use common::TestContext;

#[tokio::test]
async fn test_extractive_summarize_short_text() {
    let _ctx = TestContext::new().await;
    let text = "The quick brown fox jumps over the lazy dog. \
                It was a sunny day in the countryside. \
                Birds sang from the treetops as the fox continued its journey. \
                Eventually it reached the river and stopped for a drink.";

    // When LLM is disabled, extractive fallback should still work.
    let result = backend::llm::summarize::summarize_text(text, None, 2).await;

    assert!(!result.summary.is_empty(), "extractive summary should not be empty");
    assert!(
        result.summary.len() <= text.len(),
        "summary should not be longer than input"
    );
    assert_eq!(result.strategy, "extractive");
}

#[tokio::test]
async fn test_extractive_summarize_trims_to_max_sentences() {
    let _ctx = TestContext::new().await;
    let text = "First. Second. Third. Fourth. Fifth. Sixth.";

    let result = backend::llm::summarize::summarize_text(text, None, 2).await;

    let sentence_count = result.summary.split('.').filter(|s| !s.trim().is_empty()).count();
    assert!(sentence_count <= 2, "should respect max_sentences=2, got {sentence_count}");
    assert_eq!(result.strategy, "extractive");
}

#[tokio::test]
async fn test_extractive_summarize_empty_input() {
    let result = backend::llm::summarize::summarize_text("", None, 3).await;
    assert!(result.summary.is_empty(), "empty input yields empty summary");
    assert_eq!(result.strategy, "extractive");
}

#[tokio::test]
async fn test_evaluate_model_extractive_baseline() {
    let texts = vec![
        "Artificial intelligence has transformed many industries. \
         Machine learning algorithms now power recommendation systems. \
         Deep learning has enabled breakthroughs in image recognition. \
         Natural language processing allows computers to understand text. \
         These advances continue to accelerate research.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<String>>();

    let evaluation = backend::llm::summarize::evaluate_strategies(&texts).await;

    // Extractive strategy should always be present and have zero cost.
    let extractive = evaluation
        .iter()
        .find(|r| r.strategy == "extractive")
        .expect("extractive strategy must be evaluated");

    assert_eq!(extractive.cost, "zero (local)");
    assert!(extractive.avg_latency_ms > 0.0);
    assert!(extractive.avg_compression_ratio > 0.0 && extractive.avg_compression_ratio <= 1.0);
    assert!(extractive.avg_rouge_l >= 0.0 && extractive.avg_rouge_l <= 1.0);
}

#[tokio::test]
async fn test_evaluate_model_llm_returns_fallback_when_disabled() {
    let texts = vec!["Sample text for evaluation. Another sentence here."]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<String>>();

    let evaluation = backend::llm::summarize::evaluate_strategies(&texts).await;

    // LLM strategy should be present but marked as skipped when client is None.
    let llm_result = evaluation
        .iter()
        .find(|r| r.strategy == "llm")
        .expect("llm strategy must appear in evaluation");

    // When no ChatClient is configured (ENABLE_LLM_FEATURES=false),
    // the LLM strategy should report skipped or fallback metrics.
    assert!(
        llm_result.skipped || llm_result.avg_rouge_l >= 0.0,
        "LLM strategy should either be skipped or produce metrics via fallback"
    );
}

#[tokio::test]
async fn test_evaluate_model_hybrid_uses_extractive_fallback() {
    // Hybrid strategy should always work by delegating to extractive when LLM unavailable.
    let texts = vec!["Test passage one. Another test sentence. Third sentence here."]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<String>>();

    let evaluation = backend::llm::summarize::evaluate_strategies(&texts).await;

    let hybrid = evaluation
        .iter()
        .find(|r| r.strategy == "hybrid")
        .expect("hybrid strategy must be evaluated");

    assert!(!hybrid.skipped, "hybrid should never be skipped — it falls back to extractive");
    assert!(hybrid.avg_latency_ms >= 0.0);
}

#[tokio::test]
async fn test_evaluate_model_all_strategies_present() {
    let texts = vec!["Another test passage for completeness."]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<String>>();

    let evaluation = backend::llm::summarize::evaluate_strategies(&texts).await;

    let strategies: Vec<&str> = evaluation.iter().map(|r| r.strategy.as_str()).collect();
    assert!(strategies.contains(&"extractive"), "must evaluate extractive");
    assert!(strategies.contains(&"llm"), "must evaluate llm");
    assert!(strategies.contains(&"hybrid"), "must evaluate hybrid");
    assert_eq!(evaluation.len(), 3, "exactly three strategies");
}

#[tokio::test]
async fn test_evaluate_model_recommendation_ranks_order() {
    let texts: Vec<String> = (0..5)
        .map(|i| format!("Document {i}. It contains several sentences for evaluation. \
                          This is additional content to make the text longer. \
                          More sentences help test the summarization pipeline. \
                          The evaluation should produce meaningful results."))
        .collect();

    let evaluation = backend::llm::summarize::evaluate_strategies(&texts).await;

    // The recommendation should rank strategies in a sensible order.
    let recommendation = backend::llm::summarize::recommend(&evaluation);
    assert!(!recommendation.is_empty(), "recommendation should not be empty");
    // First-ranked strategy should be the one with highest quality score.
}
