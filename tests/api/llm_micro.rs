// tests/api/llm_micro.rs
//
// Micro-update endpoints: small, single-purpose mutations designed for LLM
// agents. Each takes a minimal body, touches only what it names, syncs the
// FTS index, and returns the full updated document so operations chain
// without a follow-up GET.

use crate::helpers::spawn_app;

// ── append ──────────────────────────────────────────────────────────

#[tokio::test]
async fn append_joins_with_default_separator_and_updates_search_index() {
    // Arrange
    let app = spawn_app().await;
    let created = app
        .post_json(
            "/rb/documents",
            serde_json::json!({"title": "Journal", "content": "day 1", "tags": ["journal"]}),
        )
        .await
        .json();
    let id = created["id"].as_str().unwrap().to_string();

    // Act
    let response = app
        .post_json(
            &format!("/rb/documents/{id}/append"),
            serde_json::json!({"content": "day 2 notes"}),
        )
        .await;

    // Assert: 200, content joined with a blank line, tags intact
    assert_eq!(response.status, 200);
    let doc = response.json();
    assert_eq!(doc["content"], "day 1\n\nday 2 notes");
    assert_eq!(doc["id"], id.as_str());
    assert_eq!(doc["tags"][0], "journal");

    // The appended text is searchable (FTS synced)
    let search = app
        .post_json("/rb/search", serde_json::json!({"query": "notes"}))
        .await
        .json();
    assert_eq!(search["results"][0]["id"], id.as_str());
}

#[tokio::test]
async fn append_to_empty_stub_sets_content_directly() {
    // Arrange: a title-only stub (content is optional on create)
    let app = spawn_app().await;
    let created = app
        .post_json("/rb/documents", serde_json::json!({"title": "Stub"}))
        .await;
    assert_eq!(created.status, 200);
    let id = created.json()["id"].as_str().unwrap().to_string();

    // Act
    let response = app
        .post_json(
            &format!("/rb/documents/{id}/append"),
            serde_json::json!({"content": "first note"}),
        )
        .await;

    // Assert: no separator in front — the text becomes the content
    assert_eq!(response.status, 200);
    assert_eq!(response.json()["content"], "first note");
}

#[tokio::test]
async fn append_supports_custom_separator() {
    // Arrange
    let app = spawn_app().await;
    let id = app
        .post_json(
            "/rb/documents",
            serde_json::json!({"title": "List", "content": "- a"}),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Act
    let response = app
        .post_json(
            &format!("/rb/documents/{id}/append"),
            serde_json::json!({"content": "- b", "separator": "\n"}),
        )
        .await;

    // Assert
    assert_eq!(response.status, 200);
    assert_eq!(response.json()["content"], "- a\n- b");
}

#[tokio::test]
async fn append_rejects_bad_requests_like_other_routes() {
    // Arrange
    let app = spawn_app().await;

    // Act / Assert: unknown UUID-shaped id → 404
    let response = app
        .post_json(
            "/rb/documents/00000000-0000-0000-0000-000000000000/append",
            serde_json::json!({"content": "x"}),
        )
        .await;
    assert_eq!(response.status, 404);

    // Non-UUID id → 400
    let response = app
        .post_json(
            "/rb/documents/not-a-uuid/append",
            serde_json::json!({"content": "x"}),
        )
        .await;
    assert_eq!(response.status, 400);

    // Missing content → 400
    let id = app
        .post_json(
            "/rb/documents",
            serde_json::json!({"title": "T", "content": "c"}),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let response = app
        .post_json(&format!("/rb/documents/{id}/append"), serde_json::json!({}))
        .await;
    assert_eq!(response.status, 400);
}

// ── add tags ────────────────────────────────────────────────────────

#[tokio::test]
async fn add_tags_unions_without_duplicating() {
    // Arrange
    let app = spawn_app().await;
    let id = app
        .post_json(
            "/rb/documents",
            serde_json::json!({"title": "Doc", "content": "c", "tags": ["rust", "notes"]}),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Act: one new tag, one already present, one repeated within the request
    let response = app
        .post_json(
            &format!("/rb/documents/{id}/tags"),
            serde_json::json!({"tags": ["llm", "rust", "llm"]}),
        )
        .await;

    // Assert: union, no duplicates, existing untouched
    assert_eq!(response.status, 200);
    let tags: Vec<String> = response.json()["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tags.len(), 3);
    for expected in ["rust", "notes", "llm"] {
        assert!(tags.iter().any(|t| t == expected), "tags: {tags:?}");
    }
}

#[tokio::test]
async fn add_tags_creates_tag_on_first_use() {
    // Arrange
    let app = spawn_app().await;
    let id = app
        .post_json(
            "/rb/documents",
            serde_json::json!({"title": "T", "content": "c"}),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Act
    let response = app
        .post_json(
            &format!("/rb/documents/{id}/tags"),
            serde_json::json!({"tags": ["brand-new"]}),
        )
        .await;

    // Assert
    assert_eq!(response.status, 200);
    assert_eq!(response.json()["tags"][0], "brand-new");
}

// ── merge metadata ──────────────────────────────────────────────────

#[tokio::test]
async fn merge_metadata_sets_deletes_and_preserves() {
    // Arrange
    let app = spawn_app().await;
    let id = app
        .post_json(
            "/rb/documents",
            serde_json::json!({
                "title": "Doc",
                "content": "c",
                "metadata": {"priority": "low", "version": 2, "stale": "x"}
            }),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Act: set two keys (one typed), delete one, preserve the rest
    let response = app
        .post_json(
            &format!("/rb/documents/{id}/metadata"),
            serde_json::json!({"priority": "high", "pinned": true, "stale": null}),
        )
        .await;

    // Assert
    assert_eq!(response.status, 200);
    let meta = response.json()["metadata"].clone();
    assert_eq!(meta["priority"], "high");
    assert_eq!(meta["version"], 2, "unlisted key must be preserved");
    assert_eq!(meta["pinned"], true, "typing follows create inference");
    assert!(
        meta.get("stale").is_none(),
        "null must delete the key: {meta}"
    );
}

#[tokio::test]
async fn merge_metadata_on_stub_creates_keys() {
    // Arrange
    let app = spawn_app().await;
    let id = app
        .post_json("/rb/documents", serde_json::json!({"title": "T"}))
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Act
    let response = app
        .post_json(
            &format!("/rb/documents/{id}/metadata"),
            serde_json::json!({"score": 10}),
        )
        .await;

    // Assert
    assert_eq!(response.status, 200);
    assert_eq!(response.json()["metadata"]["score"], 10);
}

// ── get-or-create ───────────────────────────────────────────────────

#[tokio::test]
async fn get_or_create_creates_then_reuses_by_title() {
    // Arrange
    let app = spawn_app().await;

    // Act: first call creates
    let first = app
        .post_json(
            "/rb/documents/get-or-create",
            serde_json::json!({"title": "Daily Journal", "content": "day 1", "tags": ["journal"]}),
        )
        .await;
    assert_eq!(first.status, 201);
    let first_id = first.json()["id"].as_str().unwrap().to_string();

    // Second call with the same title returns the same document, unchanged
    let second = app
        .post_json(
            "/rb/documents/get-or-create",
            serde_json::json!({"title": "Daily Journal", "content": "should be ignored"}),
        )
        .await;
    assert_eq!(second.status, 200);
    let second_doc = second.json();
    assert_eq!(second_doc["id"], first_id.as_str());
    assert_eq!(second_doc["content"], "day 1", "get path must not modify");

    // A different title creates a distinct document
    let third = app
        .post_json(
            "/rb/documents/get-or-create",
            serde_json::json!({"title": "Scratch"}),
        )
        .await;
    assert_eq!(third.status, 201);
    assert_ne!(third.json()["id"], first_id.as_str());

    // Missing title is a 400
    let bad = app
        .post_json("/rb/documents/get-or-create", serde_json::json!({}))
        .await;
    assert_eq!(bad.status, 400);
}

#[tokio::test]
async fn get_or_create_ignores_soft_deleted_titles() {
    // Arrange
    let app = spawn_app().await;
    let id = app
        .post_json(
            "/rb/documents/get-or-create",
            serde_json::json!({"title": "Phoenix", "content": "v1"}),
        )
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.delete(&format!("/rb/documents/{id}")).await;

    // Act: same title after soft delete
    let response = app
        .post_json(
            "/rb/documents/get-or-create",
            serde_json::json!({"title": "Phoenix"}),
        )
        .await;

    // Assert: a new document, not the deleted one
    assert_eq!(response.status, 201);
    let new_id = response.json()["id"].as_str().unwrap().to_string();
    assert_ne!(new_id, id);
    assert_eq!(response.json()["content"], "");
}

// ── relaxed create ──────────────────────────────────────────────────

#[tokio::test]
async fn json_create_allows_title_only_stub() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app
        .post_json("/rb/documents", serde_json::json!({"title": "Idea"}))
        .await;

    // Assert
    assert_eq!(response.status, 200);
    let doc = response.json();
    assert_eq!(doc["content"], "");
}
