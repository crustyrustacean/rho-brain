// src/app/assets.rs
//
// Self-hosted static assets, served by plain routes.
//
// These are deliberately outside topcoat's `asset!` bundling pipeline: the
// binary embeds the files with `include_bytes!` and serves them from stable
// URLs, so there is no bundle step to run before `cargo test` or `cargo run`
// and no way for the binary and its assets to drift apart. If the app ever
// grows many assets, switch to `asset!` + `AssetBundle` and revisit.

use topcoat::{
    Result,
    router::{content::Js, route},
};

/// The vendored Datastar runtime, the client-side half of the server-driven
/// UI.
///
/// Source: https://cdn.jsdelivr.net/gh/starfederation/datastar@v1.0.2/bundles/datastar.js
/// (sha256 2837d87acf6ee0ba8e4e63765926c25a98d63883b02f88be194a86b81d3fd24a,
/// banner `// Datastar v1.0.2`).
///
/// `Js` sets the JavaScript media type, which a `<script type="module">`
/// requires: browsers refuse to execute a module whose response lacks it.
#[route(GET "/js/datastar.js")]
pub async fn datastar_js() -> Result<Js<&'static [u8]>> {
    Ok(Js(include_bytes!("assets/datastar-v1.0.2.js")))
}
