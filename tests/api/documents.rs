// tests/api/documents.rs

use crate::helpers::spawn_app;
use rho_brain::models::{Document, DocumentTag, Metadata, Tag};
use serde_json::json;

#[tokio::test]
async fn create_document_returns_200_with_document() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app
        .post_json(
            "/rb/documents",
            json!({
                "title": "Test Document",
                "content": "This is test content for the knowledge base.",
                "tags": ["test", "sample"],
                "metadata": {"author": "test_user", "version": 1},
            }),
        )
        .await;

    // Assert
    assert!(response.status.is_success());

    let body = response.json();
    let id = body["id"].as_str().unwrap().to_string();

    assert_eq!(body["title"], "Test Document");
    assert_eq!(
        body["content"],
        "This is test content for the knowledge base."
    );
    assert_eq!(body["tags"], json!(["test", "sample"]));
    assert_eq!(body["metadata"]["author"], "test_user");
    assert_eq!(body["metadata"]["version"], 1);

    // Verify the rows landed in the database.
    let mut db = app.db.clone();
    let uuid = uuid::Uuid::parse_str(&id).unwrap();
    let stored = Document::get_by_id(&mut db, &uuid)
        .await
        .expect("Document should exist.");
    assert_eq!(stored.title, "Test Document");

    let doc_tags = DocumentTag::filter_by_document_id(stored.id)
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(doc_tags.len(), 2);

    let metadata = Metadata::filter_by_document_id(stored.id)
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(metadata.len(), 2);
}

#[tokio::test]
async fn create_document_with_existing_tag_reuses_it() {
    // Arrange
    let app = spawn_app().await;
    let request = |title: &str| {
        json!({"title": title, "content": "content", "tags": ["shared"]})
    };

    // Act
    app.post_json("/rb/documents", request("first")).await;
    app.post_json("/rb/documents", request("second")).await;

    // Assert: the tag was not duplicated
    let mut db = app.db.clone();
    let tags = Tag::all().exec(&mut db).await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "shared");
}

#[tokio::test]
async fn list_documents_returns_all_active_documents() {
    // Arrange
    let app = spawn_app().await;
    for title in ["alpha", "beta", "gamma"] {
        app.post_json("/rb/documents", json!({"title": title, "content": "c"}))
            .await;
    }

    // Act
    let response = app.get("/rb/documents").await;

    // Assert
    assert!(response.status.is_success());
    let body = response.json();
    assert_eq!(body["total"], 3);
    assert_eq!(body["documents"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn get_document_returns_404_for_missing_and_deleted() {
    // Arrange
    let app = spawn_app().await;

    // Missing document
    let missing = uuid::Uuid::now_v7();
    let response = app.get(&format!("/rb/documents/{missing}")).await;
    assert_eq!(response.status, 404);

    // Soft-deleted document
    let created = app
        .post_json("/rb/documents", json!({"title": "t", "content": "c"}))
        .await
        .json();
    let id = created["id"].as_str().unwrap();
    app.delete(&format!("/rb/documents/{id}")).await;

    let response = app.get(&format!("/rb/documents/{id}")).await;
    assert_eq!(response.status, 404);
}

#[tokio::test]
async fn get_document_returns_400_for_invalid_id() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get("/rb/documents/not-a-uuid").await;

    // Assert
    assert_eq!(response.status, 400);
}

#[tokio::test]
async fn update_document_changes_fields_and_replaces_tags_and_metadata() {
    // Arrange
    let app = spawn_app().await;
    let created = app
        .post_json(
            "/rb/documents",
            json!({
                "title": "before",
                "content": "original content",
                "tags": ["old"],
                "metadata": {"keep": "no"},
            }),
        )
        .await
        .json();
    let id = created["id"].as_str().unwrap();

    // Act
    let response = app
        .put_json(
            &format!("/rb/documents/{id}"),
            json!({
                "title": "after",
                "tags": ["new", "fresh"],
                "metadata": {"keep": "yes", "count": 2},
            }),
        )
        .await;

    // Assert
    assert!(response.status.is_success());
    let body = response.json();
    assert_eq!(body["title"], "after");
    assert_eq!(body["content"], "original content"); // untouched
    assert_eq!(body["tags"], json!(["new", "fresh"]));
    assert_eq!(body["metadata"], json!({"keep": "yes", "count": 2}));
    assert_ne!(body["updated_at"], body["created_at"]);

    // Verify the database state directly.
    let mut db = app.db.clone();
    let uuid = uuid::Uuid::parse_str(id).unwrap();
    let doc_tags = DocumentTag::filter_by_document_id(uuid)
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(doc_tags.len(), 2);

    let metadata = Metadata::filter_by_document_id(uuid)
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(metadata.len(), 2);
}

#[tokio::test]
async fn delete_document_soft_deletes() {
    // Arrange
    let app = spawn_app().await;
    let created = app
        .post_json("/rb/documents", json!({"title": "t", "content": "c"}))
        .await
        .json();
    let id = created["id"].as_str().unwrap();

    // Act
    let response = app.delete(&format!("/rb/documents/{id}")).await;

    // Assert
    assert!(response.status.is_success());
    assert_eq!(response.json()["deleted"], true);

    // The row still exists, marked deleted.
    let mut db = app.db.clone();
    let uuid = uuid::Uuid::parse_str(id).unwrap();
    let stored = Document::get_by_id(&mut db, &uuid).await.unwrap();
    assert!(stored.deleted_at.is_some());

    // ... and is hidden from the list endpoint.
    let list = app.get("/rb/documents").await.json();
    assert_eq!(list["total"], 0);
}
