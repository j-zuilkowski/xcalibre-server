#![allow(dead_code, unused_imports)]

mod common;

#[tokio::test]
#[ignore]
async fn test_register_first_user_becomes_admin() { todo!() }

#[tokio::test]
#[ignore]
async fn test_register_fails_if_users_exist() { todo!() }

#[tokio::test]
#[ignore]
async fn test_login_success_returns_tokens() { todo!() }

#[tokio::test]
#[ignore]
async fn test_login_wrong_password_returns_401() { todo!() }

#[tokio::test]
#[ignore]
async fn test_login_nonexistent_user_returns_401() { todo!() }

#[tokio::test]
#[ignore]
async fn test_login_lockout_after_max_attempts() { todo!() }

#[tokio::test]
#[ignore]
async fn test_login_lockout_resets_after_duration() { todo!() }

#[tokio::test]
#[ignore]
async fn test_refresh_token_returns_new_pair() { todo!() }

#[tokio::test]
#[ignore]
async fn test_refresh_token_revoked_after_use() { todo!() }

#[tokio::test]
#[ignore]
async fn test_refresh_invalid_token_returns_401() { todo!() }

#[tokio::test]
#[ignore]
async fn test_logout_revokes_refresh_token() { todo!() }

#[tokio::test]
#[ignore]
async fn test_me_returns_current_user() { todo!() }

#[tokio::test]
#[ignore]
async fn test_me_without_token_returns_401() { todo!() }

#[tokio::test]
#[ignore]
async fn test_me_with_expired_token_returns_401() { todo!() }

#[tokio::test]
#[ignore]
async fn test_change_password_success() { todo!() }

#[tokio::test]
#[ignore]
async fn test_change_password_wrong_current_returns_400() { todo!() }

