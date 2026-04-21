use crate::llm::chat::ChatClient;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum DocumentType {
    Novel,
    Textbook,
    Reference,
    Magazine,
    Datasheet,
    Comic,
    Unknown,
}

impl DocumentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentType::Novel => "novel",
            DocumentType::Textbook => "textbook",
            DocumentType::Reference => "reference",
            DocumentType::Magazine => "magazine",
            DocumentType::Datasheet => "datasheet",
            DocumentType::Comic => "comic",
            DocumentType::Unknown => "unknown",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "novel" => DocumentType::Novel,
            "textbook" => DocumentType::Textbook,
            "reference" => DocumentType::Reference,
            "magazine" => DocumentType::Magazine,
            "datasheet" => DocumentType::Datasheet,
            "comic" => DocumentType::Comic,
            _ => DocumentType::Unknown,
        }
    }
}

pub async fn classify_document_type(
    client: &ChatClient,
    title: &str,
    authors: &str,
    description: &str,
) -> DocumentType {
    let prompt = format!(
        "Classify this book into exactly one category: novel, textbook, reference, magazine, datasheet, comic, or unknown. Title: {title}. Authors: {authors}. Description: {description}. Reply with the single category word only."
    );

    match client.complete(&prompt).await {
        Ok(content) => DocumentType::from_str(content.trim()),
        Err(_) => DocumentType::Unknown,
    }
}
