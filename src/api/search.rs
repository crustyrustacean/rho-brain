// src/api/search.rs

use serde::{Deserialize, Serialize};
use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{Json, bad_request, query_params, route},
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

    // Basic LIKE-based search for now
    // TODO: Integrate SQLite FTS5 for proper full-text search
    let pattern = format!("%{}%", input.query);

    let mut query = Document::all()
        .filter(Document::fields().deleted_at().is_none())
        .filter(
            Document::fields()
                .title()
                .like(&pattern)
                .or(Document::fields().content().like(&pattern)),
        );

    if let Some(tags) = input.tags {
        if !tags.is_empty() {
            // Filter to documents that have at least one of the specified tags.
            //
            // Toasty 0.9 mis-lowers a `belongs_to` chain inside `any()`
            // (`tag().name()` panics / mis-matches), so resolve tag names to
            // IDs first and filter the join table on the primitive `tag_id`.
            let matching_tags = Tag::all()
                .filter(Tag::fields().name().in_list(tags))
                .exec(&mut db)
                .await
                .map_err(topcoat::router::internal_server_error)?;

            let tag_ids: Vec<i64> = matching_tags.iter().map(|tag| tag.id).collect();

            if tag_ids.is_empty() {
                // No such tags — no document can match.
                return Ok(Json(SearchResponse {
                    results: Vec::new(),
                    query: input.query,
                    limit,
                    offset,
                }));
            }

            query = query.filter(
                Document::fields()
                    .document_tags()
                    .any(DocumentTag::fields().tag_id().in_list(tag_ids)),
            );
        }
    }

    let docs = query
        .limit(limit)
        .offset(offset)
        .exec(&mut db)
        .await
        .map_err(topcoat::router::internal_server_error)?;

    let mut results = Vec::new();
    for doc in docs {
        // Load tags for each result
        let doc_tags = crate::models::DocumentTag::filter_by_document_id(doc.id)
            .exec(&mut db)
            .await
            .map_err(topcoat::router::internal_server_error)?;

        let mut tag_names = Vec::new();
        for dt in doc_tags {
            let tag = crate::models::Tag::get_by_id(&mut db, &dt.tag_id)
                .await
                .map_err(topcoat::router::internal_server_error)?;
            tag_names.push(tag.name);
        }

        // Generate excerpt from content
        let excerpt = if doc.content.len() > 200 {
            format!("{}...", &doc.content[..200])
        } else {
            doc.content.clone()
        };

        results.push(SearchResult {
            id: doc.id.to_string(),
            title: doc.title,
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
