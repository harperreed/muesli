// ABOUTME: Integration tests for the Granola API client layer
// ABOUTME: Uses wiremock to verify HTTP request/response handling for documents, notes, and metadata

use muesli::api::ApiClient;
use wiremock::matchers::{body_partial_json, header, method, path};
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

/// Test A: list_documents_with_notes returns ProseMirror data in last_viewed_panel
#[tokio::test]
async fn test_list_documents_with_notes_returns_prosemirror() {
    let mock_server = MockServer::start().await;

    let response = serde_json::json!({
        "docs": [
            {
                "id": "doc-notes-1",
                "title": "Meeting with Notes",
                "created_at": "2025-11-01T10:00:00Z",
                "last_viewed_panel": {
                    "type": "doc",
                    "content": [
                        {
                            "type": "heading",
                            "attrs": {"level": 2},
                            "content": [
                                {"type": "text", "text": "Action Items"}
                            ]
                        },
                        {
                            "type": "paragraph",
                            "content": [
                                {"type": "text", "text": "Follow up on "},
                                {"type": "text", "text": "deployment", "marks": [{"type": "bold"}]}
                            ]
                        }
                    ]
                }
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

    let result = tokio::task::spawn_blocking(move || {
        let client = ApiClient::new("test_token".into(), Some(uri))
            .unwrap()
            .disable_throttle();
        client.list_documents_with_notes()
    })
    .await
    .unwrap();

    let docs = result.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].id, "doc-notes-1");

    // Verify ProseMirror data is populated
    let panel = docs[0]
        .last_viewed_panel
        .as_ref()
        .expect("last_viewed_panel should be populated");
    assert_eq!(panel.node_type, "doc");

    let content = panel.content.as_ref().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].node_type, "heading");
    assert_eq!(content[1].node_type, "paragraph");

    // Verify the text content is accessible
    let heading_text = content[0].content.as_ref().unwrap();
    assert_eq!(heading_text[0].text.as_deref(), Some("Action Items"));
}

/// Test B: PublicNote model parsing (since get_public_note uses hardcoded URL)
#[test]
fn test_public_note_model_parsing_with_summary() {
    let json = r#"{
        "id": "note-integration-test",
        "title": "Sprint Retrospective",
        "summary_text": "Team discussed velocity improvements and identified three key blockers.",
        "participants": ["Alice", "Bob"],
        "unknown_future_field": {"nested": true}
    }"#;

    let note: muesli::PublicNote = serde_json::from_str(json).unwrap();
    assert_eq!(note.id, "note-integration-test");
    assert_eq!(note.title.as_deref(), Some("Sprint Retrospective"));
    assert_eq!(
        note.summary_text.as_deref(),
        Some("Team discussed velocity improvements and identified three key blockers.")
    );
}

/// Test B (continued): PublicNote with null summary
#[test]
fn test_public_note_model_parsing_without_summary() {
    let json = r#"{"id": "note-no-summary", "title": "Quick Sync"}"#;

    let note: muesli::PublicNote = serde_json::from_str(json).unwrap();
    assert_eq!(note.id, "note-no-summary");
    assert_eq!(note.title.as_deref(), Some("Quick Sync"));
    assert!(note.summary_text.is_none());
}

/// Test C: list_documents_with_notes sends include_last_viewed_panel in request body
#[tokio::test]
async fn test_list_documents_with_notes_sends_correct_body() {
    let mock_server = MockServer::start().await;

    let response = serde_json::json!({
        "docs": []
    });

    // Use body_partial_json to verify the request body includes include_last_viewed_panel: true
    Mock::given(method("POST"))
        .and(path("/v2/get-documents"))
        .and(header("Authorization", "Bearer test_token"))
        .and(body_partial_json(
            serde_json::json!({"include_last_viewed_panel": true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();

    let result = tokio::task::spawn_blocking(move || {
        let client = ApiClient::new("test_token".into(), Some(uri))
            .unwrap()
            .disable_throttle();
        client.list_documents_with_notes()
    })
    .await
    .unwrap();

    let docs = result.unwrap();
    assert!(docs.is_empty());
    // The mock's expect(1) will verify the request was received with the correct body.
    // If the body didn't match, the mock wouldn't have been triggered and the request
    // would have gotten a 404.
}
