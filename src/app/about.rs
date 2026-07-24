// src/app/about.rs

use topcoat::{Result, router::page, view::view};

#[page("/about")]
pub async fn about() -> Result {
    view! {
        <h1>"About Topcoat"</h1>
        <p>"A modular, batteries-included Rust framework for building fullstack apps."</p>
    }
}
