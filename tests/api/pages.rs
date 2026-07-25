// tests/api/pages.rs
//
// Smoke tests for the server-rendered UI: the pages return HTML, the forms
// drive the same document lifecycle as the JSON API, and POST handlers
// answer with 303 redirects (post/redirect/get).

use crate::helpers::spawn_app;

#[tokio::test]
async fn home_page_renders_layout_and_empty_state() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get("/").await;

    // Assert
    assert!(response.status.is_success());
    let html = response.text();
    assert!(html.contains("<title>rho-brain</title>"));
    assert!(html.contains(r#"<nav>"#));
    assert!(html.contains("No documents found"));
    assert!(html.contains("Total Documents"));
}

#[tokio::test]
async fn document_lifecycle_through_html_forms() {
    // Arrange
    let app = spawn_app().await;

    // Create via the form endpoint
    let response = app
        .post_form(
            "/documents",
            "title=Form+Doc&content=Hello+**markdown**&tags=rust%2C+web",
        )
        .await;
    assert_eq!(response.status, 303);
    let location = response.headers.get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/documents/"));

    // The document page renders markdown and tags
    let page = app.get(location).await;
    assert!(page.status.is_success());
    let html = page.text();
    assert!(html.contains("Form Doc"));
    assert!(html.contains("<strong>markdown</strong>"));
    assert!(html.contains("rust"));

    // The home page lists the document
    let home = app.get("/").await.text();
    assert!(home.contains("Form Doc"));

    // Edit via the form endpoint
    let response = app
        .post_form(
            &format!("{location}/edit"),
            "title=Renamed&content=updated&tags=rust",
        )
        .await;
    assert_eq!(response.status, 303);
    assert!(app.get(location).await.text().contains("Renamed"));

    // Delete via the form endpoint
    let response = app.post_form(&format!("{location}/delete"), "").await;
    assert_eq!(response.status, 303);
    assert_eq!(app.get(location).await.status, 404);
}

#[tokio::test]
async fn home_page_search_filters_documents() {
    // Arrange
    let app = spawn_app().await;
    app.post_json("/rb/documents", json_doc("Rust patterns", "borrow checker"))
        .await;
    app.post_json("/rb/documents", json_doc("Cooking pasta", "salt the water"))
        .await;

    // Act
    let html = app.get("/?q=pasta").await.text();

    // Assert
    assert!(html.contains("Cooking pasta"));
    assert!(!html.contains("Rust patterns"));
    assert!(html.contains("Clear Search"));
}

fn json_doc(title: &str, content: &str) -> serde_json::Value {
    serde_json::json!({"title": title, "content": content})
}
