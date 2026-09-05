// src/app.rs

use topcoat::{
    Result,
    router::{Slot, layout},
    view::{Unescaped, View, view},
};

mod assets;
mod document;
mod editor;
mod home;
mod markdown;

const CSS: &str = include_str!("app/style.css");

// ── Layout ─────────────────────────────────────────

#[layout("/")]
pub async fn root_layout(slot: Slot<'_>) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"rho-brain"</title>
                <style>(Unescaped::new_unchecked(CSS))</style>
                <script type="module" src="/js/datastar.js"></script>
            </head>
            <body>
                <header>
                    <div class="container header-inner">
                        <h1><a href="/">"rho-brain"</a></h1>
                        <nav>
                            <a href="/">"Home"</a>
                            <a href="/documents/new">"+ New Document"</a>
                        </nav>
                    </div>
                </header>
                <main class="container">
                    (slot)
                </main>
            </body>
        </html>
    })
}
