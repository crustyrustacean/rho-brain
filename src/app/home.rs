// src/app/home.rs

use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{page, query_params},
    view::{component, view},
};

use super::document::load_tag_names;
use crate::models::{Document, DocumentTag, Tag};

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

// ── Query params ────────────────────────────────────

#[query_params(error = bad_request)]
struct HomeQuery {
    q: Option<String>,
    tag: Option<String>,
}

// ── Components ──────────────────────────────────────

#[component]
async fn stats_display(total: usize, unique_tags: usize, active: usize) -> Result {
    view! {
        <div class="stats-grid">
            <div class="stat-card">
                <div class="stat-value">(total)</div>
                <div class="stat-label">"Total Documents"</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">(unique_tags)</div>
                <div class="stat-label">"Unique Tags"</div>
            </div>
            <div class="stat-card">
                <div class="stat-value">(active)</div>
                <div class="stat-label">"Active Documents"</div>
            </div>
        </div>
    }
}

#[component]
async fn search_bar(query: &str) -> Result {
    view! {
        <form class="search-bar" method="get" action="/">
            <input type="text" name="q" placeholder="Search documents..." value=(query)>
            <button type="submit" class="btn btn-primary">"Search"</button>
        </form>
    }
}

struct DocumentCard {
    id: String,
    title: String,
    excerpt: String,
    updated: String,
    tags: Vec<String>,
}

#[component]
async fn document_card(card: DocumentCard) -> Result {
    let href = format!("/documents/{}", card.id);
    view! {
        <article class="document-item">
            <a class="document-title" href=(href)>(card.title)</a>
            <div class="document-content">(card.excerpt)</div>
            <div class="document-meta">
                <span>"Updated: " (card.updated)</span>
            </div>
            if !card.tags.is_empty() {
                <div class="tag-list">
                    for tag in card.tags {
                        <a class="tag" href=(format!("/?tag={}", tag))>(tag)</a>
                    }
                </div>
            }
        </article>
    }
}

// ── Page ────────────────────────────────────────────

#[page("/")]
pub async fn home(cx: &Cx) -> Result {
    let mut db = db(cx);

    let params = query_params::<HomeQuery>(cx).ok();
    let search = params
        .as_ref()
        .and_then(|p| p.q.clone())
        .filter(|q| !q.trim().is_empty());
    let tag_filter = params
        .and_then(|p| p.tag.clone())
        .filter(|t| !t.trim().is_empty());

    // Stats
    let all_docs = Document::all()
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;
    let total = all_docs.len();
    let active = all_docs.iter().filter(|d| d.deleted_at.is_none()).count();

    let unique_tags = Tag::all()
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?
        .len();

    // Documents: text search and/or tag filter, or the 50 most recent active docs
    let mut query = Document::all().filter(Document::fields().deleted_at().is_none());
    if let Some(q) = &search {
        let pattern = format!("%{}%", q);
        query = query.filter(
            Document::fields()
                .title()
                .like(&pattern)
                .or(Document::fields().content().like(&pattern)),
        );
    }

    // Tag filter: an unknown tag name matches no documents. (Toasty 0.9
    // mis-lowers a `belongs_to` chain inside `any()` (`tag().name()` panics /
    // mis-matches), so resolve the name to its id and filter the join table on
    // the primitive `tag_id`.)
    let unknown_tag = if let Some(tag_name) = &tag_filter {
        match Tag::get_by_name(&mut db, tag_name).await {
            Ok(tag) => {
                query = query.filter(
                    Document::fields()
                        .document_tags()
                        .any(DocumentTag::fields().tag_id().eq(tag.id)),
                );
                false
            }
            Err(_) => true,
        }
    } else {
        false
    };

    let docs = if unknown_tag {
        Vec::new()
    } else {
        query
            .limit(50)
            .exec(&mut db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?
    };

    let mut cards = Vec::new();
    for doc in docs {
        let tags = load_tag_names(&mut db, doc.id).await?;
        let excerpt = if doc.content.chars().count() > 200 {
            format!("{}...", doc.content.chars().take(200).collect::<String>())
        } else {
            doc.content.clone()
        };
        cards.push(DocumentCard {
            id: doc.id.to_string(),
            title: doc.title.clone(),
            excerpt,
            updated: doc.updated_at.strftime("%Y-%m-%d %H:%M").to_string(),
            tags,
        });
    }

    let query_str = search.clone().unwrap_or_default();
    let searching = search.is_some();
    let filtering = tag_filter.is_some();
    let no_results = cards.is_empty();

    view! {
        stats_display(total: total, unique_tags: unique_tags, active: active)

        search_bar(query: &query_str)

        if searching || filtering {
            <div class="filter-bar">
                <span class="filter-label">"Filters:"</span>
                if let Some(tag) = &tag_filter {
                    <span class="tag">"#"(tag)</span>
                }
                if searching {
                    <span class="tag">"\""(query_str)"\""</span>
                }
                <a class="btn btn-secondary btn-small" href="/">"Clear"</a>
            </div>
        }

        <div class="actions">
            <a class="btn btn-primary" href="/documents/new">"+ New Document"</a>
        </div>

        <div class="document-list">
            for card in cards {
                document_card(card: card)
            }
        </div>

        if no_results {
            <div class="card empty-state">
                <h3>"No documents found"</h3>
                <p>"Create your first document to get started!"</p>
            </div>
        }
    }
}
