// src/lib.rs

use toasty::Db;
use topcoat::router::{Router, RouterBuilderDiscoverExt};

pub mod api;
pub mod app;
pub mod models;

/// Build the application router with all discovered routes, layouts, and
/// pages, sharing the given database handle via the app context.
pub fn router(db: Db) -> Router {
    Router::builder()
        .discover()
        .app_context(db)
        .build()
}

/// Connect to the database at `url`, creating the schema when the database
/// is fresh.
///
/// `connect()` only validates the connection — it never creates tables. The
/// SQLite driver emits plain `CREATE TABLE` (no `IF NOT EXISTS`), so the
/// schema is only pushed for a new file or an in-memory database.
pub async fn connect(url: &str) -> toasty::Result<Db> {
    let is_new_db = match url.strip_prefix("sqlite:") {
        Some(path) => path == ":memory:" || !std::path::Path::new(path).exists(),
        None => false,
    };

    let db = Db::builder()
        .models(toasty::models!(crate::*))
        .connect(url)
        .await?;

    if is_new_db {
        db.push_schema().await?;
    }

    Ok(db)
}

/// Run the HTTP server. `connect()` expects a driver URL; the bare path form
/// for SQLite is `sqlite:<path>` (`sqlite::memory:` for an in-memory
/// database).
pub async fn run() -> std::io::Result<()> {
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:rho-brain.db".to_string());

    let db = connect(&db_url)
        .await
        .expect("Unable to initialize the database");

    topcoat::start(router(db)).await?;

    Ok(())
}
