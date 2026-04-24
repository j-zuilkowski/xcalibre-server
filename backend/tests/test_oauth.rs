#![allow(dead_code, unused_imports)]

mod common;

use std::{
    net::{IpAddr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, Request},
    http::{header, HeaderName, HeaderValue},
    middleware::Next,
    Router,
};
use axum_test::TestServer;
use backend::{app, config::AppConfig, AppState};
use common::{test_db, TestContext, TEST_JWT_SECRET};
use serde_json::json;
use tempfile::TempDir;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn app_with_connect_info(state: AppState, remote_ip: IpAddr) -> Router {
    app(state).layer(axum::middleware::from_fn(
        move |mut req: Request, next: Next| {
            let connect_info = ConnectInfo(SocketAddr::new(remote_ip, 12_345));
            async move {
                req.extensions_mut().insert(connect_info);
                next.run(req).await
            }
        },
    ))
}

async fn oauth_context(remote_ip: IpAddr, mock_server: &MockServer) -> TestContext {
    let storage = TempDir::new().expect("tempdir");
    let db = test_db().await;
    std::env::set_var("AUTOLIBRE_DISABLE_METRICS", "1");

    let mut config = AppConfig::default();
    config.app.storage_path = storage.path().to_string_lossy().to_string();
    config.auth.jwt_secret = TEST_JWT_SECRET.to_string();
    config.oauth.github.client_id = "client-id".to_string();
    config.oauth.github.client_secret = "client-secret".to_string();
    config.oauth.github.authorization_url = format!("{}/authorize", mock_server.uri());
    config.oauth.github.token_url = format!("{}/token", mock_server.uri());
    config.oauth.github.userinfo_url = format!("{}/user", mock_server.uri());
    config.oauth.github.email_url = format!("{}/emails", mock_server.uri());
    config.oauth.github.scope = "read:user user:email".to_string();

    let state = AppState::new(db.clone(), config)
        .await
        .expect("initialize app state");
    let server = TestServer::new(app_with_connect_info(state.clone(), remote_ip))
        .expect("build test server");

    TestContext {
        db,
        storage,
        server,
        state,
    }
}

fn oauth_state_cookie_value(set_cookie: &str) -> String {
    set_cookie
        .strip_prefix("oauth_state=")
        .and_then(|value| value.split(';').next())
        .expect("oauth state cookie")
        .to_string()
}

fn oauth_state_from_location(location: &str) -> String {
    let url = reqwest::Url::parse(location).expect("parse redirect url");
    url.query_pairs()
        .find_map(|(name, value)| (name == "state").then_some(value.to_string()))
        .expect("oauth state query param")
}

async fn setup_provider(mock_server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "provider-access-token"
        })))
        .mount(mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 12345,
            "login": "oauth-user",
            "email": "oauth-user@example.com"
        })))
        .mount(mock_server)
        .await;
}

async fn start_oauth_flow(ctx: &TestContext) -> (String, String) {
    let response = ctx.server.get("/api/v1/auth/oauth/github").await;
    assert_status!(response, 302);

    let set_cookie = response
        .header(header::SET_COOKIE)
        .to_str()
        .expect("set-cookie header")
        .to_string();
    let location = response
        .header(header::LOCATION)
        .to_str()
        .expect("location header")
        .to_string();

    (oauth_state_cookie_value(&set_cookie), oauth_state_from_location(&location))
}

#[tokio::test]
async fn test_oauth_state_valid_from_same_ip_succeeds() {
    let mock_server = MockServer::start().await;
    setup_provider(&mock_server).await;
    let ctx = oauth_context("198.51.100.10".parse().expect("ip"), &mock_server).await;

    let (cookie_nonce, state_token) = start_oauth_flow(&ctx).await;
    assert!(!cookie_nonce.contains('.'));
    assert!(state_token.starts_with(&format!("{cookie_nonce}.")));

    let response = ctx
        .server
        .get("/api/v1/auth/oauth/github/callback")
        .add_query_param("code", "authorization-code")
        .add_query_param("state", state_token)
        .add_header(
            header::COOKIE,
            HeaderValue::from_str(&format!("oauth_state={cookie_nonce}")).expect("cookie header"),
        )
        .await;

    assert_status!(response, 302);
    let location_header = response.header(header::LOCATION);
    let location = location_header.to_str().expect("location header");
    assert_eq!(location, "/");
}

#[tokio::test]
async fn test_oauth_state_valid_nonce_but_tampered_ip_returns_400() {
    let mock_server = MockServer::start().await;
    setup_provider(&mock_server).await;
    let start_ctx = oauth_context("198.51.100.11".parse().expect("ip"), &mock_server).await;
    let callback_ctx = oauth_context("198.51.100.12".parse().expect("ip"), &mock_server).await;

    let (cookie_nonce, state_token) = start_oauth_flow(&start_ctx).await;

    let response = callback_ctx
        .server
        .get("/api/v1/auth/oauth/github/callback")
        .add_query_param("code", "authorization-code")
        .add_query_param("state", state_token)
        .add_header(
            header::COOKIE,
            HeaderValue::from_str(&format!("oauth_state={cookie_nonce}")).expect("cookie header"),
        )
        .await;

    assert_status!(response, 400);
}

#[tokio::test]
async fn test_oauth_state_tampered_mac_returns_400() {
    let mock_server = MockServer::start().await;
    setup_provider(&mock_server).await;
    let ctx = oauth_context("198.51.100.13".parse().expect("ip"), &mock_server).await;

    let (cookie_nonce, state_token) = start_oauth_flow(&ctx).await;
    let mut tampered_state = state_token.clone();
    let last = tampered_state.pop().expect("state token char");
    tampered_state.push(if last == '0' { '1' } else { '0' });

    let response = ctx
        .server
        .get("/api/v1/auth/oauth/github/callback")
        .add_query_param("code", "authorization-code")
        .add_query_param("state", tampered_state)
        .add_header(
            header::COOKIE,
            HeaderValue::from_str(&format!("oauth_state={cookie_nonce}")).expect("cookie header"),
        )
        .await;

    assert_status!(response, 400);
}
