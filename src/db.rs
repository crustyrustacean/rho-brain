// src/db.rs

use std::sync::OnceLock;
use toasty::Db;

use crate::models::{Document, DocumentTag, Metadata, Tag};

static DB: OnceLock<Db> = OnceLock::new();

/// Initialize the database connection and register all models.
pub async fn init(path: &str) -> toasty::Result<()> {
    let db = Db::builder()
        .register_model::<Document>()
        .register_model::<Tag>()
        .register_model::<DocumentTag>()
        .register_model::<Metadata>()
        .build(path)
        .await?;

    // Run migrations / create tables
    db.sync_schema().await?;

    DB.set(db).map_err(|_| {
        toasty::Error::from(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "database already initialized",
        ))
    })?;

    Ok(())
}

/// Get a reference to the global database handle.
pub fn db() -> &'static Db {
    DB.get().expect("database not initialized")
}
