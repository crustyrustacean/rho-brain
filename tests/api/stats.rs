// tests/api/stats.rs

use crate::helpers::spawn_app;
use serde_json::json;

#[tokio::test]
async fn stats_are_zero_for_empty_database() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get("/rb/stats").await;

    // Assert
    assert!(response.status.is_success());
    assert_eq!(
        response.json(),
        json!({
            "document_count": 0,
            "unique_tags": 0,
            "active_documents": 0,
            "deleted_documents": 0,
        })
    );
}

#[tokio::test]
async fn stats_reflect_created_and_deleted_documents() {
    // Arrange
    let app = spawn_app().await;

    let first = app
        .post_json(
            "/rb/documents",
            json!({"title": "a", "content": "c", "tags": ["x", "y"]}),
        )
        .await
        .json();
    app.post_json(
        "/rb/documents",
        json!({"title": "b", "content": "c", "tags": ["y", "z"]}),
    )
    .await;

    // Act: delete the first document
    let id = first["id"].as_str().unwrap();
    app.delete(&format!("/rb/documents/{id}")).await;

    let response = app.get("/rb/stats").await;

    // Assert
    assert!(response.status.is_success());
    assert_eq!(
        response.json(),
        json!({
            "document_count": 2,
            "unique_tags": 3,
            "active_documents": 1,
            "deleted_documents": 1,
        })
    );
}
