// src/api/stats.rs

use serde::Serialize;
use toasty::Db;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{content::Json, route},
};

use crate::models::{Document, Tag};

fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub document_count: usize,
    pub unique_tags: usize,
    pub active_documents: usize,
    pub deleted_documents: usize,
}

#[route(GET "/rb/stats")]
pub async fn stats(cx: &Cx) -> Result<Json<StatsResponse>> {
    let mut db = db(cx);

    let all_docs = Document::all()
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    let active = all_docs.iter().filter(|d| d.deleted_at.is_none()).count();
    let deleted = all_docs.len() - active;

    let all_tags = Tag::all()
        .exec(&mut db)
        .await
        .map_err(topcoat::router::error::internal_server_error)?;

    Ok(Json(StatsResponse {
        document_count: all_docs.len(),
        unique_tags: all_tags.len(),
        active_documents: active,
        deleted_documents: deleted,
    }))
}
