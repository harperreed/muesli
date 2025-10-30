use muesli::api::ApiClient;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn test_list_documents_success() {
    let mock_server = MockServer::start().await;

    let response = serde_json::json!({
        "docs": [
            {
                "id": "doc123",
                "title": "Test Meeting",
                "created_at": "2025-10-28T15:04:05Z",
                "updated_at": "2025-10-29T01:23:45Z"
            }
        ]
    });

    Mock::given(method("POST"))
        .and(path("/v2/get-documents"))
        .and(header("Authorization", "Bearer test_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();

    // Run blocking client in a blocking context
    let result = tokio::task::spawn_blocking(move || {
        let client = ApiClient::new("test_token".into(), Some(uri))
            .unwrap()
            .disable_throttle();
        client.list_documents()
    }).await.unwrap();

    let docs = result.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].id, "doc123");
}

#[tokio::test]
async fn test_api_error_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/get-documents"))
        .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();

    // Run blocking client in a blocking context
    let result = tokio::task::spawn_blocking(move || {
        let client = ApiClient::new("bad_token".into(), Some(uri))
            .unwrap()
            .disable_throttle();
        client.list_documents()
    }).await.unwrap();

    assert!(result.is_err());

    if let Err(muesli::Error::Api { status, .. }) = result {
        assert_eq!(status, 403);
    } else {
        panic!("Expected API error");
    }
}
