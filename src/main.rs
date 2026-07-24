// src/main.rs

mod api;
mod api_test;
mod app;
mod db;
mod models;

use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    router::{Router, RouterBuilderDiscoverExt},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Initialize the database
    let db_path = std::env::var("DATABASE_URL").unwrap_or_else(|_| "rho-brain.db".to_string());
    db::init(&db_path)
        .await
        .expect("failed to initialize database");

    topcoat::start(
        Router::builder()
            .discover()
            .assets(AssetBundle::load()?)
            .app_context(db::db().clone())
            .build(),
    )
    .await?;

    Ok(())
}
