// src/api/documents.rs

use serde::{Deserialize, Serialize};
use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{Json, bad_request, not_found, path_param, query_params, route},
};

use crate::models::{Document, DocumentTag, Metadata, Tag};

// ── Helpers ─────────────────────────────────────────────────────────

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

/// Get-or-create a tag by name.
///
/// A bare `upsert_by_name(...)` is rejected by toasty ("upsert requires at
/// least one update assignment"), so use `or_ignore`: it returns `Some` on
/// insert and `None` on conflict, in which case we fetch the existing row.
pub(crate) async fn get_or_create_tag(db: &mut Db, tag_name: &str) -> Result<Tag> {
    let inserted = Tag::upsert_by_name(tag_name)
        .or_ignore()
        .exec(&mut *db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    match inserted {
        Some(tag) => Ok(tag),
        None => Tag::get_by_name(db, tag_name)
            .await
            .map_err(|e| topcoat::router::internal_server_error(e).into()),
    }
}

// ── Path params ─────────────────────────────────────────────────────

#[path_param(error = bad_request)]
struct DocumentId(String);

// ── Query params ────────────────────────────────────────────────────

#[derive(Debug)]
#[query_params(error = bad_request)]
pub struct ListQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ── Request / Response types ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateDocumentRequest {
    pub title: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDocumentRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct DocumentResponse {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct DocumentListResponse {
    pub documents: Vec<DocumentResponse>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

// ── Routes ──────────────────────────────────────────────────────────

#[route(POST "/rb/documents")]
pub async fn create_document(
    cx: &Cx,
    Json(input): Json<CreateDocumentRequest>,
) -> Result<Json<DocumentResponse>> {
    let mut db = db(cx);

    let doc = Document::create()
        .title(&input.title)
        .content(&input.content)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    // Handle tags
    if let Some(tags) = input.tags {
        for tag_name in tags {
            let tag = get_or_create_tag(&mut db, &tag_name).await?;

            DocumentTag::create()
                .document_id(doc.id)
                .tag_id(tag.id)
                .exec(&mut db)
                .await
                .map_err(topcoat::router::internal_server_error)?;
        }
    }

    // Handle metadata
    if let Some(meta) = input.metadata {
        if let serde_json::Value::Object(map) = meta {
            for (key, value) in map {
                let builder = Metadata::create().document_id(doc.id).key(&key);

                let builder = match value {
                    serde_json::Value::String(s) => builder.value_text(s),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            builder.value_int(i)
                        } else {
                            builder.value_text(n.to_string())
                        }
                    }
                    serde_json::Value::Bool(b) => builder.value_bool(b),
                    other => builder.value_text(other.to_string()),
                };

                builder
                    .exec(&mut db)
                    .await
                    .map_err(topcoat::router::internal_server_error)?;
            }
        }
    }

    build_document_response(&mut db, doc).await
}

#[route(GET "/rb/documents")]
pub async fn list_documents(cx: &Cx) -> Result<Json<DocumentListResponse>> {
    let mut db = db(cx);

    let query = query_params::<ListQuery>(cx).unwrap_or(&ListQuery {
        limit: None,
        offset: None,
    });

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    // Only return non-deleted documents
    let docs = Document::all()
        .filter(Document::fields().deleted_at().is_none())
        .limit(limit)
        .offset(offset)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    let mut responses = Vec::new();
    for doc in docs {
        let Json(resp) = build_document_response(&mut db, doc).await?;
        responses.push(resp);
    }

    let total = responses.len();

    Ok(Json(DocumentListResponse {
        documents: responses,
        total,
        limit,
        offset,
    }))
}

#[route(GET "/rb/documents/{document_id}")]
pub async fn get_document(cx: &Cx) -> Result<Json<DocumentResponse>> {
    let mut db = db(cx);
    let id_str = path_param::<DocumentId>(cx)?;

    let id = uuid::Uuid::parse_str(&id_str).map_err(|_| bad_request("invalid document id"))?;

    let doc = Document::get_by_id(&mut db, &id)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    // Don't return soft-deleted documents
    if doc.deleted_at.is_some() {
        return Err(not_found().into());
    }

    build_document_response(&mut db, doc).await
}

#[route(PUT "/rb/documents/{document_id}")]
pub async fn update_document(
    cx: &Cx,
    Json(input): Json<UpdateDocumentRequest>,
) -> Result<Json<DocumentResponse>> {
    let mut db = db(cx);
    let id_str = path_param::<DocumentId>(cx)?;

    let id = uuid::Uuid::parse_str(&id_str).map_err(|_| bad_request("invalid document id"))?;

    let mut doc = Document::get_by_id(&mut db, &id)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    if doc.deleted_at.is_some() {
        return Err(not_found().into());
    }

    // Update fields. The instance-level `update()` borrows `doc` mutably and
    // applies the changes in place on `exec`, returning `()`.
    let mut update = doc.update();
    if let Some(title) = input.title {
        update = update.title(title);
    }
    if let Some(content) = input.content {
        update = update.content(content);
    }

    update
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    // Replace tags if provided
    if let Some(tags) = input.tags {
        // Remove existing tags
        DocumentTag::filter_by_document_id(doc.id)
            .delete()
            .exec(&mut db)
            .await
            .map_err(topcoat::router::internal_server_error)?;

        // Add new tags
        for tag_name in tags {
            let tag = get_or_create_tag(&mut db, &tag_name).await?;

            DocumentTag::create()
                .document_id(doc.id)
                .tag_id(tag.id)
                .exec(&mut db)
                .await
                .map_err(topcoat::router::internal_server_error)?;
        }
    }

    // Replace metadata if provided
    if let Some(meta) = input.metadata {
        Metadata::filter_by_document_id(doc.id)
            .delete()
            .exec(&mut db)
            .await
            .map_err(topcoat::router::internal_server_error)?;

        if let serde_json::Value::Object(map) = meta {
            for (key, value) in map {
                let builder = Metadata::create().document_id(doc.id).key(&key);

                let builder = match value {
                    serde_json::Value::String(s) => builder.value_text(s),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            builder.value_int(i)
                        } else {
                            builder.value_text(n.to_string())
                        }
                    }
                    serde_json::Value::Bool(b) => builder.value_bool(b),
                    other => builder.value_text(other.to_string()),
                };

                builder
                    .exec(&mut db)
                    .await
                    .map_err(topcoat::router::internal_server_error)?;
            }
        }
    }

    build_document_response(&mut db, doc).await
}

#[route(DELETE "/rb/documents/{document_id}")]
pub async fn delete_document(cx: &Cx) -> Result<Json<serde_json::Value>> {
    let mut db = db(cx);
    let id_str = path_param::<DocumentId>(cx)?;

    let id = uuid::Uuid::parse_str(&id_str).map_err(|_| bad_request("invalid document id"))?;

    let mut doc = Document::get_by_id(&mut db, &id)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    if doc.deleted_at.is_some() {
        return Err(not_found().into());
    }

    // Soft delete
    doc.update()
        .deleted_at(Some(jiff::Timestamp::now()))
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ── Response builder ────────────────────────────────────────────────

async fn build_document_response(db: &mut Db, doc: Document) -> Result<Json<DocumentResponse>> {
    // Load tags
    let doc_tags = DocumentTag::filter_by_document_id(doc.id)
        .exec(db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    let mut tag_names = Vec::new();
    for dt in doc_tags {
        let tag = Tag::get_by_id(&mut *db, &dt.tag_id)
            .await
            .map_err(topcoat::router::internal_server_error)?;
        tag_names.push(tag.name);
    }

    // Load metadata
    let metadata_rows = Metadata::filter_by_document_id(doc.id)
        .exec(db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    let mut metadata_map = serde_json::Map::new();
    for row in metadata_rows {
        let value = if let Some(s) = row.value_text {
            serde_json::Value::String(s)
        } else if let Some(i) = row.value_int {
            serde_json::Value::Number(i.into())
        } else if let Some(b) = row.value_bool {
            serde_json::Value::Bool(b)
        } else {
            serde_json::Value::Null
        };
        metadata_map.insert(row.key, value);
    }

    Ok(Json(DocumentResponse {
        id: doc.id.to_string(),
        title: doc.title,
        content: doc.content,
        tags: tag_names,
        metadata: serde_json::Value::Object(metadata_map),
        created_at: doc.created_at.to_string(),
        updated_at: doc.updated_at.to_string(),
    }))
}
