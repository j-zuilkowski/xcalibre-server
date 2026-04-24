use axum::Router;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "autolibre API",
        version = "0.1.0",
        description = "Self-hosted ebook library manager — REST API",
        license(name = "MIT")
    ),
    paths(
        crate::api::health::health_handler,
        crate::api::auth::login,
        crate::api::auth::refresh,
        crate::api::auth::logout,
        crate::api::auth::totp_setup,
        crate::api::auth::totp_confirm,
        crate::api::auth::totp_verify,
        crate::api::authors::get_author,
        crate::api::authors::patch_author,
        crate::api::authors::list_admin_authors,
        crate::api::authors::merge_author,
        crate::api::books::list_books,
        crate::api::books::get_book,
        crate::api::books::upload_book,
        crate::api::books::patch_book,
        crate::api::books::delete_book,
        crate::api::books::get_cover,
        crate::api::books::download_format,
        crate::api::books::get_reading_progress,
        crate::api::books::upsert_reading_progress,
        crate::api::books::list_annotations,
        crate::api::books::create_annotation,
        crate::api::books::get_chunks,
        crate::api::collections::list_collections,
        crate::api::collections::create_collection,
        crate::api::collections::get_collection,
        crate::api::collections::update_collection,
        crate::api::collections::delete_collection,
        crate::api::collections::add_books_to_collection,
        crate::api::collections::remove_book_from_collection,
        crate::api::collections::search_collection_chunks,
        crate::api::webhooks::list_webhooks,
        crate::api::webhooks::create_webhook,
        crate::api::webhooks::update_webhook,
        crate::api::webhooks::delete_webhook,
        crate::api::webhooks::test_webhook,
        crate::api::search::search_books,
        crate::api::search::search_semantic,
        crate::api::search::search_chunks,
        crate::api::shelves::list_shelves,
        crate::api::shelves::create_shelf,
        crate::api::shelves::get_shelf,
        crate::api::shelves::add_book_to_shelf,
        crate::api::shelves::remove_book_from_shelf,
        crate::api::users::me,
        crate::api::users::me_stats,
        crate::api::users::patch_me,
        crate::api::users::import_goodreads,
        crate::api::users::import_storygraph,
        crate::api::users::get_import_status
    ),
    components(schemas(
        crate::db::models::Book,
        crate::db::queries::books::BookSummary,
        crate::db::queries::collections::CollectionSummary,
        crate::db::queries::collections::CollectionDetail,
        crate::db::models::AuthorRef,
        crate::db::queries::authors::AuthorProfile,
        crate::db::queries::authors::AuthorDetail,
        crate::db::queries::authors::AdminAuthor,
        crate::db::queries::authors::MergeAuthorResponse,
        crate::db::models::TagRef,
        crate::db::models::SeriesRef,
        crate::db::models::FormatRef,
        crate::db::models::Identifier,
        crate::db::models::User,
        crate::db::models::RoleRef,
        crate::db::queries::books::RolePermissions,
        crate::db::models::ReadingProgress,
        crate::db::queries::annotations::Annotation,
        crate::db::queries::book_user_state::BookUserState,
        crate::db::queries::import_logs::ImportErrorEntry,
        crate::db::queries::import_logs::ImportLogRow,
        crate::db::queries::llm::JobRow,
        crate::db::queries::stats::MonthlyCount,
        crate::db::queries::stats::NamedCount,
        crate::db::queries::stats::UserStats,
        crate::db::queries::scheduled_tasks::ScheduledTask,
        crate::api::books::ChunkResponse,
        crate::api::books::ChunksResponse,
        crate::api::search::ChunkSearchResponse,
        crate::api::search::ChunkSearchItem,
        crate::ingest::chunker::ChunkType,
        crate::api::users::ImportJobResponse,
        crate::api::webhooks::CreateWebhookRequest,
        crate::api::webhooks::UpdateWebhookRequest,
        crate::api::webhooks::WebhookResponse,
        crate::api::webhooks::DeleteWebhookResponse,
        crate::webhooks::DeliveryAttemptResult,
        crate::error::AppErrorResponse
    )),
    modifiers(&SecurityAddon),
    security(("bearer_auth" = [])),
    tags(
        (name = "auth", description = "Authentication and session management"),
        (name = "books", description = "Book library management"),
        (name = "collections", description = "Shared book collections and synthesis corpora"),
        (name = "authors", description = "Author profiles and management"),
        (name = "search", description = "Full-text, hybrid chunk, and semantic search"),
        (name = "shelves", description = "Personal reading lists"),
        (name = "reader", description = "Reading progress and annotations"),
        (name = "users", description = "Current user profile operations"),
        (name = "webhooks", description = "User webhook delivery"),
        (name = "health", description = "Service health checks")
    )
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let Some(components) = openapi.components.as_mut() else {
            return;
        };

        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

pub fn openapi_routes(state: crate::AppState) -> Router<crate::AppState> {
    Router::new()
        .merge(SwaggerUi::new("/api/docs").url("/api/docs/openapi.json", ApiDoc::openapi()))
        .with_state(state)
}
