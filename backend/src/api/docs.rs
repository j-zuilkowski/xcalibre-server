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
        crate::api::search::search_books,
        crate::api::search::search_semantic,
        crate::api::shelves::list_shelves,
        crate::api::shelves::create_shelf,
        crate::api::shelves::get_shelf,
        crate::api::shelves::add_book_to_shelf,
        crate::api::shelves::remove_book_from_shelf,
        crate::api::users::me,
        crate::api::users::patch_me
    ),
    components(schemas(
        crate::db::models::Book,
        crate::db::queries::books::BookSummary,
        crate::db::models::AuthorRef,
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
        crate::db::queries::llm::JobRow,
        crate::db::queries::scheduled_tasks::ScheduledTask,
        crate::error::AppErrorResponse
    )),
    modifiers(&SecurityAddon),
    security(("bearer_auth" = [])),
    tags(
        (name = "auth", description = "Authentication and session management"),
        (name = "books", description = "Book library management"),
        (name = "search", description = "Full-text and semantic search"),
        (name = "shelves", description = "Personal reading lists"),
        (name = "reader", description = "Reading progress and annotations"),
        (name = "users", description = "Current user profile operations"),
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
