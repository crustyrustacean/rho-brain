// src/main.rs

use toasty::Db;

mod api;
mod app;
mod models;

use topcoat::{
    router::{Router, RouterBuilderDiscoverExt},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Initialize the database. `connect()` expects a driver URL; the bare
    // path form for SQLite is `sqlite:<path>` (`sqlite::memory:` for an
    // in-memory database).
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:rho-brain.db".to_string());

    // `connect()` only validates the connection — it never creates tables.
    // The SQLite driver emits plain `CREATE TABLE` (no `IF NOT EXISTS`), so
    // only push the schema when the database is being created fresh.
    let is_new_db = match db_url.strip_prefix("sqlite:") {
        Some(path) => path == ":memory:" || !std::path::Path::new(path).exists(),
        None => false,
    };

    let db = Db::builder()
        .models(toasty::models!(crate::*))
        .connect(&db_url)
        .await
        .expect("Unable to connect to the database");

    if is_new_db {
        db.push_schema()
            .await
            .expect("Unable to create database schema");
    }

    topcoat::start(
        Router::builder()
            .discover()
            .app_context(db.clone())
            .build(),
    )
    .await?;

    Ok(())
}
