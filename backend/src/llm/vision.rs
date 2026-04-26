//! Vision LLM pass for image-heavy document pages.
//!
//! Sends a page image to the configured LLM (must support vision / multimodal input)
//! and returns a textual description that is appended to the chunk text before indexing.
//!
//! # Domain-aware prompting
//! The prompt is tailored by [`ChunkDomain`]:
//! - `Technical` / `Electronics`: exhaustive component/net description for RAG fidelity.
//! - All other domains: generic image description covering text, layout, and meaning.
//!
//! # Gating
//! Callers (in `ingest/text.rs`) must check both:
//! 1. `chunk.is_image_heavy == true` (PDF page has fewer than 80 words)
//! 2. `chat_client.supports_vision().await == true`
//!
//! If either check fails, the vision pass is skipped and the chunk text is stored as-is.
//! The vision pass is best-effort — a failure logs a warning and does not abort ingest.

use crate::{ingest::chunker::ChunkDomain, llm::LlmClient};

/// Describe the visual content of a page image using the vision LLM.
///
/// `page_image_bytes` may be JPEG or PNG; the MIME type is auto-detected from the
/// magic bytes. Returns the description string or an error if the LLM response is empty.
///
/// # Errors
/// Returns `Err` when the LLM call fails or returns an empty response.
/// The caller logs the error as a warning and continues ingest without the description.
pub async fn describe_image_page(
    llm: &LlmClient,
    page_image_bytes: &[u8],
    domain: &ChunkDomain,
) -> anyhow::Result<String> {
    let prompt = match domain {
        ChunkDomain::Technical | ChunkDomain::Electronics => {
            "You are analyzing a page from a technical document. Describe everything you see: all text (component labels, values, net names, annotations), the circuit or diagram topology (what connects to what), the function of the circuit or diagram, and any design notes. Be precise and complete. Include all component reference designators, values, and units."
        }
        _ => {
            "Describe the content of this image completely. Include all text visible in the image, the structure or layout of any diagram or chart, and the meaning or function it communicates."
        }
    };

    let mime_type = detect_image_mime_type(page_image_bytes);
    let response = llm
        .complete_with_image(prompt, page_image_bytes, mime_type)
        .await?;

    let response = response.trim().to_string();
    if response.is_empty() {
        anyhow::bail!("vision response was empty");
    }

    Ok(response)
}

fn detect_image_mime_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else {
        "image/png"
    }
}
