// src/app/editor.rs
//
// The document editor: the new/edit form pages and their POST handlers.
// Split out of document.rs so the view page stays focused on presentation
// while the editor grows preview and draft support.

use serde::Deserialize;
use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    datastar::{PatchElements, Signals},
    router::{
        StatusCode,
        content::Form,
        error::{bad_request, see_other},
        page, path_param,
        response::{IntoResponse, Response},
        route,
    },
    view::{Unescaped, component, view},
};

use crate::api::documents::get_or_create_tag;
use crate::models::{Document, DocumentTag, Metadata, Tag};

use super::document::{load_active_document, load_metadata_pairs, load_tag_names};
use super::markdown::render_markdown;

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

// ── Path params ─────────────────────────────────────

path_param!(document_id);

fn document_id(cx: &Cx) -> Result<uuid::Uuid> {
    let id_str = path_param::<DocumentId>(cx);
    uuid::Uuid::parse_str(id_str).map_err(|_| bad_request("invalid document id").into())
}

// ── Form input ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DocumentFormInput {
    title: String,
    /// Optional so a title-only draft can be started (e.g. from a phone)
    /// and filled in later; missing and empty are the same thing.
    content: Option<String>,
    tags: Option<String>,
    metadata: Option<String>,
}

impl DocumentFormInput {
    fn content(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }
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

/// Word count for the editor's live counter, matching the client-side count:
/// whitespace-separated, non-empty runs.
fn word_count(content: &str) -> usize {
    content.split_whitespace().count()
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
            content: input.content.clone().unwrap_or_default(),
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
    draft_key: &str,
    state: &DocumentFormState,
    all_tags: &[String],
) -> Result {
    // The preview pane starts populated so the split view is correct before
    // any Datastar round-trip.
    let initial_preview = if state.content.trim().is_empty() {
        String::new()
    } else {
        render_markdown(&state.content)
    };

    // Server-rendered so the counter is correct before any JavaScript runs;
    // editor.js keeps it live from then on.
    let words = word_count(&state.content);

    view! {
        if state.error.is_some() {
            <div class="error" role="alert">(state.error.as_deref().unwrap_or_default())</div>
        }
        <div class="card editor-card">
            <form method="post" action=(action) data-draft=(draft_key)>
                <div class="form-group">
                    <label for="title">"Title"</label>
                    <input type="text" id="title" name="title" value=(state.title.clone()) required=(true)>
                </div>
                <div class="editor" data-signals:mode="'split'">
                    <div class="editor-toolbar" role="toolbar" aria-label="Editor view mode">
                        <button type="button" class="mode-btn" data-class:active="$mode === 'write'" data-on:click="$mode = 'write'">"Write"</button>
                        <button type="button" class="mode-btn active" data-class:active="$mode === 'split'" data-on:click="$mode = 'split'">"Split"</button>
                        <button type="button" class="mode-btn" data-class:active="$mode === 'preview'" data-on:click="$mode = 'preview'">"Preview"</button>
                    </div>
                    <div class="editor-panes split" data-class:split="$mode === 'split'">
                        <div class="editor-pane editor-write-pane" data-show="$mode !== 'preview'">
                            <textarea id="content" name="content" class="editor-textarea" placeholder="Write markdown here — the preview updates as you type. Leave empty to save a title-only draft." data-bind="content" data-on:input__debounce.400ms="@post('/documents/preview')">(state.content.clone())</textarea>
                            <div class="editor-statusline">
                                <p class="form-hint">"Markdown is rendered on the document page."</p>
                                <span id="word-count" class="word-count">
                                    if words == 1 {
                                        "1 word"
                                    } else {
                                        (words) " words"
                                    }
                                </span>
                            </div>
                        </div>
                        <div class="editor-pane editor-preview-pane" data-show="$mode !== 'write'">
                            <div id="preview" class="markdown-content">(Unescaped::new_unchecked(initial_preview))</div>
                        </div>
                    </div>
                </div>
                <details class="editor-extras">
                    <summary>"Tags &amp; metadata"</summary>
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
                </details>
                <div class="form-actions editor-actions">
                    <a class="btn btn-secondary" href=(cancel_href)>"Cancel"</a>
                    <button type="submit" class="btn btn-primary">(submit_label)</button>
                </div>
            </form>
        </div>
        <script src="/js/editor.js" defer=""></script>
    }
}

/// Everything that varies between the new-document and edit-document forms.
struct FormSpec {
    heading: &'static str,
    action: String,
    cancel_href: String,
    submit_label: &'static str,
    /// Key the client-side draft autosave stores under ("new" or the id).
    draft_key: String,
}

/// A full form page: heading plus the form.
async fn form_page(
    cx: &Cx,
    spec: &FormSpec,
    state: &DocumentFormState,
    all_tags: &[String],
) -> Result {
    view! { cx =>
        <h2 class="page-title">(spec.heading)</h2>
        document_form(
            action: spec.action.as_str(),
            cancel_href: spec.cancel_href.as_str(),
            submit_label: spec.submit_label,
            draft_key: spec.draft_key.as_str(),
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
        &FormSpec {
            heading: "New Document",
            action: "/documents".into(),
            cancel_href: "/".into(),
            submit_label: "Create",
            draft_key: "new".into(),
        },
        &DocumentFormState::blank(),
        &all_tags,
    )
    .await
}

#[page("/documents/{document_id}/edit")]
pub async fn edit_document(cx: &Cx) -> Result {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let doc = load_active_document(&mut db, &id).await?;
    let tags = load_tag_names(&mut db, doc.id).await?.join(", ");
    let metadata = format_metadata(&load_metadata_pairs(&mut db, doc.id).await?);
    let all_tags = load_all_tag_names(&mut db).await?;

    let spec = FormSpec {
        heading: "Edit Document",
        action: format!("/documents/{}/edit", doc.id),
        cancel_href: format!("/documents/{}", doc.id),
        submit_label: "Save Changes",
        draft_key: doc.id.to_string(),
    };
    let state = DocumentFormState {
        title: doc.title.clone(),
        content: doc.content.clone(),
        tags,
        metadata,
        error: None,
    };

    form_page(cx, &spec, &state, &all_tags).await
}

// ── Live preview ───────────────────────────────────

/// The signals the editor page keeps. The preview action sends the whole
/// store; only `content` matters here and every other signal is ignored.
#[derive(Debug, Deserialize)]
struct PreviewSignals {
    content: Option<String>,
}

/// Render the editor's `content` signal and return it as a patch of the
/// `#preview` element.
///
/// Uses the same renderer as the document view page, so what the preview
/// shows is exactly what the saved page will render.
#[route(POST "/documents/preview")]
pub async fn preview_document(
    cx: &Cx,
    Signals(input): Signals<PreviewSignals>,
) -> Result<PatchElements> {
    let content = input.content.as_deref().unwrap_or("");
    let rendered = if content.trim().is_empty() {
        String::new()
    } else {
        render_markdown(content)
    };

    let fragment = view! { cx =>
        <div id="preview" class="markdown-content">(Unescaped::new_unchecked(rendered))</div>
    }?;
    Ok(PatchElements::new(fragment.render(cx)))
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
                &FormSpec {
                    heading: "New Document",
                    action: "/documents".into(),
                    cancel_href: "/".into(),
                    submit_label: "Create",
                    draft_key: "new".into(),
                },
                &state,
                &all_tags,
            )
            .await?;
            return (StatusCode::UNPROCESSABLE_ENTITY, page).into_response(cx);
        }
    };

    let doc = Document::create()
        .title(input.title.trim())
        .content(input.content())
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    replace_tags(&mut db, doc.id, &parse_tags(&input.tags)).await?;
    replace_metadata(&mut db, doc.id, &entries).await?;

    // Index in FTS5
    crate::fts::index_document(&mut db, &doc.id, input.title.trim(), input.content())
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    see_other(format!("/documents/{}?saved=1", doc.id)).into_response(cx)
}

#[route(POST "/documents/{document_id}/edit")]
pub async fn update_document(cx: &Cx, Form(input): Form<DocumentFormInput>) -> Result<Response> {
    let mut db = db(cx);
    let id = document_id(cx)?;
    let mut doc = load_active_document(&mut db, &id).await?;

    let entries = match validate_input(&input) {
        Ok(entries) => entries,
        Err(error) => {
            let spec = FormSpec {
                heading: "Edit Document",
                action: format!("/documents/{}/edit", doc.id),
                cancel_href: format!("/documents/{}", doc.id),
                submit_label: "Save Changes",
                draft_key: doc.id.to_string(),
            };
            let state = DocumentFormState::from_input(&input, error);
            let all_tags = load_all_tag_names(&mut db).await?;
            let page = form_page(cx, &spec, &state, &all_tags).await?;
            return (StatusCode::UNPROCESSABLE_ENTITY, page).into_response(cx);
        }
    };

    doc.update()
        .title(input.title.trim())
        .content(input.content())
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    replace_tags(&mut db, doc.id, &parse_tags(&input.tags)).await?;
    replace_metadata(&mut db, doc.id, &entries).await?;

    // Sync FTS5 index
    crate::fts::update_document_index(&mut db, &doc.id, input.title.trim(), input.content())
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    see_other(format!("/documents/{}?saved=1", doc.id)).into_response(cx)
}

/// Load all tag names in the knowledge base, for the form's autocomplete list.
async fn load_all_tag_names(db: &mut Db) -> Result<Vec<String>> {
    let tags = Tag::all()
        .exec(&mut *db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;
    Ok(tags.into_iter().map(|tag| tag.name).collect())
}
