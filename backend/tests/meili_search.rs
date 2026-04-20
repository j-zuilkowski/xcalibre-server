#![cfg(feature = "meilisearch")]
#![allow(dead_code, unused_imports)]

mod common;

use std::time::{Duration, Instant};

use axum::http::header;
use axum_test::multipart::{MultipartForm, Part};
use backend::config::AppConfig;
use common::{auth_header, minimal_pdf_bytes, TestContext};
use serde_json::json;
use wiremock::{
    matchers::{body_partial_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn test_meili_backend_falls_back_when_unreachable() {
    let mut config = AppConfig::default();
    config.meilisearch.enabled = true;
    config.meilisearch.url = "http://127.0.0.1:9".to_string();

    let ctx = TestContext::new_with_config(config).await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/system/search-status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["backend"], "fts5");
    assert_eq!(body["meilisearch"], false);
}

#[tokio::test]
async fn test_meili_backend_routes_search_when_available() {
    let meili = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "available"})))
        .mount(&meili)
        .await;

    let mut config = AppConfig::default();
    config.meilisearch.enabled = true;
    config.meilisearch.url = meili.uri();

    let ctx = TestContext::new_with_config(config).await;
    let token = ctx.admin_token().await;
    let target = ctx.create_book("Meili Rust Guide", "Ada Lovelace").await;

    Mock::given(method("POST"))
        .and(path("/indexes/books/search"))
        .and(body_partial_json(json!({
            "q": "Rust",
            "page": 1,
            "hitsPerPage": 24,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "hits": [
                { "id": target.id, "_rankingScore": 0.97 }
            ],
            "estimatedTotalHits": 1,
            "page": 1,
            "hitsPerPage": 24
        })))
        .expect(1)
        .mount(&meili)
        .await;

    let response = ctx
        .server
        .get("/api/v1/search")
        .add_query_param("q", "Rust")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], target.id);
}

#[tokio::test]
async fn test_book_indexed_on_create() {
    let meili = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "available"})))
        .mount(&meili)
        .await;

    Mock::given(method("POST"))
        .and(path("/indexes/books/documents"))
        .and(body_partial_json(json!([
            {
                "title": "Index Title",
                "authors": ["Index Author"],
                "tags": []
            }
        ])))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskUid": 1,
            "status": "enqueued"
        })))
        .mount(&meili)
        .await;

    let mut config = AppConfig::default();
    config.meilisearch.enabled = true;
    config.meilisearch.url = meili.uri();

    let ctx = TestContext::new_with_config(config).await;
    let token = ctx.admin_token().await;

    let metadata = serde_json::json!({
        "title": "Index Title",
        "author": "Index Author"
    })
    .to_string();

    let form = MultipartForm::new()
        .add_part(
            "file",
            Part::bytes(minimal_pdf_bytes())
                .file_name("index-title.pdf")
                .mime_type("application/pdf"),
        )
        .add_text("metadata", metadata);

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .multipart(form)
        .await;

    assert_status!(response, 201);

    wait_for_request_count(&meili, "POST", "/indexes/books/documents", 1).await;
}

#[tokio::test]
async fn test_book_removed_on_delete() {
    let meili = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "available"})))
        .mount(&meili)
        .await;

    let mut config = AppConfig::default();
    config.meilisearch.enabled = true;
    config.meilisearch.url = meili.uri();

    let ctx = TestContext::new_with_config(config).await;
    let token = ctx.admin_token().await;
    let (book, _file_path) = ctx
        .create_book_with_file("Delete Search Index", "EPUB")
        .await;

    let delete_path = format!("/indexes/books/documents/{}", book.id);
    Mock::given(method("DELETE"))
        .and(path(delete_path.clone()))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskUid": 2,
            "status": "enqueued"
        })))
        .mount(&meili)
        .await;

    let response = ctx
        .server
        .delete(&format!("/api/v1/books/{}", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);

    wait_for_request_count(&meili, "DELETE", &delete_path, 1).await;
}

async fn wait_for_request_count(server: &MockServer, method: &str, path: &str, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(3);

    loop {
        let requests = server.received_requests().await.unwrap_or_default();
        let count = requests
            .iter()
            .filter(|request| request.method.as_str() == method && request.url.path() == path)
            .count();

        if count >= expected {
            return;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected} {method} request(s) to {path}; observed {count}"
        );

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
