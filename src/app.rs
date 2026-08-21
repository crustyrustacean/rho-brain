// src/app.rs

use topcoat::{
    Result,
    router::layout,
    view::{Unescaped, view},
};

mod document;
mod editor;
mod home;
mod markdown;

const CSS: &str = include_str!("app/style.css");

// ── Layout ─────────────────────────────────────────

#[layout("/")]
pub async fn root_layout(slot: Result) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>"rho-brain"</title>
                <style>(Unescaped::new_unchecked(CSS))</style>
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
                    (slot?)
                </main>
            </body>
        </html>
    }
}
