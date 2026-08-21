// tests/api/endpoints.rs

use crate::helpers::spawn_app;

/// GET /rb/endpoints returns a valid discovery document.
#[tokio::test]
async fn endpoints_returns_discovery_document() {
    let app = spawn_app().await;

    let response = app.get("/rb/endpoints").await;

    assert!(response.status.is_success());

    let body = response.json();
    assert_eq!(body["name"], "rho-brain");
    assert_eq!(body["base"], "/rb");
    assert!(body["version"].is_string());

    // Conventions for LLM agents discovering the API through this document
    let notes = body["notes"].as_array().expect("notes should be an array");
    assert!(!notes.is_empty(), "discovery doc should carry usage notes");
    assert!(
        notes
            .iter()
            .any(|n| n.as_str().unwrap_or("").contains("micro-updates")),
        "notes: {notes:?}"
    );

    let endpoints = body["endpoints"]
        .as_array()
        .expect("endpoints should be an array");
    assert!(!endpoints.is_empty());
    for ep in endpoints {
        assert!(ep["method"].is_string(), "each endpoint has a method");
        assert!(ep["path"].is_string(), "each endpoint has a path");
        assert!(
            ep["description"].is_string(),
            "each endpoint has a description"
        );
    }
}

/// The advertised list covers the full known surface. If a route is added or
/// removed, this fails until `build_endpoint_list` is updated to match.
#[tokio::test]
async fn advertised_list_matches_known_surface() {
    let app = spawn_app().await;
    let body = app.get("/rb/endpoints").await.json();

    let advertised: Vec<(String, String)> = body["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|ep| {
            (
                ep["method"].as_str().unwrap().to_string(),
                ep["path"].as_str().unwrap().to_string(),
            )
        })
        .collect();

    let known = [
        ("GET", "/rb/health_check"),
        ("POST", "/rb/documents"),
        ("GET", "/rb/documents"),
        ("GET", "/rb/documents/{id}"),
        ("PUT", "/rb/documents/{id}"),
        ("DELETE", "/rb/documents/{id}"),
        ("POST", "/rb/documents/{id}/append"),
        ("POST", "/rb/documents/{id}/tags"),
        ("POST", "/rb/documents/{id}/metadata"),
        ("POST", "/rb/documents/get-or-create"),
        ("POST", "/rb/search"),
        ("GET", "/rb/search"),
        ("GET", "/rb/stats"),
    ];

    for (method, path) in known {
        assert!(
            advertised.iter().any(|(m, p)| m == method && p == path),
            "endpoint {method} {path} missing from advertised list"
        );
    }
    assert_eq!(
        advertised.len(),
        known.len(),
        "advertised list has entries beyond the known surface — update this test"
    );
}

/// Every advertised endpoint exists on the live router.
///
/// Concrete paths are probed directly. Parameterised `{id}` routes are probed
/// with a non-UUID: the path pattern still matches, so the handler runs and
/// rejects the id with a 400 — whereas a missing route would 404. Any
/// response other than 404 proves the route exists.
#[tokio::test]
async fn every_advertised_endpoint_exists_on_the_router() {
    let app = spawn_app().await;
    let body = app.get("/rb/endpoints").await.json();

    for ep in body["endpoints"].as_array().unwrap() {
        let method = ep["method"].as_str().unwrap();
        let path = ep["path"].as_str().unwrap();
        let concrete_path = path.replace("{id}", "not-a-uuid");

        let response = match method {
            "GET" => app.get(&concrete_path).await,
            "POST" => app.post_json(&concrete_path, serde_json::json!({})).await,
            "PUT" => app.put_json(&concrete_path, serde_json::json!({})).await,
            "DELETE" => app.delete(&concrete_path).await,
            other => panic!("unhandled method in advertised endpoint: {other}"),
        };

        assert_ne!(
            response.status.as_u16(),
            404,
            "advertised endpoint {method} {path} does not exist on the router"
        );
    }
}
