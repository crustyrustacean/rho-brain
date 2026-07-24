// src/app/about.rs

use topcoat::{router::page, view::view, Result};

#[page("/about")]
pub async fn about() -> Result {
    view! {
        <h1>"About Topcoat"</h1>
        <p>"A modular, batteries-included Rust framework for building fullstack apps."</p>
    }
}
