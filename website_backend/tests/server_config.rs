use std::fs;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use tempfile::TempDir;
use tower::ServiceExt;
use website_backend::{BlogStore, Config, app};

fn config_file(contents: &str) -> (TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("aimer.toml");
    fs::write(&path, contents).unwrap();
    (root, path)
}

fn blog_fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("index.json"),
        r#"[{"id":"post","upload_time":"2026-07-23T00:00:00Z","title":"Post","author":"Aimer Team","tags":["Aimer"]}]"#,
    )
    .unwrap();
    fs::write(root.path().join("post.md"), "# Post\n").unwrap();
    root
}

#[test]
fn loads_server_settings_from_aimer_toml() {
    let (_root, path) = config_file(
        r#"
[server]
ip = "127.0.0.1"
port = 4123
cors = ["https://docs.example.com", "http://localhost:8080"]
"#,
    );

    let config = Config::load(&path).unwrap();

    assert_eq!(
        config
            .server()
            .address()
            .to_string(),
        "127.0.0.1:4123"
    );
    assert_eq!(
        config.server().cors_origins(),
        ["https://docs.example.com", "http://localhost:8080"]
    );
}

#[test]
fn rejects_invalid_server_settings() {
    let (_root, path) = config_file(
        r#"
[server]
ip = "not-an-ip"
port = 70000
cors = ["https://docs.example.com"]
"#,
    );

    let error = Config::load(&path)
        .unwrap_err()
        .to_string();

    assert!(error.contains("parsing"), "unexpected error: {error}");
}

#[test]
fn rejects_invalid_cors_origin() {
    let (_root, path) = config_file(
        r#"
[server]
ip = "127.0.0.1"
port = 4123
cors = ["bad\norigin"]
"#,
    );

    let error = Config::load(&path)
        .unwrap_err()
        .to_string();

    assert!(error.contains("CORS origin"), "unexpected error: {error}");
}

#[tokio::test]
async fn router_uses_configured_cors_origins() {
    let (_root, path) = config_file(
        r#"
[server]
ip = "127.0.0.1"
port = 4123
cors = ["https://allowed.example.com"]
"#,
    );
    let config = Config::load(path).unwrap();
    let blogs = blog_fixture();
    let router = app(
        BlogStore::load(blogs.path()).unwrap(),
        config.server().cors_origins(),
    );

    let allowed = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/blogs")
                .header(header::ORIGIN, "https://allowed.example.com")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let denied = router
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/blogs")
                .header(header::ORIGIN, "https://denied.example.com")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(allowed.status(), StatusCode::OK);
    assert_eq!(
        allowed.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "https://allowed.example.com"
    );
    assert!(
        !denied
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
}
