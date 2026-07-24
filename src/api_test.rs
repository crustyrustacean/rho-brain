// src/api_test.rs
// Minimal test to discover toasty generated API

#[cfg(test)]
mod tests {
    use crate::models::{Document, Tag};
    use toasty::Db;

    #[test]
    fn discover_api() {
        // This won't compile, but the error messages will tell us what methods exist
        let _ = Document::filter_by_id;
        let _ = Document::get_by_id;
        let _ = Document::all;
        let _ = Document::create;
        let _ = Tag::upsert_by_name;
        let _ = Tag::filter_by_id;
    }
}
