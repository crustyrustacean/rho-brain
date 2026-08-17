// tests/api/backfill.rs

use crate::helpers::spawn_app_at;

/// Regression test: backfill must index documents that already exist in the
/// `documents` table when the FTS index is empty (first run with FTS, or a
/// database migrated from before FTS existed).
///
/// The original bug: toasty stores UUID primary keys as 16-byte SQLite
/// blobs, but backfill decoded them with `as_str()`, which returns `None`
/// for blobs — so every row was silently skipped and the index stayed empty.
#[tokio::test]
async fn backfill_reindexes_existing_documents() {
    // A file-based database so the documents survive across two `connect()`
    // calls; a nanos timestamp keeps parallel test runs from colliding.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rho-brain-backfill-{nanos}.db"));
    let url = format!("sqlite:{}", path.display());

    // Instance 1: create documents through the API (toasty writes the ids
    // as blobs — the exact storage the buggy decode could not read).
    let app = spawn_app_at(&url).await;
    let marker = format!("backfillmarker{nanos}");
    let response = app
        .post_json(
            "/rb/documents",
            serde_json::json!({
                "title": format!("Pre-FTS document {marker}"),
                "content": format!("This document predates the index; it contains the word {marker}."),
            }),
        )
        .await;
    assert!(response.status.is_success());

    // Simulate a pre-FTS database by emptying the index the API writes to.
    toasty::sql::statement("DELETE FROM documents_fts")
        .exec(&mut app.db.clone())
        .await
        .expect("Failed to clear FTS index");

    // Instance 2: a fresh `connect()` runs `backfill_if_empty`, which must
    // now re-index the existing (blob-id'd) documents.
    let app2 = spawn_app_at(&url).await;
    let response = app2
        .post_json(
            "/rb/search",
            serde_json::json!({ "query": marker }),
        )
        .await;

    assert!(response.status.is_success());
    let body = response.json();
    let results = body["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        1,
        "backfill should have re-indexed the pre-existing document"
    );
    assert!(results[0]["title"]
        .as_str()
        .unwrap()
        .contains(&marker));

    std::fs::remove_file(&path).ok();
}
