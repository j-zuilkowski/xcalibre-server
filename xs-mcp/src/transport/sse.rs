use crate::tools::CalibreMcpServer;
use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use backend::{
    config::AppConfig,
    db::queries::{api_tokens as api_token_queries, auth as auth_queries},
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
struct SseState {
    db: SqlitePool,
    service: StreamableHttpService<CalibreMcpServer, LocalSessionManager>,
}

pub async fn run_sse_server(
    db: SqlitePool,
    config: AppConfig,
    server: CalibreMcpServer,
    port: u16,
) -> anyhow::Result<()> {
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let state = SseState { db, service };
    let app = Router::new()
        .route("/mcp/sse", get(handle_mcp))
        .route("/mcp/message", post(handle_mcp))
        .with_state(state);

    let bind_addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(
        bind_addr = %bind_addr,
        llm_enabled = config.llm.enabled,
        "starting xs-mcp sse server"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_mcp(State(state): State<SseState>, request: Request) -> Response {
    if let Err(response) = authorize_request(
        &state.db,
        request.headers(),
        request.method(),
        request.uri().path(),
    )
    .await
    {
        return response;
    }

    state
        .service
        .clone()
        .handle(request)
        .await
        .map(axum::body::Body::new)
}

async fn authorize_request(
    db: &SqlitePool,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
) -> Result<(), Response> {
    let bearer = bearer_token(headers).ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())?;
    let token_hash = sha256_hex(bearer);

    let Some(api_token) = api_token_queries::find_by_hash(db, &token_hash)
        .await
        .map_err(internal_error)?
    else {
        return Err(StatusCode::UNAUTHORIZED.into_response());
    };

    api_token_queries::touch_last_used(db, &api_token.id)
        .await
        .map_err(internal_error)?;

    let Some(user) = auth_queries::find_user_by_id(db, &api_token.created_by)
        .await
        .map_err(internal_error)?
    else {
        return Err(StatusCode::UNAUTHORIZED.into_response());
    };

    if !user.is_active {
        return Err(StatusCode::UNAUTHORIZED.into_response());
    }

    if method == Method::GET && path == "/mcp/sse" {
        tracing::info!(
            token_id = %api_token.id,
            token_name = %api_token.name,
            user_id = %user.id,
            "accepted authenticated mcp sse connection"
        );
    }

    Ok(())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (prefix, token) = value.split_once(' ')?;
    if prefix.eq_ignore_ascii_case("bearer") && !token.trim().is_empty() {
        Some(token.trim())
    } else {
        None
    }
}

fn sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(digest)
}

fn internal_error(err: impl std::fmt::Display) -> Response {
    tracing::error!(error = %err, "mcp sse auth failure");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}
