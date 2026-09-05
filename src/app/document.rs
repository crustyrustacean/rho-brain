// src/app/document.rs
//
// The document view page and delete handler, plus the shared loading helpers
// used by the home page and the editor.

use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        error::{SeeOther, bad_request, not_found, see_other},
        page, path_param, query_params, route,
    },
    view::{Unescaped, View, view},
};

use crate::models::{Document, DocumentTag, Metadata, Tag};

use super::markdown::render_markdown;

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

// ── Shared helpers ──────────────────────────────────

/// Load the tag names attached to a document.
pub(crate) async fn load_tag_names(db: &mut Db, doc_id: uuid::Uuid) -> Result<Vec<String>> {
    let doc_tags = DocumentTag::filter_by_document_id(doc_id)
        .exec(&mut *db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    let mut names = Vec::new();
    for dt in doc_tags {
        let tag = Tag::get_by_id(&mut *db, &dt.tag_id)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;
        names.push(tag.name);
    }
    Ok(names)
}

/// Load a document's metadata as ordered display pairs.
pub(crate) async fn load_metadata_pairs(
    db: &mut Db,
    doc_id: uuid::Uuid,
) -> Result<Vec<(String, String)>> {
    let rows = Metadata::filter_by_document_id(doc_id)
        .exec(&mut *db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let value = row
                .value_text
                .or_else(|| row.value_int.map(|i| i.to_string()))
                .or_else(|| row.value_bool.map(|b| b.to_string()))?;
            Some((row.key, value))
        })
        .collect())
}

// ── Path params ─────────────────────────────────────

path_param!(document_id);

fn document_id(cx: &Cx) -> Result<uuid::Uuid> {
    let id_str = path_param::<DocumentId>(cx);
    uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id").into())
}

pub(crate) async fn load_active_document(db: &mut Db, id: &uuid::Uuid) -> Result<Document> {
    let doc = Document::get_by_id(&mut *db, id)
        .await
        .map_err(|_| not_found())?;

    if doc.deleted_at.is_some() {
        return Err(not_found().into());
    }
    Ok(doc)
}

// ── Pages ───────────────────────────────────────────

#[page("/documents/{document_id}")]
pub async fn view_document(cx: &Cx) -> Result<impl View> {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let doc = load_active_document(&mut db, &id).await?;
    let tags = load_tag_names(&mut db, doc.id).await?;
    let metadata = load_metadata_pairs(&mut db, doc.id).await?;

    #[query_params(error = bad_request)]
    struct ViewQuery {
        saved: Option<String>,
    }
    let saved = query_params::<ViewQuery>(cx)
        .ok()
        .and_then(|q| q.saved.clone())
        .is_some();

    let edit_href = format!("/documents/{}/edit", doc.id);
    let delete_action = format!("/documents/{}/delete", doc.id);
    let created = doc.created_at.strftime("%Y-%m-%d %H:%M").to_string();
    let updated = doc.updated_at.strftime("%Y-%m-%d %H:%M").to_string();
    let content_html = render_markdown(&doc.content);

    Ok(view! {
        if saved {
            <div class="flash" role="status">"Document saved."</div>
        }
        <div class="card document-detail">
            <div class="detail-header">
                <h2 class="document-title">(doc.title)</h2>
                <div class="detail-actions">
                    <a class="btn btn-primary" href=(edit_href)>"Edit"</a>
                    <form method="post" action=(delete_action) onsubmit="return confirm('Delete this document? This cannot be undone.')">
                        <button type="submit" class="btn btn-danger">"Delete"</button>
                    </form>
                </div>
            </div>
            <div class="document-meta">
                <span>"Created: " (created)</span>
                <span>"Updated: " (updated)</span>
            </div>
            if !tags.is_empty() {
                <div class="tag-list">
                    for tag in tags {
                        <a class="tag" href=(format!("/?tag={}", tag))>(tag)</a>
                    }
                </div>
            }
            if !metadata.is_empty() {
                <dl class="metadata-list">
                    for (key, value) in metadata {
                        <div class="metadata-row">
                            <dt>(key)</dt>
                            <dd>(value)</dd>
                        </div>
                    }
                </dl>
            }
            <div class="markdown-content">(Unescaped::new_unchecked(content_html))</div>
        </div>
    })
}

// ── Form handlers (POST → redirect → GET) ───────────

#[route(POST "/documents/{document_id}/delete")]
pub async fn delete_document(cx: &Cx) -> Result<SeeOther> {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let mut doc = load_active_document(&mut db, &id).await?;

    // Remove from FTS5 index before soft-deleting
    crate::fts::remove_document(&mut db, &doc.id)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    doc.update()
        .deleted_at(Some(jiff::Timestamp::now()))
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    Ok(see_other("/"))
}
