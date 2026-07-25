// tests/api/search.rs

use crate::helpers::spawn_app;
use serde_json::json;

async fn seed(app: &crate::helpers::TestApp) {
    app.post_json(
        "/rb/documents",
        json!({
            "title": "Rust patterns",
            "content": "Ownership and borrowing explained.",
            "tags": ["rust", "programming"],
        }),
    )
    .await;
    app.post_json(
        "/rb/documents",
        json!({
            "title": "Cooking pasta",
            "content": "Salt the water like the sea.",
            "tags": ["cooking"],
        }),
    )
    .await;
}

#[tokio::test]
async fn search_matches_title_and_content() {
    // Arrange
    let app = spawn_app().await;
    seed(&app).await;

    // Act: title match
    let by_title = app.get("/rb/search?q=Rust").await.json();
    assert_eq!(by_title["results"].as_array().unwrap().len(), 1);
    assert_eq!(by_title["results"][0]["title"], "Rust patterns");

    // Act: content match
    let by_content = app.get("/rb/search?q=water").await.json();
    assert_eq!(by_content["results"].as_array().unwrap().len(), 1);
    assert_eq!(by_content["results"][0]["title"], "Cooking pasta");

    // Act: no match
    let no_match = app.get("/rb/search?q=zzz-not-found").await.json();
    assert_eq!(no_match["results"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn search_get_requires_query_param() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get("/rb/search").await;

    // Assert
    assert_eq!(response.status, 400);
}

#[tokio::test]
async fn search_post_filters_by_tags() {
    // Arrange
    let app = spawn_app().await;
    seed(&app).await;

    // Act
    let response = app
        .post_json(
            "/rb/search",
            json!({"query": "e", "tags": ["cooking"]}),
        )
        .await;

    // Assert: both documents contain "e", but only one is tagged "cooking"
    assert!(response.status.is_success());
    let body = response.json();
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["title"], "Cooking pasta");
    assert_eq!(results[0]["tags"], json!(["cooking"]));
}

#[tokio::test]
async fn search_post_with_unknown_tag_matches_nothing() {
    // Arrange
    let app = spawn_app().await;
    seed(&app).await;

    // Act
    let body = app
        .post_json("/rb/search", json!({"query": "e", "tags": ["nonexistent"]}))
        .await
        .json();

    // Assert
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn search_results_include_excerpt_and_tags() {
    // Arrange
    let app = spawn_app().await;
    seed(&app).await;

    // Act
    let body = app.get("/rb/search?q=ownership").await.json();

    // Assert
    let result = &body["results"][0];
    assert!(result["id"].is_string());
    assert_eq!(result["title"], "Rust patterns");
    assert!(result["excerpt"].as_str().unwrap().contains("Ownership"));
    assert_eq!(result["tags"], json!(["rust", "programming"]));
}
