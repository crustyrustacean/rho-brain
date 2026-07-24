// src/models.rs

use serde::{Deserialize, Serialize};
use toasty::Model;

/// A knowledge base document.
#[derive(Debug, Model, Serialize, Deserialize)]
pub struct Document {
    #[key]
    #[auto]
    pub id: uuid::Uuid,

    pub title: String,

    pub content: String,

    #[auto]
    pub created_at: jiff::Timestamp,

    #[auto]
    pub updated_at: jiff::Timestamp,

    /// Soft-delete marker. `None` means the document is active.
    pub deleted_at: Option<jiff::Timestamp>,

    #[has_many]
    pub document_tags: toasty::Deferred<Vec<DocumentTag>>,

    #[has_many]
    pub metadata: toasty::Deferred<Vec<Metadata>>,
}

/// A tag that can be applied to documents.
#[derive(Debug, Model, Serialize, Deserialize)]
pub struct Tag {
    #[key]
    #[auto]
    pub id: i64,

    #[unique]
    pub name: String,

    #[has_many]
    pub document_tags: toasty::Deferred<Vec<DocumentTag>>,
}

/// Join model for the many-to-many relationship between documents and tags.
#[derive(Debug, Model, Serialize, Deserialize)]
#[key(document_id, tag_id)]
pub struct DocumentTag {
    #[index]
    pub document_id: uuid::Uuid,

    #[belongs_to(key = document_id, references = id)]
    pub document: toasty::Deferred<Document>,

    #[index]
    pub tag_id: i64,

    #[belongs_to(key = tag_id, references = id)]
    pub tag: toasty::Deferred<Tag>,
}

/// Arbitrary key-value metadata attached to a document.
/// Values are stored in the appropriate typed column.
#[derive(Debug, Model, Serialize, Deserialize)]
pub struct Metadata {
    #[key]
    #[auto]
    pub id: i64,

    #[index]
    pub document_id: uuid::Uuid,

    #[belongs_to(key = document_id, references = id)]
    pub document: toasty::Deferred<Document>,

    pub key: String,

    pub value_text: Option<String>,

    pub value_int: Option<i64>,

    pub value_bool: Option<bool>,
}
