#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, TestContext};
use serde_json::{json, Value};

// ────────────────────────────────────────────────────────────────
// POST /api/v1/graph/triples
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_graph_triples_ingest_happy_path() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/graph/triples")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "triples": [
                {
                    "subject": "U4",
                    "predicate": "shares_net",
                    "object": "VCC",
                    "domain_id": "electronics",
                    "session_id": "sess-abc",
                    "confidence": 0.9
                },
                {
                    "subject": "VCC",
                    "predicate": "connects",
                    "object": "C12",
                    "domain_id": "electronics",
                    "session_id": "sess-abc",
                    "confidence": 0.85
                }
            ]
        }))
        .await;
    assert_status!(resp, 201);
    let body: Value = resp.json();
    assert_eq!(body["written"], 2);
}

#[tokio::test]
async fn test_graph_triples_requires_auth() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .server
        .post("/api/v1/graph/triples")
        .json(&json!({ "triples": [] }))
        .await;
    assert_status!(resp, 401);
}

#[tokio::test]
async fn test_graph_triples_empty_array_succeeds() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/graph/triples")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({ "triples": [] }))
        .await;
    assert_status!(resp, 201);
    let body: Value = resp.json();
    assert_eq!(body["written"], 0);
}

#[tokio::test]
async fn test_graph_triples_missing_required_field_returns_422() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    // Missing "object"
    let resp = ctx
        .server
        .post("/api/v1/graph/triples")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "triples": [{ "subject": "A", "predicate": "calls" }]
        }))
        .await;
    assert_status!(resp, 422);
}

// ────────────────────────────────────────────────────────────────
// GET /api/v1/graph/traverse
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_graph_traverse_returns_direct_edges() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    // Seed two triples
    ctx.server
        .post("/api/v1/graph/triples")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "triples": [
                { "subject": "FnA", "predicate": "calls", "object": "FnB",
                  "domain_id": "software", "session_id": "s1", "confidence": 0.9 },
                { "subject": "FnB", "predicate": "calls", "object": "FnC",
                  "domain_id": "software", "session_id": "s1", "confidence": 0.9 }
            ]
        }))
        .await;

    let resp = ctx
        .server
        .get("/api/v1/graph/traverse?anchor=FnA&hops=1")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    let triples = body["triples"].as_array().expect("triples array");
    // hops=1: should see FnA→FnB but not FnB→FnC
    assert!(
        triples.iter().any(|t| t["subject"] == "FnA" && t["object"] == "FnB"),
        "expected FnA->FnB edge at hops=1"
    );
    assert!(
        !triples.iter().any(|t| t["subject"] == "FnB" && t["object"] == "FnC"),
        "FnB->FnC should not appear at hops=1"
    );
}

#[tokio::test]
async fn test_graph_traverse_two_hops_reaches_second_level() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    ctx.server
        .post("/api/v1/graph/triples")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "triples": [
                { "subject": "FnA", "predicate": "calls", "object": "FnB",
                  "domain_id": "software", "session_id": "s2", "confidence": 0.9 },
                { "subject": "FnB", "predicate": "calls", "object": "FnC",
                  "domain_id": "software", "session_id": "s2", "confidence": 0.9 }
            ]
        }))
        .await;

    let resp = ctx
        .server
        .get("/api/v1/graph/traverse?anchor=FnA&hops=2")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    let triples = body["triples"].as_array().expect("triples array");
    assert!(
        triples.iter().any(|t| t["subject"] == "FnB" && t["object"] == "FnC"),
        "FnB->FnC should appear at hops=2"
    );
}

#[tokio::test]
async fn test_graph_traverse_domain_filter() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    ctx.server
        .post("/api/v1/graph/triples")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "triples": [
                { "subject": "turmeric", "predicate": "substitutes_for", "object": "saffron",
                  "domain_id": "culinary", "session_id": "s3", "confidence": 0.8 },
                { "subject": "U4", "predicate": "shares_net", "object": "VCC",
                  "domain_id": "electronics", "session_id": "s3", "confidence": 0.9 }
            ]
        }))
        .await;

    let resp = ctx
        .server
        .get("/api/v1/graph/traverse?anchor=turmeric&hops=1&domain_id=culinary")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    let triples = body["triples"].as_array().expect("triples array");
    assert!(
        triples.iter().all(|t| t["domain_id"] == "culinary"),
        "only culinary triples should appear when domain_id=culinary"
    );
    assert!(
        !triples.iter().any(|t| t["subject"] == "U4"),
        "electronics triples must be filtered out"
    );
}

#[tokio::test]
async fn test_graph_traverse_empty_graph_returns_empty_array() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/graph/traverse?anchor=nonexistent&hops=2")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    assert!(body["triples"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_graph_traverse_hops_cap_enforced() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    // hops=99 — server should clamp to max_hops (3) and not error
    let resp = ctx
        .server
        .get("/api/v1/graph/traverse?anchor=A&hops=99")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
}

#[tokio::test]
async fn test_graph_traverse_requires_auth() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .server
        .get("/api/v1/graph/traverse?anchor=A&hops=1")
        .await;
    assert_status!(resp, 401);
}

// ────────────────────────────────────────────────────────────────
// GET /api/v1/search/enriched
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_search_enriched_returns_chunks_and_graph() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    // Seed a book and a triple
    ctx.seed_book("Thermal Management Guide").await;
    ctx.server
        .post("/api/v1/graph/triples")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "triples": [
                { "subject": "U4", "predicate": "requires", "object": "thermal_relief",
                  "domain_id": "electronics", "session_id": "s4", "confidence": 0.9 }
            ]
        }))
        .await;

    let resp = ctx
        .server
        .get("/api/v1/search/enriched?q=thermal+management&hops=1")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    // Must have both keys even if graph is empty for this query
    assert!(body["chunks"].is_array(), "enriched response must have chunks array");
    assert!(body["graph"]["triples"].is_array(), "enriched response must have graph.triples array");
}

#[tokio::test]
async fn test_search_enriched_requires_auth() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .server
        .get("/api/v1/search/enriched?q=test")
        .await;
    assert_status!(resp, 401);
}

#[tokio::test]
async fn test_search_enriched_missing_q_returns_400() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/search/enriched")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 400);
}
