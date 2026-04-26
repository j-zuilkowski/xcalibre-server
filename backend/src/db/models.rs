//! Shared data-transfer structs that map directly to database rows or are
//! assembled from query results.  These types are used across all query
//! modules and are serialised into API responses.
//!
//! Nullable fields use `Option<T>` (e.g. `Book::description`, `Book::pubdate`,
//! `KoboDevice::sync_token`).  Fields that are always present in the schema
//! are non-optional.  The `Book` struct is the fully-hydrated view (authors,
//! tags, formats, identifiers all loaded); `BookSummary` in `queries/books.rs`
//! is the lighter list projection that avoids the per-book detail queries.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
#[schema(title = "Role")]
pub struct RoleRef {
    pub id: String,
    pub name: String,
}

/// Public view of a user row, safe to include in API responses.
/// Does not carry the password hash, TOTP secret, or lockout state —
/// those are held in `UserAuthRecord` in `queries/auth.rs`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
    pub role: RoleRef,
    pub is_active: bool,
    pub force_pw_reset: bool,
    pub default_library_id: String,
    pub totp_enabled: bool,
    pub created_at: String,
    pub last_modified: String,
}

/// Lightweight author projection embedded in book responses.
/// `sort_name` is the filing form (e.g. "Tolkien, J.R.R.") used for ordering
/// and may differ from `name`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
#[schema(title = "Author")]
pub struct AuthorRef {
    pub id: String,
    pub name: String,
    pub sort_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
#[schema(title = "Series")]
pub struct SeriesRef {
    pub id: String,
    pub name: String,
}

/// Tag projection embedded in book responses.  `confirmed` is `true` when a
/// human (or admin action) has accepted the tag; `false` for LLM-suggested
/// tags awaiting review.
#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
#[schema(title = "Tag")]
pub struct TagRef {
    pub id: String,
    pub name: String,
    pub confirmed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
#[schema(title = "Format")]
pub struct FormatRef {
    pub id: String,
    pub format: String,
    pub size_bytes: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
pub struct Identifier {
    pub id: String,
    pub id_type: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct OauthAccount {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub provider_user_id: String,
    pub email: String,
    pub created_at: String,
}

/// A registered Kobo hardware device.  `sync_token` is the opaque cursor
/// from the last successful delta sync; `None` means the device has never
/// synced or was reassigned to a different user (the token is cleared on
/// ownership change to force a full resync).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct KoboDevice {
    pub id: String,
    pub user_id: String,
    pub device_id: String,
    pub device_name: String,
    pub sync_token: Option<String>,
    pub last_sync_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct KoboReadingState {
    pub id: String,
    pub device_id: String,
    pub book_id: String,
    pub kobo_position: Option<String>,
    pub percent_read: Option<f64>,
    pub last_modified: String,
}

/// Fully-hydrated book record returned by detail endpoints.  `authors`,
/// `tags`, `formats`, and `identifiers` are loaded via separate queries after
/// the main book row is fetched.  `is_read` / `is_archived` default to `false`
/// when `user_id` is not provided (e.g. OPDS or admin context).
/// `indexed_at` is `None` until the semantic indexer has processed the book.
#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub sort_title: String,
    pub description: Option<String>,
    pub pubdate: Option<String>,
    pub language: Option<String>,
    pub rating: Option<i64>,
    pub document_type: String,
    pub series: Option<SeriesRef>,
    pub series_index: Option<f64>,
    pub authors: Vec<AuthorRef>,
    pub tags: Vec<TagRef>,
    pub formats: Vec<FormatRef>,
    pub cover_url: Option<String>,
    pub has_cover: bool,
    pub is_read: bool,
    pub is_archived: bool,
    pub identifiers: Vec<Identifier>,
    pub created_at: String,
    pub last_modified: String,
    pub indexed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
pub struct ReadingProgress {
    pub id: String,
    pub book_id: String,
    pub format_id: String,
    pub cfi: Option<String>,
    pub page: Option<i64>,
    pub percentage: f64,
    pub updated_at: String,
    pub last_modified: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
pub struct Shelf {
    pub id: String,
    pub name: String,
    pub is_public: bool,
    pub book_count: i64,
    pub created_at: String,
    pub last_modified: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, ToSchema)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub domain: String,
    pub is_public: bool,
    pub book_count: i64,
    pub total_chunks: i64,
    pub created_at: String,
    pub updated_at: String,
}
