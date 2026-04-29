use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub mod google_books;
pub mod open_library;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetadataCandidate {
    pub source: String,
    pub external_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub published_date: Option<String>,
    pub isbn_13: Option<String>,
    pub isbn_10: Option<String>,
    pub thumbnail_url: Option<String>,
    pub cover_url: Option<String>,
}
