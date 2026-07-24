// src/app/document.rs

use serde::Deserialize;
use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        Form, SeeOther, bad_request, not_found, page, path_param, route, see_other,
    },
    view::{Unescaped, component, view},
};

use crate::api::documents::get_or_create_tag;
use crate::models::{Document, DocumentTag, Tag};

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

// ── Shared helpers ──────────────────────────────────

/// Load the tag names attached to a document.
pub(crate) async fn load_tag_names(db: &mut Db, doc_id: uuid::Uuid) -> Result<Vec<String>> {
    let doc_tags = DocumentTag::filter_by_document_id(doc_id)
        .exec(&mut *db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    let mut names = Vec::new();
    for dt in doc_tags {
        let tag = Tag::get_by_id(&mut *db, &dt.tag_id)
            .await
            .map_err(topcoat::router::internal_server_error)?;
        names.push(tag.name);
    }
    Ok(names)
}

/// Render markdown to HTML, mirroring pi-brain's document view.
fn render_markdown(content: &str) -> String {
    use pulldown_cmark::{Options, Parser, html};
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(content, options);
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

// ── Path params ─────────────────────────────────────

#[path_param(error = bad_request)]
struct DocumentId(String);

fn document_id(cx: &Cx) -> Result<uuid::Uuid> {
    let id_str = path_param::<DocumentId>(cx)?;
    uuid::Uuid::parse_str(&id_str).map_err(|_| bad_request("invalid document id").into())
}

async fn load_active_document(db: &mut Db, id: &uuid::Uuid) -> Result<Document> {
    let doc = Document::get_by_id(&mut *db, id)
        .await
        .map_err(|_| not_found())?;

    if doc.deleted_at.is_some() {
        return Err(not_found().into());
    }
    Ok(doc)
}

// ── Form ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DocumentFormInput {
    title: String,
    content: String,
    tags: Option<String>,
}

fn parse_tags(tags: &Option<String>) -> Vec<String> {
    tags.as_deref()
        .unwrap_or("")
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

#[component]
async fn document_form(action: &str, title: &str, content: &str, tags: &str) -> Result {
    view! {
        <div class="card">
            <form method="post" action=(action)>
                <div class="form-group">
                    <label for="title">"Title"</label>
                    <input type="text" id="title" name="title" value=(title) required=(true)>
                </div>
                <div class="form-group">
                    <label for="content">"Content"</label>
                    <textarea id="content" name="content" required=(true)>(content)</textarea>
                </div>
                <div class="form-group">
                    <label for="tags">"Tags (comma separated)"</label>
                    <input type="text" id="tags" name="tags" value=(tags) placeholder="tag1, tag2, tag3">
                </div>
                <div class="form-actions">
                    <a class="btn btn-secondary" href="/">"Cancel"</a>
                    <button type="submit" class="btn btn-primary">"Save"</button>
                </div>
            </form>
        </div>
    }
}

// ── Pages ───────────────────────────────────────────

#[page("/documents/new")]
pub async fn new_document() -> Result {
    view! {
        <h2 class="page-title">"New Document"</h2>
        document_form(action: "/documents", title: "", content: "", tags: "")
    }
}

#[page("/documents/{document_id}")]
pub async fn view_document(cx: &Cx) -> Result {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let doc = load_active_document(&mut db, &id).await?;
    let tags = load_tag_names(&mut db, doc.id).await?;

    let edit_href = format!("/documents/{}/edit", doc.id);
    let delete_action = format!("/documents/{}/delete", doc.id);
    let created = doc.created_at.strftime("%Y-%m-%d %H:%M").to_string();
    let updated = doc.updated_at.strftime("%Y-%m-%d %H:%M").to_string();
    let content_html = render_markdown(&doc.content);

    view! {
        <div class="card document-detail">
            <div class="detail-header">
                <h2 class="document-title">(doc.title)</h2>
                <div class="detail-actions">
                    <a class="btn btn-primary" href=(edit_href)>"Edit"</a>
                    <form method="post" action=(delete_action)>
                        <button type="submit" class="btn btn-danger">"Delete"</button>
                    </form>
                </div>
            </div>
            <div class="document-meta">
                <span>"Created: " (created)</span>
                <span>"Updated: " (updated)</span>
            </div>
            if !tags.is_empty() {
                <div>
                    for tag in tags {
                        <span class="tag">(tag)</span>
                    }
                </div>
            }
            <div class="markdown-content">(Unescaped::new_unchecked(content_html))</div>
        </div>
    }
}

#[page("/documents/{document_id}/edit")]
pub async fn edit_document(cx: &Cx) -> Result {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let doc = load_active_document(&mut db, &id).await?;
    let tags = load_tag_names(&mut db, doc.id).await?.join(", ");

    let action = format!("/documents/{}/edit", doc.id);

    view! {
        <h2 class="page-title">"Edit Document"</h2>
        document_form(action: &action, title: &doc.title, content: &doc.content, tags: &tags)
    }
}

// ── Form handlers (POST → redirect → GET) ───────────

#[route(POST "/documents")]
pub async fn create_document(cx: &Cx, Form(input): Form<DocumentFormInput>) -> Result<SeeOther> {
    let mut db = db(cx);

    let doc = Document::create()
        .title(&input.title)
        .content(&input.content)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    for tag_name in parse_tags(&input.tags) {
        let tag = get_or_create_tag(&mut db, &tag_name).await?;
        DocumentTag::create()
            .document_id(doc.id)
            .tag_id(tag.id)
            .exec(&mut db)
            .await
            .map_err(topcoat::router::internal_server_error)?;
    }

    Ok(see_other(&format!("/documents/{}", doc.id)))
}

#[route(POST "/documents/{document_id}/edit")]
pub async fn update_document(cx: &Cx, Form(input): Form<DocumentFormInput>) -> Result<SeeOther> {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let mut doc = load_active_document(&mut db, &id).await?;

    doc.update()
        .title(&input.title)
        .content(&input.content)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    // Replace tags
    DocumentTag::filter_by_document_id(doc.id)
        .delete()
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    for tag_name in parse_tags(&input.tags) {
        let tag = get_or_create_tag(&mut db, &tag_name).await?;
        DocumentTag::create()
            .document_id(doc.id)
            .tag_id(tag.id)
            .exec(&mut db)
            .await
            .map_err(topcoat::router::internal_server_error)?;
    }

    Ok(see_other(&format!("/documents/{}", doc.id)))
}

#[route(POST "/documents/{document_id}/delete")]
pub async fn delete_document(cx: &Cx) -> Result<SeeOther> {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let mut doc = load_active_document(&mut db, &id).await?;

    doc.update()
        .deleted_at(Some(jiff::Timestamp::now()))
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    Ok(see_other("/"))
}
