// src/app/document.rs

use serde::Deserialize;
use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        IntoResponse, Response, StatusCode,
        content::Form,
        error::{SeeOther, bad_request, not_found, see_other},
        page, path_param, query_params, route,
    },
    view::{Unescaped, component, view},
};

use crate::api::documents::get_or_create_tag;
use crate::models::{Document, DocumentTag, Metadata, Tag};

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

/// Load all tag names in the knowledge base, for the form's autocomplete list.
async fn load_all_tag_names(db: &mut Db) -> Result<Vec<String>> {
    let tags = Tag::all()
        .exec(&mut *db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;
    Ok(tags.into_iter().map(|tag| tag.name).collect())
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
    uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id").into())
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

// ── Form input ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DocumentFormInput {
    title: String,
    content: String,
    tags: Option<String>,
    metadata: Option<String>,
}

fn parse_tags(tags: &Option<String>) -> Vec<String> {
    tags.as_deref()
        .unwrap_or("")
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// A typed metadata value parsed from the form, mirroring the JSON API's
/// type inference: `true`/`false` → bool, integers → int, anything else → text.
enum MetadataValue {
    Text(String),
    Int(i64),
    Bool(bool),
}

/// Parse the metadata textarea: one `key=value` pair per line.
fn parse_metadata(
    raw: &Option<String>,
) -> std::result::Result<Vec<(String, MetadataValue)>, String> {
    let mut entries = Vec::new();
    for (line_no, line) in raw.as_deref().unwrap_or("").lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "line {} is missing '=' (expected key=value): {:?}",
                line_no + 1,
                line
            ));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("line {} has an empty key", line_no + 1));
        }
        let value = value.trim();
        let typed = if value == "true" {
            MetadataValue::Bool(true)
        } else if value == "false" {
            MetadataValue::Bool(false)
        } else if let Ok(i) = value.parse::<i64>() {
            MetadataValue::Int(i)
        } else {
            MetadataValue::Text(value.to_string())
        };
        entries.push((key.to_string(), typed));
    }
    Ok(entries)
}

/// Validate a form submission, returning the parsed metadata on success.
fn validate_input(
    input: &DocumentFormInput,
) -> std::result::Result<Vec<(String, MetadataValue)>, String> {
    if input.title.trim().is_empty() {
        return Err("Title is required.".to_string());
    }
    if input.content.trim().is_empty() {
        return Err("Content is required.".to_string());
    }
    parse_metadata(&input.metadata).map_err(|e| format!("Invalid metadata: {e}"))
}

/// Format stored metadata rows back into `key=value` lines for the edit form.
fn format_metadata(pairs: &[(String, String)]) -> String {
    let mut out = String::new();
    for (key, value) in pairs {
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    out
}

/// Replace a document's tags with the given names.
async fn replace_tags(db: &mut Db, doc_id: uuid::Uuid, tag_names: &[String]) -> Result<()> {
    DocumentTag::filter_by_document_id(doc_id)
        .delete()
        .exec(&mut *db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    for tag_name in tag_names {
        let tag = get_or_create_tag(&mut *db, tag_name).await?;
        DocumentTag::create()
            .document_id(doc_id)
            .tag_id(tag.id)
            .exec(&mut *db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;
    }
    Ok(())
}

/// Replace a document's metadata with the given typed entries.
async fn replace_metadata(
    db: &mut Db,
    doc_id: uuid::Uuid,
    entries: &[(String, MetadataValue)],
) -> Result<()> {
    Metadata::filter_by_document_id(doc_id)
        .delete()
        .exec(&mut *db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    for (key, value) in entries {
        let builder = Metadata::create().document_id(doc_id).key(key);
        let builder = match value {
            MetadataValue::Text(s) => builder.value_text(s),
            MetadataValue::Int(i) => builder.value_int(*i),
            MetadataValue::Bool(b) => builder.value_bool(*b),
        };
        builder
            .exec(&mut *db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;
    }
    Ok(())
}

// ── Form ────────────────────────────────────────────

/// The user-entered values (and any validation error) the form should display.
struct DocumentFormState {
    title: String,
    content: String,
    tags: String,
    metadata: String,
    error: Option<String>,
}

impl DocumentFormState {
    /// A blank form, as on the "new document" page.
    fn blank() -> Self {
        Self {
            title: String::new(),
            content: String::new(),
            tags: String::new(),
            metadata: String::new(),
            error: None,
        }
    }

    /// What the user just submitted, preserved for an error re-render.
    fn from_input(input: &DocumentFormInput, error: String) -> Self {
        Self {
            title: input.title.clone(),
            content: input.content.clone(),
            tags: input.tags.clone().unwrap_or_default(),
            metadata: input.metadata.clone().unwrap_or_default(),
            error: Some(error),
        }
    }
}

#[component]
async fn document_form(
    action: &str,
    cancel_href: &str,
    submit_label: &str,
    state: &DocumentFormState,
    all_tags: &[String],
) -> Result {
    view! {
        if state.error.is_some() {
            <div class="error" role="alert">(state.error.as_deref().unwrap_or_default())</div>
        }
        <div class="card">
            <form method="post" action=(action)>
                <div class="form-group">
                    <label for="title">"Title"</label>
                    <input type="text" id="title" name="title" value=(state.title.clone()) required=(true)>
                </div>
                <div class="form-group">
                    <label for="content">"Content"</label>
                    <textarea id="content" name="content" required=(true)>(state.content.clone())</textarea>
                    <p class="form-hint">"Markdown is rendered on the document page."</p>
                </div>
                <div class="form-group">
                    <label for="tags">"Tags (comma separated)"</label>
                    <input type="text" id="tags" name="tags" value=(state.tags.clone()) list="tag-suggestions" placeholder="tag1, tag2, tag3">
                    <datalist id="tag-suggestions">
                        for tag in all_tags {
                            <option value=(tag)></option>
                        }
                    </datalist>
                </div>
                <div class="form-group">
                    <label for="metadata">"Metadata (one key=value per line)"</label>
                    <textarea id="metadata" name="metadata" class="metadata-input" placeholder="priority=high&#10;version=2&#10;pinned=true">(state.metadata.clone())</textarea>
                    <p class="form-hint">"Values are typed automatically: true/false become booleans, integers become numbers, everything else is text."</p>
                </div>
                <div class="form-actions">
                    <a class="btn btn-secondary" href=(cancel_href)>"Cancel"</a>
                    <button type="submit" class="btn btn-primary">(submit_label)</button>
                </div>
            </form>
        </div>
    }
}

/// A full form page: heading plus the form.
async fn form_page(
    cx: &Cx,
    heading: &str,
    action: &str,
    cancel_href: &str,
    submit_label: &str,
    state: &DocumentFormState,
    all_tags: &[String],
) -> Result {
    view! { cx =>
        <h2 class="page-title">(heading)</h2>
        document_form(
            action: action,
            cancel_href: cancel_href,
            submit_label: submit_label,
            state: state,
            all_tags: all_tags
        )
    }
}

// ── Pages ───────────────────────────────────────────

#[page("/documents/new")]
pub async fn new_document(cx: &Cx) -> Result {
    let mut db = db(cx);
    let all_tags = load_all_tag_names(&mut db).await?;

    form_page(
        cx,
        "New Document",
        "/documents",
        "/",
        "Create",
        &DocumentFormState::blank(),
        &all_tags,
    )
    .await
}

#[page("/documents/{document_id}")]
pub async fn view_document(cx: &Cx) -> Result {
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

    view! {
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
    }
}

#[page("/documents/{document_id}/edit")]
pub async fn edit_document(cx: &Cx) -> Result {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let doc = load_active_document(&mut db, &id).await?;
    let tags = load_tag_names(&mut db, doc.id).await?.join(", ");
    let metadata = format_metadata(&load_metadata_pairs(&mut db, doc.id).await?);
    let all_tags = load_all_tag_names(&mut db).await?;

    let action = format!("/documents/{}/edit", doc.id);
    let cancel_href = format!("/documents/{}", doc.id);
    let state = DocumentFormState {
        title: doc.title.clone(),
        content: doc.content.clone(),
        tags,
        metadata,
        error: None,
    };

    form_page(
        cx,
        "Edit Document",
        &action,
        &cancel_href,
        "Save Changes",
        &state,
        &all_tags,
    )
    .await
}

// ── Form handlers (POST → redirect → GET) ───────────

#[route(POST "/documents")]
pub async fn create_document(cx: &Cx, Form(input): Form<DocumentFormInput>) -> Result<Response> {
    let mut db = db(cx);

    // Re-render the form with an error banner on invalid input, preserving
    // what the user typed.
    let entries = match validate_input(&input) {
        Ok(entries) => entries,
        Err(error) => {
            let state = DocumentFormState::from_input(&input, error);
            let all_tags = load_all_tag_names(&mut db).await?;
            let page = form_page(
                cx,
                "New Document",
                "/documents",
                "/",
                "Create",
                &state,
                &all_tags,
            )
            .await?;
            return (StatusCode::UNPROCESSABLE_ENTITY, page).into_response(cx);
        }
    };

    let doc = Document::create()
        .title(input.title.trim())
        .content(&input.content)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    replace_tags(&mut db, doc.id, &parse_tags(&input.tags)).await?;
    replace_metadata(&mut db, doc.id, &entries).await?;

    see_other(&format!("/documents/{}?saved=1", doc.id)).into_response(cx)
}

#[route(POST "/documents/{document_id}/edit")]
pub async fn update_document(cx: &Cx, Form(input): Form<DocumentFormInput>) -> Result<Response> {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let mut doc = load_active_document(&mut db, &id).await?;

    let entries = match validate_input(&input) {
        Ok(entries) => entries,
        Err(error) => {
            let action = format!("/documents/{}/edit", doc.id);
            let cancel_href = format!("/documents/{}", doc.id);
            let state = DocumentFormState::from_input(&input, error);
            let all_tags = load_all_tag_names(&mut db).await?;
            let page = form_page(
                cx,
                "Edit Document",
                &action,
                &cancel_href,
                "Save Changes",
                &state,
                &all_tags,
            )
            .await?;
            return (StatusCode::UNPROCESSABLE_ENTITY, page).into_response(cx);
        }
    };

    doc.update()
        .title(input.title.trim())
        .content(&input.content)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    replace_tags(&mut db, doc.id, &parse_tags(&input.tags)).await?;
    replace_metadata(&mut db, doc.id, &entries).await?;

    see_other(&format!("/documents/{}?saved=1", doc.id)).into_response(cx)
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
        .map_err(topcoat::router::error::internal_server_error)?;

    Ok(see_other("/"))
}
