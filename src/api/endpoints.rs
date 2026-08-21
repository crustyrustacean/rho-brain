// src/api/endpoints.rs

// Returns a machine-readable description of all rho-brain API endpoints so
// clients can discover and interact with the knowledge base programmatically
// without out-of-band documentation. Mirrors pi-brain's `/pb/endpoints`.
//
// The list is hand-maintained; `tests/api/endpoints.rs` guards against drift
// by probing every advertised endpoint against the live router. Topcoat's
// `Router` keeps its route table private, so the list cannot be generated
// from the router at runtime.

use serde::Serialize;
use topcoat::{
    Result,
    router::{content::Json, route},
};

/// Description of a single API endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointDescription {
    /// HTTP method (GET, POST, PUT, DELETE).
    pub method: String,
    /// URL path (relative to the API base).
    pub path: String,
    /// Human-readable description of what the endpoint does.
    pub description: String,
    /// Description of each parameter (body fields for POST/PUT, query params for GET).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterDescription>,
    /// Example request body (for POST/PUT endpoints).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParameterDescription {
    /// Parameter name.
    pub name: String,
    /// JSON type or location hint (e.g. "string", "number", "array<string>", "query").
    #[serde(rename = "type")]
    pub param_type: String,
    /// Whether the parameter is required.
    pub required: bool,
    /// Human-readable description.
    pub description: String,
}

/// Top-level response for the endpoints discovery endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointsResponse {
    /// API name.
    pub name: String,
    /// API version string.
    pub version: String,
    /// Base URL all paths are relative to (e.g. "/rb").
    pub base: String,
    /// Usage conventions that hold across endpoints — written for LLM agents
    /// discovering the API through this document.
    pub notes: Vec<String>,
    /// List of available endpoints.
    pub endpoints: Vec<EndpointDescription>,
}

#[route(GET "/rb/endpoints")]
pub async fn get_endpoints() -> Result<Json<EndpointsResponse>> {
    Ok(Json(EndpointsResponse {
        name: "rho-brain".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        base: "/rb".into(),
        notes: vec![
            "Document ids are UUIDs; malformed ids are rejected with 400, unknown or deleted ids with 404.".into(),
            "Every mutating endpoint returns the full updated document, so operations can chain without a follow-up GET.".into(),
            "PUT /rb/documents/{id} is partial for title/content (absent = unchanged) but REPLACES the whole tag list and metadata map when supplied.".into(),
            "For micro-updates prefer the append / tags / metadata routes: they touch only what they name and never require read-modify-write.".into(),
            "POST /rb/documents/get-or-create is the entry point for per-topic working documents: same title returns the existing document (200), otherwise creates a stub (201).".into(),
            "content is optional everywhere (empty string when omitted) — start with a title stub and append into it.".into(),
            "Search and content are synced through SQLite FTS5: appended text is searchable immediately.".into(),
        ],
        endpoints: build_endpoint_list(),
    }))
}

fn build_endpoint_list() -> Vec<EndpointDescription> {
    vec![
        // ---- Health ----
        EndpointDescription {
            method: "GET".into(),
            path: "/rb/health_check".into(),
            description: "Health check — confirms the API service is running.".into(),
            parameters: vec![],
            example_body: None,
        },
        // ---- Documents ----
        EndpointDescription {
            method: "POST".into(),
            path: "/rb/documents".into(),
            description: "Create a new document.".into(),
            parameters: vec![
                ParameterDescription {
                    name: "title".into(),
                    param_type: "string".into(),
                    required: true,
                    description: "Document title.".into(),
                },
                ParameterDescription {
                    name: "content".into(),
                    param_type: "string".into(),
                    required: false,
                    description: "Document body text (optional — create a title-only stub and append later).".into(),
                },
                ParameterDescription {
                    name: "tags".into(),
                    param_type: "array<string>".into(),
                    required: false,
                    description: "Optional tags for organisation.".into(),
                },
                ParameterDescription {
                    name: "metadata".into(),
                    param_type: "object".into(),
                    required: false,
                    description: "Optional arbitrary JSON metadata (string, number, and boolean values are preserved).".into(),
                },
            ],
            example_body: Some(serde_json::json!({
                "title": "Example document",
                "content": "The body of the document.",
                "tags": ["example", "demo"],
                "metadata": { "source": "manual" }
            })),
        },
        EndpointDescription {
            method: "GET".into(),
            path: "/rb/documents".into(),
            description: "List all non-deleted documents with pagination.".into(),
            parameters: vec![
                ParameterDescription {
                    name: "limit".into(),
                    param_type: "query (number)".into(),
                    required: false,
                    description: "Max results to return (default 50).".into(),
                },
                ParameterDescription {
                    name: "offset".into(),
                    param_type: "query (number)".into(),
                    required: false,
                    description: "Number of results to skip (default 0).".into(),
                },
            ],
            example_body: None,
        },
        EndpointDescription {
            method: "GET".into(),
            path: "/rb/documents/{id}".into(),
            description: "Get a single document by UUID.".into(),
            parameters: vec![ParameterDescription {
                name: "id".into(),
                param_type: "path (UUID)".into(),
                required: true,
                description: "The document UUID.".into(),
            }],
            example_body: None,
        },
        EndpointDescription {
            method: "PUT".into(),
            path: "/rb/documents/{id}".into(),
            description: "Update an existing document. Only supplied fields are changed."
                .into(),
            parameters: vec![
                ParameterDescription {
                    name: "id".into(),
                    param_type: "path (UUID)".into(),
                    required: true,
                    description: "The document UUID.".into(),
                },
                ParameterDescription {
                    name: "title".into(),
                    param_type: "string".into(),
                    required: false,
                    description: "New title.".into(),
                },
                ParameterDescription {
                    name: "content".into(),
                    param_type: "string".into(),
                    required: false,
                    description: "New content.".into(),
                },
                ParameterDescription {
                    name: "tags".into(),
                    param_type: "array<string>".into(),
                    required: false,
                    description: "Replace tags.".into(),
                },
                ParameterDescription {
                    name: "metadata".into(),
                    param_type: "object".into(),
                    required: false,
                    description: "Replace metadata.".into(),
                },
            ],
            example_body: Some(serde_json::json!({
                "title": "Updated title"
            })),
        },
        EndpointDescription {
            method: "DELETE".into(),
            path: "/rb/documents/{id}".into(),
            description: "Soft-delete a document.".into(),
            parameters: vec![ParameterDescription {
                name: "id".into(),
                param_type: "path (UUID)".into(),
                required: true,
                description: "The document UUID.".into(),
            }],
            example_body: None,
        },
        EndpointDescription {
            method: "POST".into(),
            path: "/rb/documents/{id}/append".into(),
            description: "Micro-update: append text to a document's content without read-modify-write. Joins with a blank line by default; sets content directly on an empty document. Returns the full updated document."
                .into(),
            parameters: vec![
                ParameterDescription {
                    name: "id".into(),
                    param_type: "path (UUID)".into(),
                    required: true,
                    description: "The document UUID.".into(),
                },
                ParameterDescription {
                    name: "content".into(),
                    param_type: "string".into(),
                    required: true,
                    description: "Text to append.".into(),
                },
                ParameterDescription {
                    name: "separator".into(),
                    param_type: "string".into(),
                    required: false,
                    description: "Joiner between existing and appended text (default: blank line).".into(),
                },
            ],
            example_body: Some(serde_json::json!({
                "content": "Follow-up: measurements are in."
            })),
        },
        EndpointDescription {
            method: "POST".into(),
            path: "/rb/documents/{id}/tags".into(),
            description: "Micro-update: add tags (set union). Duplicates are no-ops; existing tags are untouched. Removing tags is a full-list PUT. Returns the full updated document."
                .into(),
            parameters: vec![
                ParameterDescription {
                    name: "id".into(),
                    param_type: "path (UUID)".into(),
                    required: true,
                    description: "The document UUID.".into(),
                },
                ParameterDescription {
                    name: "tags".into(),
                    param_type: "array<string>".into(),
                    required: true,
                    description: "Tag names to add.".into(),
                },
            ],
            example_body: Some(serde_json::json!({
                "tags": ["follow-up", "measurements"]
            })),
        },
        EndpointDescription {
            method: "POST".into(),
            path: "/rb/documents/{id}/metadata".into(),
            description: "Micro-update: merge metadata keys. Listed keys are set (typed like create: booleans and integers preserved), null deletes a key, unlisted keys are left alone. Returns the full updated document."
                .into(),
            parameters: vec![
                ParameterDescription {
                    name: "id".into(),
                    param_type: "path (UUID)".into(),
                    required: true,
                    description: "The document UUID.".into(),
                },
                ParameterDescription {
                    name: "(body)".into(),
                    param_type: "object".into(),
                    required: true,
                    description: "Keys to set or delete (value null deletes).".into(),
                },
            ],
            example_body: Some(serde_json::json!({
                "priority": "high",
                "pinned": true,
                "stale-key": null
            })),
        },
        EndpointDescription {
            method: "POST".into(),
            path: "/rb/documents/get-or-create".into(),
            description: "Find the active document with this exact title or create it. 200 returns the existing document unchanged; 201 creates a stub (empty content unless supplied) with optional tags. The idempotent entry point for per-topic working documents."
                .into(),
            parameters: vec![
                ParameterDescription {
                    name: "title".into(),
                    param_type: "string".into(),
                    required: true,
                    description: "Exact title to match or create under.".into(),
                },
                ParameterDescription {
                    name: "content".into(),
                    param_type: "string".into(),
                    required: false,
                    description: "Initial content (create path only; ignored when the document exists).".into(),
                },
                ParameterDescription {
                    name: "tags".into(),
                    param_type: "array<string>".into(),
                    required: false,
                    description: "Initial tags (create path only; ignored when the document exists).".into(),
                },
            ],
            example_body: Some(serde_json::json!({
                "title": "Daily Journal",
                "tags": ["journal"]
            })),
        },
        // ---- Search ----
        EndpointDescription {
            method: "POST".into(),
            path: "/rb/search".into(),
            description: "Full-text search (SQLite FTS5) across documents, returning ranked results with excerpts."
                .into(),
            parameters: vec![
                ParameterDescription {
                    name: "query".into(),
                    param_type: "string".into(),
                    required: true,
                    description: "Search query.".into(),
                },
                ParameterDescription {
                    name: "tags".into(),
                    param_type: "array<string>".into(),
                    required: false,
                    description: "Filter to documents with any of these tags.".into(),
                },
                ParameterDescription {
                    name: "limit".into(),
                    param_type: "number".into(),
                    required: false,
                    description: "Max results (default 20).".into(),
                },
                ParameterDescription {
                    name: "offset".into(),
                    param_type: "number".into(),
                    required: false,
                    description: "Results to skip (default 0).".into(),
                },
            ],
            example_body: Some(serde_json::json!({
                "query": "rust async patterns",
                "tags": ["rust"],
                "limit": 10
            })),
        },
        EndpointDescription {
            method: "GET".into(),
            path: "/rb/search".into(),
            description: "Search via GET query parameters (simpler but without tag filtering)."
                .into(),
            parameters: vec![
                ParameterDescription {
                    name: "q".into(),
                    param_type: "query (string)".into(),
                    required: true,
                    description: "Search query.".into(),
                },
                ParameterDescription {
                    name: "limit".into(),
                    param_type: "query (number)".into(),
                    required: false,
                    description: "Max results.".into(),
                },
                ParameterDescription {
                    name: "offset".into(),
                    param_type: "query (number)".into(),
                    required: false,
                    description: "Results to skip.".into(),
                },
            ],
            example_body: None,
        },
        // ---- Stats ----
        EndpointDescription {
            method: "GET".into(),
            path: "/rb/stats".into(),
            description: "Get knowledge base statistics (document counts, unique tags).".into(),
            parameters: vec![],
            example_body: None,
        },
    ]
}
