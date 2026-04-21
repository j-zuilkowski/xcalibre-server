#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};

#[tokio::test]
async fn test_fts_search_finds_by_title() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let target = ctx
        .create_book("The Rust Programming Language", "Steve Klabnik")
        .await;
    let _ = ctx.create_book("Cooking with Fire", "Chef Doe").await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("q", "Rust")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], target.id);
}

#[tokio::test]
async fn test_fts_search_finds_by_author() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let target = ctx.create_book("Kindred", "Octavia Butler").await;
    let _ = ctx.create_book("Foundation", "Isaac Asimov").await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("q", "Butler")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], target.id);
}

#[tokio::test]
async fn test_fts_search_prefix_match() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let target = ctx.create_book("Dune", "Frank Herbert").await;
    let _ = ctx.create_book("Neuromancer", "William Gibson").await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("q", "Du")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], target.id);
}

#[tokio::test]
async fn test_fts_search_empty_query_returns_all() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let first = ctx
        .create_book("A Wizard of Earthsea", "Ursula Le Guin")
        .await;
    let second = ctx
        .create_book("The Left Hand of Darkness", "Ursula Le Guin")
        .await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("q", "")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 2);
    let ids = body["items"]
        .as_array()
        .expect("items should be array")
        .iter()
        .map(|item| item["id"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert!(ids.contains(&first.id));
    assert!(ids.contains(&second.id));
}
