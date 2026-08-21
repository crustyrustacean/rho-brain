// tests/api/editor_ui.rs
//
// The editor form is progressively enhanced: it renders as a plain HTML form
// (the existing lifecycle tests cover that path) and grows a Datastar-driven
// split view — mode toggle, live preview, and a collapsed extras section —
// when JavaScript is available.

use crate::helpers::spawn_app;

#[tokio::test]
async fn editor_script_is_served_as_javascript() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get("/js/editor.js").await;

    // Assert
    assert_eq!(response.status, 200);
    let content_type = response
        .headers
        .get("content-type")
        .expect("content-type header present")
        .to_str()
        .unwrap();
    assert!(content_type.contains("javascript"), "{content_type}");
    let body = response.text();
    assert!(
        body.len() > 500,
        "expected the real editor script, got {} bytes",
        body.len()
    );
    assert!(body.contains("rho-brain"), "body: {body}");
}

#[tokio::test]
async fn form_pages_wire_up_draft_autosave() {
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
    let doc_id = doc_path.trim_start_matches("/documents/");

    // Act
    let new_page = app.get("/documents/new").await.text();
    let edit_page = app.get(&format!("{doc_path}/edit")).await.text();

    // Assert: both pages load the editor script and key drafts per document
    // (a "new" draft must never clobber an existing document's draft)
    assert!(
        new_page.contains(r#"<script src="/js/editor.js" defer="">"#),
        "{new_page}"
    );
    assert!(
        edit_page.contains(r#"<script src="/js/editor.js" defer="">"#),
        "{edit_page}"
    );
    assert!(new_page.contains(r#"data-draft="new""#), "{new_page}");
    assert!(
        edit_page.contains(&format!("data-draft=\"{doc_id}\"")),
        "{edit_page}"
    );
}

#[tokio::test]
async fn new_document_page_has_editor_structure() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let html = app.get("/documents/new").await.text();

    // Assert: mode signal, split default, toggle actions
    assert!(
        html.contains(r#"data-signals-mode="'split'""#),
        "mode signal missing:\n{html}"
    );
    for mode in ["write", "split", "preview"] {
        assert!(
            html.contains(&format!("@set(mode='{mode}')")),
            "{mode} toggle missing:\n{html}"
        );
    }

    // Panes: write pane hidden in preview mode, preview pane hidden in write mode
    assert!(html.contains(r#"data-show="mode !== 'preview'""#), "{html}");
    assert!(html.contains(r#"data-show="mode !== 'write'""#), "{html}");

    // The textarea binds the content signal and refreshes the preview
    assert!(html.contains("data-bind-content"), "{html}");
    assert!(
        html.contains("@post('/documents/preview'"),
        "preview action missing:\n{html}"
    );

    // A server-rendered preview element exists from the start
    assert!(html.contains(r#"<div id="preview""#), "{html}");

    // Tags and metadata are collapsed behind a details element
    assert!(
        html.contains(r#"<details class="editor-extras">"#),
        "{html}"
    );
    assert!(html.contains("<summary>"), "{html}");

    // The plain-form contract survives: fields still submit with these names
    assert!(html.contains(r#"name="title""#), "{html}");
    assert!(html.contains(r#"name="content""#), "{html}");
    assert!(html.contains(r#"name="tags""#), "{html}");
    assert!(html.contains(r#"name="metadata""#), "{html}");
}

#[tokio::test]
async fn edit_page_shows_server_rendered_initial_preview() {
    // Arrange
    let app = spawn_app().await;
    let create = app
        .post_form(
            "/documents",
            "title=Draft&content=Hello+**world**+from+markdown",
        )
        .await;
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
    let html = app.get(&format!("{doc_path}/edit")).await.text();

    // Assert: the preview pane starts populated with rendered markdown, so
    // the split view is correct before any Datastar round-trip
    assert!(html.contains(r#"<div id="preview""#), "{html}");
    assert!(
        html.contains("<strong>world</strong>"),
        "initial preview should render existing content:\n{html}"
    );

    // And the textarea carries the raw markdown for editing
    assert!(html.contains("Hello **world**"), "{html}");
}
