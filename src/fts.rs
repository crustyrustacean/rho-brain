// src/fts.rs
///
/// SQLite FTS5 full-text search integration for documents.
///
/// Uses a dedicated FTS5 virtual table (`documents_fts`) that is kept in sync
/// with the `documents` table on create/update/delete operations.  The virtual
/// table stores the document UUID (as text), title, and content — all three
/// columns are indexed so that MATCH queries can match against any of them.

use toasty::Executor;
use toasty::sql;
use toasty::stmt::Value;

/// The name of the FTS5 virtual table.
const FTS_TABLE: &str = "documents_fts";

/// Create the FTS5 virtual table if it does not exist.
///
/// Must be called once after the main schema is pushed.
pub async fn create_fts_table(db: &mut dyn Executor) -> toasty::Result<()> {
    let sql = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS {FTS_TABLE} \
         USING fts5(doc_id, title, content)"
    );
    sql::statement(&sql).exec(db).await?;
    Ok(())
}

/// Index an existing document's title and content into the FTS table.
pub async fn index_document(
    db: &mut dyn Executor,
    doc_id: &uuid::Uuid,
    title: &str,
    content: &str,
) -> toasty::Result<()> {
    let sql = format!(
        "INSERT INTO {FTS_TABLE} (doc_id, title, content) VALUES (?1, ?2, ?3)"
    );
    sql::statement(&sql)
        .bind(doc_id.to_string())
        .bind(title)
        .bind(content)
        .exec(db)
        .await?;
    Ok(())
}

/// Update a document's entry in the FTS index.
pub async fn update_document_index(
    db: &mut dyn Executor,
    doc_id: &uuid::Uuid,
    title: &str,
    content: &str,
) -> toasty::Result<()> {
    let sql = format!(
        "UPDATE {FTS_TABLE} SET title = ?2, content = ?3 WHERE doc_id = ?1"
    );
    sql::statement(&sql)
        .bind(doc_id.to_string())
        .bind(title)
        .bind(content)
        .exec(db)
        .await?;
    Ok(())
}

/// Remove a document from the FTS index.
pub async fn remove_document(
    db: &mut dyn Executor,
    doc_id: &uuid::Uuid,
) -> toasty::Result<()> {
    let sql = format!("DELETE FROM {FTS_TABLE} WHERE doc_id = ?1");
    sql::statement(&sql)
        .bind(doc_id.to_string())
        .exec(db)
        .await?;
    Ok(())
}

/// Backfill the FTS index with all existing non-deleted documents.
///
/// This is called on startup for existing databases that were created before
/// FTS5 support was added.  It does nothing if the FTS table already has rows.
pub async fn backfill_if_empty(db: &mut dyn Executor) -> toasty::Result<()> {
    // Check if the FTS table already has data.
    let count_sql = format!("SELECT COUNT(*) AS cnt FROM {FTS_TABLE}");
    let rows = sql::query(&count_sql).exec(db).await?;
    if let Some(row) = rows.first() {
        if let Some(record) = row.as_record() {
            let fields: Vec<&Value> = record.iter().collect();
            if let Some(cnt) = fields.first().and_then(|v| v.to_i64()) {
                if cnt > 0 {
                    return Ok(());
                }
            }
        }
    }

    // Index all non-deleted documents via raw SQL against the documents table.
    let select_sql = format!(
        "SELECT id, title, content FROM documents WHERE deleted_at IS NULL"
    );
    let docs = sql::query(&select_sql).exec(db).await?;
    for doc in docs {
        if let Some(record) = doc.as_record() {
            let fields: Vec<&Value> = record.iter().collect();
            if fields.len() >= 3 {
                if let (Some(id_val), Some(title), Some(content)) =
                    (fields[0].as_str(), fields[1].as_str(), fields[2].as_str())
                {
                    if let Ok(doc_id) = uuid::Uuid::parse_str(id_val) {
                        index_document(db, &doc_id, title, content).await?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// A single FTS5 search result.
#[derive(Debug)]
pub struct FtsResult {
    pub doc_id: uuid::Uuid,
    pub title: String,
    /// HTML snippet from the content with `<mark>` around matched terms.
    pub snippet: String,
}

/// Perform an FTS5 full-text search and return matching documents.
///
/// Non-alphanumeric characters are stripped from the query and each word is
/// treated as a prefix match (the `*` operator).  Results are ordered by
/// relevance (FTS5's built-in `rank`).
pub async fn search(
    db: &mut dyn Executor,
    query: &str,
    limit: usize,
    offset: usize,
) -> toasty::Result<Vec<FtsResult>> {
    // Strip non-alphanumeric characters (except spaces) so that arbitrary
    // user input doesn't trigger FTS5 syntax errors.
    let clean: String = query
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();

    let safe_query = if clean.is_empty() {
        return Ok(Vec::new());
    } else if clean.contains(' ') {
        let words: Vec<String> = clean
            .split_whitespace()
            .map(|w| format!("{}*", w))
            .collect();
        words.join(" ")
    } else {
        format!("{}*", clean)
    };

    let sql = format!(
        "SELECT doc_id, title, \
         snippet({FTS_TABLE}, 2, '<mark>', '</mark>', '...', 32) AS snippet \
         FROM {FTS_TABLE} \
         WHERE {FTS_TABLE} MATCH ?1 \
         ORDER BY rank \
         LIMIT ?2 OFFSET ?3"
    );

    let rows = sql::query(&sql)
        .bind(&safe_query)
        .bind(limit as i64)
        .bind(offset as i64)
        .exec(db)
        .await?;

    let mut results = Vec::new();
    for row in rows {
        if let Some(record) = row.as_record() {
            let fields: Vec<&Value> = record.iter().collect();
            if fields.len() >= 3 {
                if let (Some(doc_id_str), Some(title), Some(snippet)) =
                    (fields[0].as_str(), fields[1].as_str(), fields[2].as_str())
                {
                    if let Ok(doc_id) = uuid::Uuid::parse_str(doc_id_str) {
                        results.push(FtsResult {
                            doc_id,
                            title: title.to_string(),
                            snippet: snippet.to_string(),
                        });
                    }
                }
            }
        }
    }

    Ok(results)
}