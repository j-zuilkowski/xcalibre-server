#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};

#[tokio::test]
async fn test_search_returns_matching_books() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let target = ctx.create_book("Rust for Systems", "Ada Lovelace").await;
    let _ = ctx.create_book("Cooking for Beginners", "Chef Doe").await;

    let response = ctx
        .server
        .get("/api/v1/search")
        .add_query_param("q", "Rust")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["items"][0]["id"], target.id);
    assert!(body["items"][0]["score"].as_f64().is_some());
}

#[tokio::test]
async fn test_search_empty_query_returns_400() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/search")
        .add_query_param("q", "")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 400);
}

#[tokio::test]
async fn test_suggestions_returns_titles() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let target = ctx.create_book("Rustonomicon", "The Rust Team").await;
    let _ = ctx.create_book("Gardening Almanac", "Green Thumb").await;

    let response = ctx
        .server
        .get("/api/v1/search/suggestions")
        .add_query_param("q", "Rusto")
        .add_query_param("limit", 5)
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let suggestions = body["suggestions"]
        .as_array()
        .expect("suggestions should be array")
        .iter()
        .filter_map(|value| value.as_str().map(ToString::to_string))
        .collect::<Vec<_>>();

    assert!(suggestions.contains(&target.title));
}

#[tokio::test]
async fn test_search_status_reports_fts() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/system/search-status")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["fts"], true);
    assert_eq!(body["meilisearch"], false);
    assert_eq!(body["semantic"], false);
    assert_eq!(body["backend"], "fts5");
}
