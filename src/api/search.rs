// src/api/search.rs

use serde::{Deserialize, Serialize};
use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{content::Json, error::bad_request, query_params, route},
};

use crate::models::{Document, DocumentTag, Tag};

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub tags: Option<Vec<String>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub excerpt: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub query: String,
    pub limit: usize,
    pub offset: usize,
}

#[route(POST "/rb/search")]
pub async fn search_post(
    cx: &Cx,
    Json(input): Json<SearchRequest>,
) -> Result<Json<SearchResponse>> {
    perform_search(cx, input).await
}

#[route(GET "/rb/search")]
pub async fn search_get(cx: &Cx) -> Result<Json<SearchResponse>> {
    #[query_params(error = bad_request)]
    struct SearchQuery {
        q: String,
        limit: Option<usize>,
        offset: Option<usize>,
    }

    let params =
        query_params::<SearchQuery>(cx).map_err(|_| bad_request("missing query parameter 'q'"))?;

    let input = SearchRequest {
        query: params.q.clone(),
        tags: None,
        limit: params.limit,
        offset: params.offset,
    };

    perform_search(cx, input).await
}

async fn perform_search(cx: &Cx, input: SearchRequest) -> Result<Json<SearchResponse>> {
    let mut db = db(cx);
    let limit = input.limit.unwrap_or(20);
    let offset = input.offset.unwrap_or(0);
    let query = input.query.trim();

    // Resolve tag filter names to IDs if provided (workaround for toasty's
    // `.any()` limitation — filter by primitive tag_id instead).
    let tag_ids: Option<Vec<i64>> = if let Some(tags) = input.tags {
        if tags.is_empty() {
            None
        } else {
            let matching = Tag::all()
                .filter(Tag::fields().name().in_list(tags))
                .exec(&mut db)
                .await
                .map_err(topcoat::router::error::internal_server_error)?;

            if matching.is_empty() {
                // No matching tags — return empty early
                return Ok(Json(SearchResponse {
                    results: Vec::new(),
                    query: input.query,
                    limit,
                    offset,
                }));
            }
            Some(matching.iter().map(|t| t.id).collect())
        }
    } else {
        None
    };

    // Search via FTS5 and gather doc IDs
    let fts_results = crate::fts::search(&mut db, query, limit, offset)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    if fts_results.is_empty() {
        return Ok(Json(SearchResponse {
            results: Vec::new(),
            query: input.query,
            limit,
            offset,
        }));
    }

    // Filter results by tag if requested
    let for_each = if let Some(tids) = tag_ids {
        // Keep only FTS results whose document has at least one matching tag
        let mut filtered = Vec::new();
        'next: for fr in fts_results {
            for tid in &tids {
                let matches = DocumentTag::filter_by_document_id(fr.doc_id)
                    .filter(DocumentTag::fields().tag_id().eq(*tid))
                    .limit(1)
                    .exec(&mut db)
                    .await
                    .map_err(topcoat::router::error::internal_server_error)?;
                if !matches.is_empty() {
                    filtered.push(fr);
                    continue 'next;
                }
            }
        }
        filtered
    } else {
        fts_results
    };

    // For each result, fetch the document (to check deleted_at) and its tags.
    let mut results = Vec::new();
    for fr in for_each {
        // Fetch the document — skip if it's been soft-deleted since indexing
        let doc = match Document::get_by_id(&mut db, &fr.doc_id).await {
            Ok(d) if d.deleted_at.is_none() => d,
            _ => continue,
        };

        // Load tags for each result
        let doc_tags = DocumentTag::filter_by_document_id(doc.id)
            .exec(&mut db)
            .await
            .map_err(topcoat::router::error::internal_server_error)?;

        let mut tag_names = Vec::new();
        for dt in doc_tags {
            let tag = Tag::get_by_id(&mut db, &dt.tag_id)
                .await
                .map_err(topcoat::router::error::internal_server_error)?;
            tag_names.push(tag.name);
        }

        // Use the snippet from FTS5 (or a fallback excerpt)
        let excerpt = if fr.snippet.is_empty() {
            if doc.content.chars().count() > 200 {
                format!("{}...", doc.content.chars().take(200).collect::<String>())
            } else {
                doc.content.clone()
            }
        } else {
            fr.snippet
        };

        results.push(SearchResult {
            id: doc.id.to_string(),
            title: fr.title,
            excerpt,
            tags: tag_names,
        });
    }

    Ok(Json(SearchResponse {
        results,
        query: input.query,
        limit,
        offset,
    }))
}
