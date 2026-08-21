// tests/api/assets.rs
//
// The app self-hosts its client-side runtime instead of loading it from a
// CDN: the vendored Datastar bundle is embedded in the binary and served by a
// plain route. This keeps the app offline-friendly, keeps `cargo test`
// hermetic (no topcoat asset bundle step), and exercises the exact URL the
// layout loads.

use crate::helpers::spawn_app;

#[tokio::test]
async fn datastar_script_is_served_as_javascript() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get("/js/datastar.js").await;

    // Assert: the route exists and identifies as JavaScript. Browsers refuse
    // to execute `<script type="module">` responses without a JS media type.
    assert_eq!(response.status, 200);
    let content_type = response
        .headers
        .get("content-type")
        .expect("content-type header present")
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("javascript"),
        "unexpected content type: {content_type}"
    );

    // The body is the real vendored bundle, not a stub.
    let body = response.text();
    assert!(
        body.len() > 10_000,
        "expected the full bundle, got {} bytes",
        body.len()
    );
    assert!(body.contains("Datastar v"), "bundle should carry its banner");
}
