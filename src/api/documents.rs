// src/api/documents.rs

use serde::{Deserialize, Serialize};
use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        StatusCode,
        content::Json,
        error::{bad_request, not_found},
        path_param, query_params,
        response::{IntoResponse, Response},
        route,
    },
};

use crate::models::{Document, DocumentTag, Metadata, Tag};

// ── Helpers ─────────────────────────────────────────────────────────

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

/// Map a toasty error to an HTTP error: a missing record is a 404, anything
/// else is a 500.
fn db_error_or_not_found(e: toasty::Error) -> topcoat::Error {
    if e.is_record_not_found() {
        not_found().into()
    } else {
        topcoat::router::error::internal_server_error(e).into()
    }
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
        .map_err(topcoat::router::error::internal_server_error)?;

    match inserted {
        Some(tag) => Ok(tag),
        None => Tag::get_by_name(db, tag_name)
            .await
            .map_err(|e| topcoat::router::error::internal_server_error(e).into()),
    }
}

// ── Path params ─────────────────────────────────────────────────────

path_param!(document_id);

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
    /// Optional so agents can create a title-only stub and append later.
    pub content: Option<String>,
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
        .content(input.content.as_deref().unwrap_or(""))
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    // Handle tags
    if let Some(tags) = input.tags {
        for tag_name in tags {
            let tag = get_or_create_tag(&mut db, &tag_name).await?;

            DocumentTag::create()
                .document_id(doc.id)
                .tag_id(tag.id)
                .exec(&mut db)
                .await
                .map_err(topcoat::router::error::internal_server_error)?;
        }
    }

    // Handle metadata
    if let Some(serde_json::Value::Object(map)) = input.metadata {
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
                .map_err(topcoat::router::error::internal_server_error)?;
        }
    }

    // Index in FTS5
    crate::fts::index_document(
        &mut db,
        &doc.id,
        &input.title,
        input.content.as_deref().unwrap_or(""),
    )
    .await
    .map_err(topcoat::router::error::internal_server_error)?;

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
        .map_err(topcoat::router::error::internal_server_error)?;

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
    let id_str = path_param::<DocumentId>(cx);

    let id = uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id"))?;

    let doc = Document::get_by_id(&mut db, &id)
        .await
        .map_err(db_error_or_not_found)?;

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
    let id_str = path_param::<DocumentId>(cx);

    let id = uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id"))?;

    let mut doc = Document::get_by_id(&mut db, &id)
        .await
        .map_err(db_error_or_not_found)?;

    if doc.deleted_at.is_some() {
        return Err(not_found().into());
    }

    // Track the new title/content for the FTS update. We need them before
    // the instance-level update borrows `doc` mutably.
    let new_title = input.title.clone().unwrap_or_else(|| doc.title.clone());
    let new_content = input.content.clone().unwrap_or_else(|| doc.content.clone());

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
        .map_err(topcoat::router::error::internal_server_error)?;

    // Replace tags if provided
    if let Some(tags) = input.tags {
        // Remove existing tags
        DocumentTag::filter_by_document_id(doc.id)
            .delete()
            .exec(&mut db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;

        // Add new tags
        for tag_name in tags {
            let tag = get_or_create_tag(&mut db, &tag_name).await?;

            DocumentTag::create()
                .document_id(doc.id)
                .tag_id(tag.id)
                .exec(&mut db)
                .await
                .map_err(topcoat::router::error::internal_server_error)?;
        }
    }

    // Replace metadata if provided
    if let Some(meta) = input.metadata {
        Metadata::filter_by_document_id(doc.id)
            .delete()
            .exec(&mut db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;

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
                    .map_err(topcoat::router::error::internal_server_error)?;
            }
        }
    }

    // Sync FTS5 index with the new title/content
    crate::fts::update_document_index(&mut db, &doc.id, &new_title, &new_content)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    build_document_response(&mut db, doc).await
}

// ── Micro-update routes ────────────────────────────────────────────
//
// Small, single-purpose mutations for LLM agents: each touches only what it
// names, keeps the FTS index in sync, and returns the full updated document
// so operations chain without a follow-up GET.

/// Load a document by id, mapping missing and soft-deleted to 404.
async fn active_document(db: &mut Db, id: &uuid::Uuid) -> Result<Document> {
    let doc = Document::get_by_id(&mut *db, id)
        .await
        .map_err(db_error_or_not_found)?;

    if doc.deleted_at.is_some() {
        return Err(not_found().into());
    }
    Ok(doc)
}

/// Write one typed metadata row, applying the same type inference as
/// create/update: string → text, integer → int, bool → bool, else text.
async fn insert_metadata_row(
    db: &mut Db,
    doc_id: uuid::Uuid,
    key: &str,
    value: serde_json::Value,
) -> Result<()> {
    let builder = Metadata::create().document_id(doc_id).key(key);
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
        .exec(db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct AppendRequest {
    pub content: String,
    /// Joiner between the existing and appended text (default: blank line).
    pub separator: Option<String>,
}

/// Append text to a document's content without read-modify-write.
///
/// Appending to an empty (stub) document sets the content directly with no
/// separator.
#[route(POST "/rb/documents/{document_id}/append")]
pub async fn append_document(
    cx: &Cx,
    Json(input): Json<AppendRequest>,
) -> Result<Json<DocumentResponse>> {
    let mut db = db(cx);
    let id_str = path_param::<DocumentId>(cx);
    let id = uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id"))?;

    let mut doc = active_document(&mut db, &id).await?;

    let separator = input.separator.unwrap_or_else(|| "\n\n".to_string());
    let new_content = if doc.content.is_empty() {
        input.content
    } else {
        format!("{}{}{}", doc.content, separator, input.content)
    };

    doc.update()
        .content(&new_content)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    crate::fts::update_document_index(&mut db, &doc.id, &doc.title, &new_content)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    build_document_response(&mut db, doc).await
}

#[derive(Debug, Deserialize)]
pub struct AddTagsRequest {
    pub tags: Vec<String>,
}

/// Add tags to a document without resending the whole tag set: the union of
/// the request's tags and the document's existing tags. Duplicates (in the
/// request or already attached) are no-ops. Removing tags stays a full-list
/// `PUT`.
#[route(POST "/rb/documents/{document_id}/tags")]
pub async fn add_tags(
    cx: &Cx,
    Json(input): Json<AddTagsRequest>,
) -> Result<Json<DocumentResponse>> {
    let mut db = db(cx);
    let id_str = path_param::<DocumentId>(cx);
    let id = uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id"))?;

    let doc = active_document(&mut db, &id).await?;

    let mut seen = std::collections::HashSet::new();
    for raw in &input.tags {
        let name = raw.trim();
        if name.is_empty() || !seen.insert(name.to_string()) {
            continue;
        }
        let tag = get_or_create_tag(&mut db, name).await?;
        let already = DocumentTag::filter_by_document_id(doc.id)
            .filter(DocumentTag::fields().tag_id().eq(tag.id))
            .limit(1)
            .exec(&mut db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;
        if already.is_empty() {
            DocumentTag::create()
                .document_id(doc.id)
                .tag_id(tag.id)
                .exec(&mut db)
                .await
                .map_err(topcoat::router::error::internal_server_error)?;
        }
    }

    build_document_response(&mut db, doc).await
}

/// Merge metadata keys: listed keys are set (typed like create), `null`
/// deletes the key, and unlisted keys are untouched. Unlike the full `PUT`,
/// which replaces the entire metadata set.
#[route(POST "/rb/documents/{document_id}/metadata")]
pub async fn merge_metadata(
    cx: &Cx,
    Json(input): Json<serde_json::Map<String, serde_json::Value>>,
) -> Result<Json<DocumentResponse>> {
    let mut db = db(cx);
    let id_str = path_param::<DocumentId>(cx);
    let id = uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id"))?;

    let doc = active_document(&mut db, &id).await?;

    for (key, value) in input {
        // Replace any existing row for this key, then insert unless deleting.
        Metadata::filter_by_document_id(doc.id)
            .filter(Metadata::fields().key().eq(&key))
            .delete()
            .exec(&mut db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;

        if !value.is_null() {
            insert_metadata_row(&mut db, doc.id, &key, value).await?;
        }
    }

    build_document_response(&mut db, doc).await
}

#[derive(Debug, Deserialize)]
pub struct GetOrCreateRequest {
    pub title: String,
    /// Used only when creating; ignored when the title already exists.
    pub content: Option<String>,
    /// Used only when creating; ignored when the title already exists.
    pub tags: Option<Vec<String>>,
}

/// Find the active document with this exact title, or create it.
///
/// Collapses the search-then-create dance agents otherwise need for
/// per-topic working documents ("Daily Journal", "Scratch notes", ...).
/// Returns `200` with the existing document, or `201` with the new one
/// (content defaults to empty — append to fill it in).
#[route(POST "/rb/documents/get-or-create")]
pub async fn get_or_create_document(
    cx: &Cx,
    Json(input): Json<GetOrCreateRequest>,
) -> Result<Response> {
    let mut db = db(cx);

    let title = input.title.trim();
    if title.is_empty() {
        return Err(bad_request("title is required").into());
    }

    let existing = Document::filter(Document::fields().title().eq(title))
        .filter(Document::fields().deleted_at().is_none())
        .limit(1)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    if let Some(doc) = existing.into_iter().next() {
        let Json(resp) = build_document_response(&mut db, doc).await?;
        return Json(resp).into_response(cx);
    }

    let content = input.content.as_deref().unwrap_or("");
    let doc = Document::create()
        .title(title)
        .content(content)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    for tag_name in input.tags.unwrap_or_default() {
        let tag = get_or_create_tag(&mut db, &tag_name).await?;
        DocumentTag::create()
            .document_id(doc.id)
            .tag_id(tag.id)
            .exec(&mut db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;
    }

    crate::fts::index_document(&mut db, &doc.id, title, content)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    let Json(resp) = build_document_response(&mut db, doc).await?;
    (StatusCode::CREATED, Json(resp)).into_response(cx)
}

#[route(DELETE "/rb/documents/{document_id}")]
pub async fn delete_document(cx: &Cx) -> Result<Json<serde_json::Value>> {
    let mut db = db(cx);
    let id_str = path_param::<DocumentId>(cx);

    let id = uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id"))?;

    let mut doc = Document::get_by_id(&mut db, &id)
        .await
        .map_err(db_error_or_not_found)?;

    if doc.deleted_at.is_some() {
        return Err(not_found().into());
    }

    // Remove from FTS5 index before soft-deleting
    crate::fts::remove_document(&mut db, &doc.id)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    // Soft delete
    doc.update()
        .deleted_at(Some(jiff::Timestamp::now()))
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ── Response builder ────────────────────────────────────────────────

async fn build_document_response(db: &mut Db, doc: Document) -> Result<Json<DocumentResponse>> {
    // Load tags
    let doc_tags = DocumentTag::filter_by_document_id(doc.id)
        .exec(db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    let mut tag_names = Vec::new();
    for dt in doc_tags {
        let tag = Tag::get_by_id(&mut *db, &dt.tag_id)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;
        tag_names.push(tag.name);
    }

    // Load metadata
    let metadata_rows = Metadata::filter_by_document_id(doc.id)
        .exec(db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

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
