// src/app/home.rs

use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{page, query_params},
    view::{component, view},
};

use super::document::load_tag_names;
use crate::models::{Document, Tag};

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

// ── Query params ────────────────────────────────────

#[query_params(error = bad_request)]
struct HomeQuery {
    q: Option<String>,
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
        <a class="document-item" href=(href)>
            <div class="document-title">(card.title)</div>
            <div class="document-content">(card.excerpt)</div>
            <div class="document-meta">
                <span>"Updated: " (card.updated)</span>
            </div>
            if !card.tags.is_empty() {
                <div>
                    for tag in card.tags {
                        <span class="tag">(tag)</span>
                    }
                </div>
            }
        </a>
    }
}

// ── Page ────────────────────────────────────────────

#[page("/")]
pub async fn home(cx: &Cx) -> Result {
    let mut db = db(cx);

    let search = query_params::<HomeQuery>(cx)
        .ok()
        .and_then(|query| query.q.clone())
        .filter(|q| !q.trim().is_empty());

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

    // Documents: full-text-ish search, or the 50 most recent active docs
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

    let docs = query
        .limit(50)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

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
    let no_results = cards.is_empty();

    view! {
        stats_display(total: total, unique_tags: unique_tags, active: active)

        search_bar(query: &query_str)

        <div class="actions">
            <a class="btn btn-primary" href="/documents/new">"+ New Document"</a>
            if searching {
                <a class="btn btn-secondary" href="/">"Clear Search"</a>
            }
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
