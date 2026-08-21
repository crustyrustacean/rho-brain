// tests/api/preview.rs
//
// POST /documents/preview: the editor's live preview endpoint. Datastar
// actions send the page's signals as a JSON body; the handler renders the
// `content` signal server-side with the same renderer as the document view
// page, so the preview always matches the final render.

use crate::helpers::spawn_app;

#[tokio::test]
async fn preview_renders_markdown_from_signals() {
    // Arrange
    let app = spawn_app().await;

    // Act: signals as Datastar would send them (extra fields like `mode` are
    // sent along and ignored)
    let response = app
        .post_json(
            "/documents/preview",
            serde_json::json!({"content": "# Hi **there**\n\nsome `code`", "mode": "split"}),
        )
        .await;

    // Assert: a patch carrying a preview element with rendered markdown
    assert_eq!(response.status, 200);
    let body = response.text();
    assert!(body.contains(r#"id="preview""#), "body: {body}");
    assert!(
        body.contains("<h1>Hi <strong>there</strong></h1>"),
        "body: {body}"
    );
    assert!(body.contains("<code>code</code>"), "body: {body}");
}

#[tokio::test]
async fn preview_escapes_raw_html_like_the_view_page() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app
        .post_json(
            "/documents/preview",
            serde_json::json!({"content": "<script>alert(1)</script>"}),
        )
        .await;

    // Assert: same XSS posture as the document view — raw HTML is escaped,
    // never executed. (The patch body itself is HTML, so check for the raw
    // tag specifically.)
    assert_eq!(response.status, 200);
    let body = response.text();
    assert!(!body.contains("<script>alert"), "body: {body}");
}

#[tokio::test]
async fn preview_handles_missing_and_empty_content() {
    // Arrange
    let app = spawn_app().await;

    // Act / Assert: no content signal at all
    let response = app
        .post_json("/documents/preview", serde_json::json!({}))
        .await;
    assert_eq!(response.status, 200);
    assert!(response.text().contains(r#"id="preview""#));

    // Act / Assert: whitespace-only content
    let response = app
        .post_json(
            "/documents/preview",
            serde_json::json!({"content": "   \n"}),
        )
        .await;
    assert_eq!(response.status, 200);
    assert!(response.text().contains(r#"id="preview""#));
}
