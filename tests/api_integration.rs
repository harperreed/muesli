use muesli::api::ApiClient;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
    })
    .await
    .unwrap();

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
    })
    .await
    .unwrap();

    assert!(result.is_err());

    if let Err(muesli::Error::Api { status, .. }) = result {
        assert_eq!(status, 403);
    } else {
        panic!("Expected API error");
    }
}

#[tokio::test]
async fn test_get_metadata_with_raw_preserves_unknown_fields() {
    let mock_server = MockServer::start().await;

    // Response includes fields our struct doesn't know about
    let response_body = r#"{
        "id": "doc123",
        "title": "Test Meeting",
        "created_at": "2025-10-28T15:04:05Z",
        "participants": ["Alice"],
        "unknown_api_field": "should be preserved in raw",
        "nested_unknown": {"deep": true},
        "creator": {
            "name": "Alice",
            "email": "alice@acme.com",
            "details": {
                "person": {
                    "name": {"fullName": "Alice Smith"},
                    "employment": {"title": "Engineer"}
                },
                "company": {"name": "Acme Corp"}
            }
        }
    }"#;

    Mock::given(method("POST"))
        .and(path("/v1/get-document-metadata"))
        .and(header("Authorization", "Bearer test_token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();

    let result = tokio::task::spawn_blocking(move || {
        let client = ApiClient::new("test_token".into(), Some(uri))
            .unwrap()
            .disable_throttle();
        client.get_metadata_with_raw("doc123")
    })
    .await
    .unwrap();

    let api_resp = result.unwrap();

    // Parsed struct has the known fields
    assert_eq!(api_resp.parsed.title.as_deref(), Some("Test Meeting"));
    assert!(api_resp.parsed.creator.is_some());
    let creator = api_resp.parsed.creator.as_ref().unwrap();
    assert_eq!(creator.email.as_deref(), Some("alice@acme.com"));

    // Raw string preserves ALL fields including unknown ones
    assert!(api_resp.raw.contains("unknown_api_field"));
    assert!(api_resp.raw.contains("should be preserved in raw"));
    assert!(api_resp.raw.contains("nested_unknown"));
}

#[tokio::test]
async fn test_get_transcript_with_raw() {
    let mock_server = MockServer::start().await;

    let response_body = r#"[
        {
            "document_id": "doc123",
            "speaker": "Alice",
            "start_timestamp": "2025-10-01T21:35:12.500Z",
            "end_timestamp": "2025-10-01T21:35:18.000Z",
            "text": "Hello",
            "extra_field_from_api": 42
        }
    ]"#;

    Mock::given(method("POST"))
        .and(path("/v1/get-document-transcript"))
        .and(header("Authorization", "Bearer test_token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();

    let result = tokio::task::spawn_blocking(move || {
        let client = ApiClient::new("test_token".into(), Some(uri))
            .unwrap()
            .disable_throttle();
        client.get_transcript_with_raw("doc123")
    })
    .await
    .unwrap();

    let api_resp = result.unwrap();

    // Parsed struct works
    assert_eq!(api_resp.parsed.entries.len(), 1);
    assert_eq!(api_resp.parsed.entries[0].text, "Hello");

    // Raw preserves unknown fields
    assert!(api_resp.raw.contains("extra_field_from_api"));
    assert!(api_resp.raw.contains("42"));
}
