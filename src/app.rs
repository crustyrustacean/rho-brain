// src/app.rs

use topcoat::{
    Result,
    router::{Slot, layout, page},
    view::{component, view},
};

mod api;

// ── Layout ─────────────────────────────────────────

#[layout("/")]
pub async fn root_layout(slot: Slot<'_>) -> Result {
    view! {
        <!DOCTYPE html>
        <html>
            <body>
                <nav>
                    <a href="/">"Home"</a>
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
