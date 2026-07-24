// src/app.rs

use topcoat::{
    asset::{asset, Asset},
    router::{layout, page, Slot},
    view::{component, view},
    Result,
};

const FAVICON: Asset = asset!("assets/favicon.png");

mod about;
mod api;

// ── Layout ─────────────────────────────────────────

#[layout("/")]
pub async fn root_layout(slot: Slot<'_>) -> Result {
    view! {
        <!DOCTYPE html>
        <html>
            <head>
                <link rel="icon" type="image/png" href=(FAVICON)>
            </head>
            <body>
                <nav>
                    <a href="/">"Home"</a>
                    <a href="/about">"About"</a>
                </nav>
                <main>
                    (slot.await?)
                </main>
            </body>
        </html>
    }
}

// ── Component ─────────────────────────────────────

#[component]
pub async fn hello(name: &str) -> Result {
    view! {
        <h1>
            "Hello, "
            (name)
            "!"
        </h1>
    }
}

// ── Pages ─────────────────────────────────────────

#[page("/")]
pub async fn home() -> Result {
    view! { hello(name: "World") }
}
