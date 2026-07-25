// src/main.rs

#[tokio::main]
async fn main() -> std::io::Result<()> {
    rho_brain::run().await
}
