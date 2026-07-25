// tests/api/helpers.rs
//
// In-process integration testing: each test gets a fresh in-memory SQLite
// database (toasty clamps its pool to one connection for `:memory:`, so the
// schema push and queries share a single private database) and a Router that
// requests are dispatched through directly — no TCP listener, no HTTP client.

use toasty::Db;
use topcoat::router::{Body, HeaderMap, Method, Router, StatusCode, to_bytes};

#[allow(dead_code)]
pub struct TestApp {
    pub router: Router,
    pub db: Db,
}

pub async fn spawn_app() -> TestApp {
    let db = rho_brain::connect("sqlite::memory:")
        .await
        .expect("Failed to initialize in-memory database.");
    let router = rho_brain::router(db.clone());

    TestApp { router, db }
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: bytes::Bytes,
}

#[allow(dead_code)]
impl TestResponse {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("Response body is not valid JSON.")
    }

    pub fn text(&self) -> String {
        String::from_utf8(self.body.to_vec()).expect("Response body is not valid UTF-8.")
    }
}

#[allow(dead_code)]
impl TestApp {
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> TestResponse {
        let mut builder = http::Request::builder().method(method).uri(path);
        if let Some(content_type) = content_type {
            builder = builder.header("content-type", content_type);
        }

        let request = builder.body(Body::from(body)).unwrap();
        let response = self.router.handle(request).await;
        let (parts, body) = response.into_parts();
        let body = to_bytes(body, usize::MAX).await.unwrap();

        TestResponse {
            status: parts.status,
            headers: parts.headers,
            body,
        }
    }

    pub async fn get(&self, path: &str) -> TestResponse {
        self.request(Method::GET, path, None, Vec::new()).await
    }

    pub async fn post_json(&self, path: &str, body: serde_json::Value) -> TestResponse {
        self.request(
            Method::POST,
            path,
            Some("application/json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .await
    }

    pub async fn put_json(&self, path: &str, body: serde_json::Value) -> TestResponse {
        self.request(
            Method::PUT,
            path,
            Some("application/json"),
            serde_json::to_vec(&body).unwrap(),
        )
        .await
    }

    pub async fn delete(&self, path: &str) -> TestResponse {
        self.request(Method::DELETE, path, None, Vec::new()).await
    }

    pub async fn post_form(&self, path: &str, form: &str) -> TestResponse {
        self.request(
            Method::POST,
            path,
            Some("application/x-www-form-urlencoded"),
            form.as_bytes().to_vec(),
        )
        .await
    }
}
