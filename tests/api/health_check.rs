// tests/api/health_check.rs

use crate::helpers::spawn_app;

#[tokio::test]
async fn health_check_returns_200_ok() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get("/rb/health_check").await;

    // Assert
    assert!(response.status.is_success());
    assert_eq!(response.text(), "ok");
}
