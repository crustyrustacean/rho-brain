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
    assert!(
        location.contains("saved=1"),
        "redirect should request a flash: {location}"
    );
    let doc_path = location.split('?').next().unwrap();

    // The document page renders markdown and tags, and the saved flash
    let page = app.get(location).await;
    assert!(page.status.is_success());
    let html = page.text();
    assert!(html.contains("Form Doc"));
    assert!(html.contains("<strong>markdown</strong>"));
    assert!(html.contains("Document saved."));
    // No flash without the query param
    assert!(!app.get(doc_path).await.text().contains("Document saved."));

    // The home page lists the document
    let home = app.get("/").await.text();
    assert!(home.contains("Form Doc"));

    // Edit via the form endpoint
    let response = app
        .post_form(
            &format!("{doc_path}/edit"),
            "title=Renamed&content=updated&tags=rust",
        )
        .await;
    assert_eq!(response.status, 303);
    assert!(app.get(doc_path).await.text().contains("Renamed"));

    // Delete via the form endpoint
    let response = app.post_form(&format!("{doc_path}/delete"), "").await;
    assert_eq!(response.status, 303);
    assert_eq!(app.get(doc_path).await.status, 404);
}

#[tokio::test]
async fn form_metadata_round_trips() {
    // Arrange
    let app = spawn_app().await;

    // Create with typed metadata lines
    let response = app
        .post_form(
            "/documents",
            "title=Meta+Doc&content=body&tags=rust&metadata=priority%3Dhigh%0Aversion%3D2%0Apinned%3Dtrue",
        )
        .await;
    assert_eq!(response.status, 303);
    let location = response.headers.get("location").unwrap().to_str().unwrap();
    let doc_path = location.split('?').next().unwrap().to_string();

    // The document page renders the metadata pairs
    let html = app.get(&doc_path).await.text();
    assert!(html.contains("metadata-list"));
    assert!(html.contains("priority"));
    assert!(html.contains("high"));
    assert!(html.contains("version"));
    assert!(html.contains("pinned"));

    // The metadata is typed the same way the JSON API types it
    let api = app.get(&format!("/rb{doc_path}")).await.json();
    assert_eq!(api["metadata"]["priority"], "high");
    assert_eq!(api["metadata"]["version"], 2);
    assert_eq!(api["metadata"]["pinned"], true);

    // The edit form is prefilled with key=value lines
    let edit = app.get(&format!("{doc_path}/edit")).await.text();
    assert!(edit.contains("priority=high"));
    assert!(edit.contains("version=2"));
    assert!(edit.contains("pinned=true"));

    // Updating replaces the metadata set
    let response = app
        .post_form(
            &format!("{doc_path}/edit"),
            "title=Meta+Doc&content=body&tags=rust&metadata=version%3D3",
        )
        .await;
    assert_eq!(response.status, 303);
    let html = app.get(&doc_path).await.text();
    assert!(html.contains("version"));
    assert!(!html.contains("priority"));

    // The JSON API agrees
    let api = app.get(&format!("/rb{doc_path}")).await.json();
    assert_eq!(api["metadata"]["version"], 3);
    assert!(api["metadata"].get("priority").is_none());
}

#[tokio::test]
async fn form_validation_rejects_empty_title_with_preserved_input() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app
        .post_form("/documents", "title=&content=precious+draft&tags=rust")
        .await;

    // Assert: the form re-renders with an error and the user's input
    assert_eq!(response.status, 422);
    let html = response.text();
    assert!(html.contains("Title is required."));
    assert!(html.contains("precious draft"));
    assert!(html.contains("rust"));

    // Nothing was persisted
    assert!(
        app.get("/rb/stats").await.json()["total"].is_null()
            || app.get("/rb/stats").await.json()["total"] == 0
    );
}

#[tokio::test]
async fn form_validation_rejects_malformed_metadata() {
    // Arrange
    let app = spawn_app().await;

    // Act: a line without '='
    let response = app
        .post_form("/documents", "title=T&content=C&metadata=justakey")
        .await;

    // Assert
    assert_eq!(response.status, 422);
    assert!(response.text().contains("Invalid metadata"));

    // And on the edit path too
    let create = app.post_form("/documents", "title=T&content=C").await;
    let doc_path = create
        .headers
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .split('?')
        .next()
        .unwrap()
        .to_string();
    let response = app
        .post_form(
            &format!("{doc_path}/edit"),
            "title=T&content=C&metadata=noequals",
        )
        .await;
    assert_eq!(response.status, 422);
    assert!(response.text().contains("Invalid metadata"));
}

#[tokio::test]
async fn delete_button_asks_for_confirmation() {
    // Arrange
    let app = spawn_app().await;
    let create = app.post_form("/documents", "title=T&content=C").await;
    let doc_path = create
        .headers
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .split('?')
        .next()
        .unwrap()
        .to_string();

    // Act
    let html = app.get(&doc_path).await.text();

    // Assert: the delete form confirms before submitting
    assert!(html.contains("confirm("));
    assert!(html.contains("This cannot be undone"));
}

#[tokio::test]
async fn home_page_filters_by_tag() {
    // Arrange
    let app = spawn_app().await;
    app.post_json(
        "/rb/documents",
        serde_json::json!({"title": "Rust notes", "content": "borrow checker", "tags": ["rust", "web"]}),
    )
    .await;
    app.post_json(
        "/rb/documents",
        serde_json::json!({"title": "Pasta recipe", "content": "salt the water", "tags": ["cooking"]}),
    )
    .await;

    // Act: filter by an existing tag
    let html = app.get("/?tag=rust").await.text();

    // Assert
    assert!(html.contains("Rust notes"));
    assert!(!html.contains("Pasta recipe"));
    assert!(html.contains("#rust"));

    // An unknown tag matches nothing
    let html = app.get("/?tag=nonexistent").await.text();
    assert!(html.contains("No documents found"));
}

#[tokio::test]
async fn home_page_tag_links_point_at_the_filter() {
    // Arrange
    let app = spawn_app().await;
    app.post_json(
        "/rb/documents",
        serde_json::json!({"title": "Tagged", "content": "c", "tags": ["rust"]}),
    )
    .await;

    // Act
    let html = app.get("/").await.text();

    // Assert
    assert!(html.contains(r#"href="/?tag=rust""#));
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
    assert!(html.contains("Clear"));
}

#[tokio::test]
async fn new_document_form_offers_tag_autocomplete() {
    // Arrange
    let app = spawn_app().await;
    app.post_json(
        "/rb/documents",
        serde_json::json!({"title": "T", "content": "C", "tags": ["rust", "web"]}),
    )
    .await;

    // Act
    let html = app.get("/documents/new").await.text();

    // Assert
    assert!(html.contains(r#"datalist id="tag-suggestions""#));
    assert!(html.contains(r#"<option value="rust">"#));
    assert!(html.contains(r#"<option value="web">"#));
}

fn json_doc(title: &str, content: &str) -> serde_json::Value {
    serde_json::json!({"title": title, "content": content})
}
