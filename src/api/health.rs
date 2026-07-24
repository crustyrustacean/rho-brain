// src/api/health.rs

use topcoat::{Result, router::route};

#[route(GET "/rb/health_check")]
pub async fn health_check() -> Result<&'static str> {
    Ok("ok")
}
