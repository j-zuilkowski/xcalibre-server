use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct RoleRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
    pub role: RoleRef,
    pub is_active: bool,
    pub force_pw_reset: bool,
    pub default_library_id: String,
    pub created_at: String,
    pub last_modified: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AuthorRef {
    pub id: String,
    pub name: String,
    pub sort_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SeriesRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TagRef {
    pub id: String,
    pub name: String,
    pub confirmed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct FormatRef {
    pub id: String,
    pub format: String,
    pub size_bytes: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Shelf {
    pub id: String,
    pub name: String,
    pub is_public: bool,
    pub book_count: i64,
    pub created_at: String,
    pub last_modified: String,
}
