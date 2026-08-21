// src/app/markdown.rs
//
// Shared markdown rendering for the web UI. The same function backs both the
// document view page and (later) the editor's live preview, so preview output
// always matches the final render.

/// Render markdown to HTML, mirroring pi-brain's document view.
///
/// Raw HTML in the source is not passed through: pulldown-cmark emits it as
/// `Html`/`InlineHtml` events which the writer would embed verbatim, and the
/// view page inserts the result unescaped. Those events are converted to
/// text, so raw markup shows up escaped (as literal text) instead of live.
pub(crate) fn render_markdown(content: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, html};
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(content, options).map(|event| match event {
        Event::Html(html) | Event::InlineHtml(html) => Event::Text(html),
        event => event,
    });
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

#[cfg(test)]
mod tests {
    use super::render_markdown;

    #[test]
    fn renders_basic_markdown() {
        assert!(render_markdown("# Title").contains("<h1>Title</h1>"));
        assert!(render_markdown("**bold**").contains("<strong>bold</strong>"));
        assert!(render_markdown("*em*").contains("<em>em</em>"));
    }

    #[test]
    fn tables_extension_is_enabled() {
        let md = "| a | b |\n| --- | --- |\n| 1 | 2 |";
        let html = render_markdown(md);
        assert!(html.contains("<table>"), "GFM tables should render: {html}");
        assert!(html.contains("<td>1</td>"), "cell contents should render: {html}");
    }

    #[test]
    fn strikethrough_extension_is_enabled() {
        let html = render_markdown("~~gone~~");
        assert!(
            html.contains("<del>gone</del>"),
            "GFM strikethrough should render: {html}"
        );
    }

    #[test]
    fn raw_html_is_not_passed_through() {
        // The view page embeds the output unescaped, so raw HTML in document
        // content must never survive rendering. It is escaped to visible text
        // instead (found by this test suite: pulldown-cmark passes raw HTML
        // through by default).
        let html = render_markdown("<script>alert(1)</script>");
        assert!(!html.contains("<script>"), "raw HTML leaked: {html}");
        assert!(html.contains("&lt;script&gt;"), "should be visible as text: {html}");
    }
}
