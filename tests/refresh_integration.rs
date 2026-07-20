// ABOUTME: Integration tests for the Granola refresh-token auth flow
// ABOUTME: Uses wiremock to verify token exchange, rotation persistence, and error handling

use muesli::refresh::try_refresh_token;
use std::path::Path;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn write_auth(dir: &Path, refresh_token: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("auth.json"),
        format!(r#"{{"refresh_token":"{refresh_token}"}}"#),
    )
    .unwrap();
}

fn stored_refresh_token(dir: &Path) -> String {
    let content = std::fs::read_to_string(dir.join("auth.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();
    value["refresh_token"].as_str().unwrap().to_string()
}

/// A stored refresh token is exchanged for an access token, and the rotated
/// refresh token from the response is persisted for next time.
#[tokio::test]
async fn test_refresh_exchanges_and_rotates_token() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/refresh-access-token"))
        .and(body_partial_json(
            serde_json::json!({ "refresh_token": "rt_old" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "at_fresh",
            "refresh_token": "rt_new",
            "expires_in": 21600,
            "token_type": "Bearer"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();
    let temp = tempfile::TempDir::new().unwrap();
    let dir = temp.path().to_path_buf();
    write_auth(&dir, "rt_old");

    let (token, rotated) = tokio::task::spawn_blocking(move || {
        let token = try_refresh_token(&uri, Some(&dir)).unwrap();
        (token, stored_refresh_token(&dir))
    })
    .await
    .unwrap();

    assert_eq!(token.as_deref(), Some("at_fresh"));
    assert_eq!(
        rotated, "rt_new",
        "rotated refresh token should be persisted"
    );
}

/// When the response omits a new refresh token, the existing one is retained.
#[tokio::test]
async fn test_refresh_without_rotation_keeps_existing_token() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/refresh-access-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "at_fresh",
            "expires_in": 21600,
            "token_type": "Bearer"
        })))
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();
    let temp = tempfile::TempDir::new().unwrap();
    let dir = temp.path().to_path_buf();
    write_auth(&dir, "rt_keep");

    let (token, retained) = tokio::task::spawn_blocking(move || {
        let token = try_refresh_token(&uri, Some(&dir)).unwrap();
        (token, stored_refresh_token(&dir))
    })
    .await
    .unwrap();

    assert_eq!(token.as_deref(), Some("at_fresh"));
    assert_eq!(retained, "rt_keep");
}

/// A persisted (authoritative) refresh token that the server rejects surfaces
/// an error rather than silently falling through.
#[tokio::test]
async fn test_persisted_token_rejection_is_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/refresh-access-token"))
        .respond_with(ResponseTemplate::new(401).set_body_string(r#"{"error":"logout_user"}"#))
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();
    let temp = tempfile::TempDir::new().unwrap();
    let dir = temp.path().to_path_buf();
    write_auth(&dir, "rt_dead");

    let result = tokio::task::spawn_blocking(move || try_refresh_token(&uri, Some(&dir)))
        .await
        .unwrap();

    assert!(
        matches!(result, Err(muesli::Error::Auth(_))),
        "revoked persisted token should be an Auth error, got {result:?}"
    );
}
