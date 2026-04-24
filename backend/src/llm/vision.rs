use crate::{
    ingest::chunker::ChunkDomain,
    llm::LlmClient,
};

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
