use std::fs;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use tempfile::TempDir;
use tower::ServiceExt;
use website_backend::{BlogStore, Config, app};

fn config_file(contents: &str) -> (TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".env");
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
fn loads_server_settings_from_env_file() {
    let (_root, path) = config_file(
        "SERVER_IP=127.0.0.1\n\
         SERVER_PORT=4123\n\
         SERVER_CORS=https://docs.example.com,http://localhost:8080\n",
    );

    let config = Config::load(&path).unwrap();

    assert_eq!(config.server().address().to_string(), "127.0.0.1:4123");
    assert_eq!(
        config.server().cors_origins(),
        ["https://docs.example.com", "http://localhost:8080"]
    );
}

#[test]
fn ignores_comments_quotes_and_surrounding_whitespace() {
    let (_root, path) = config_file(
        "# website backend settings\n\
         \n\
         SERVER_IP=\"127.0.0.1\"\n\
         export SERVER_PORT=4123\n\
         SERVER_CORS='https://docs.example.com , http://localhost:8080'\n",
    );

    let config = Config::load(&path).unwrap();

    assert_eq!(config.server().address().to_string(), "127.0.0.1:4123");
    assert_eq!(
        config.server().cors_origins(),
        ["https://docs.example.com", "http://localhost:8080"]
    );
}

#[test]
fn allows_an_empty_cors_list() {
    let (_root, path) = config_file("SERVER_IP=127.0.0.1\nSERVER_PORT=4123\nSERVER_CORS=\n");

    let config = Config::load(&path).unwrap();

    assert!(config.server().cors_origins().is_empty());
}

#[test]
fn reports_a_missing_configuration_file() {
    let root = tempfile::tempdir().unwrap();

    let error = Config::load(root.path().join(".env")).unwrap_err().to_string();

    assert!(error.contains("reading"), "unexpected error: {error}");
}

#[test]
fn reports_a_missing_variable() {
    let (_root, path) = config_file("SERVER_IP=127.0.0.1\nSERVER_CORS=https://docs.example.com\n");

    let error = Config::load(&path).unwrap_err().to_string();

    assert!(error.contains("SERVER_PORT"), "unexpected error: {error}");
}

#[test]
fn rejects_an_invalid_ip_address() {
    let (_root, path) = config_file(
        "SERVER_IP=not-an-ip\nSERVER_PORT=4123\nSERVER_CORS=https://docs.example.com\n",
    );

    let error = Config::load(&path).unwrap_err().to_string();

    assert!(error.contains("SERVER_IP"), "unexpected error: {error}");
}

#[test]
fn rejects_an_out_of_range_port() {
    let (_root, path) = config_file(
        "SERVER_IP=127.0.0.1\nSERVER_PORT=70000\nSERVER_CORS=https://docs.example.com\n",
    );

    let error = Config::load(&path).unwrap_err().to_string();

    assert!(error.contains("SERVER_PORT"), "unexpected error: {error}");
}

#[test]
fn rejects_invalid_cors_origin() {
    let (_root, path) =
        config_file("SERVER_IP=127.0.0.1\nSERVER_PORT=4123\nSERVER_CORS=bad\u{7f}origin\n");

    let error = Config::load(&path).unwrap_err().to_string();

    assert!(error.contains("CORS origin"), "unexpected error: {error}");
}

#[test]
fn reads_server_settings_from_variables() {
    let config = Config::from_vars([
        ("SERVER_IP".to_owned(), "0.0.0.0".to_owned()),
        ("SERVER_PORT".to_owned(), "3200".to_owned()),
        (
            "SERVER_CORS".to_owned(),
            "https://aimer.cottonsofficial.com".to_owned(),
        ),
    ])
    .unwrap();

    assert_eq!(config.server().address().to_string(), "0.0.0.0:3200");
    assert_eq!(
        config.server().cors_origins(),
        ["https://aimer.cottonsofficial.com"]
    );
}

#[test]
fn reports_a_variable_missing_from_the_environment() {
    let error = Config::from_vars([
        ("SERVER_IP".to_owned(), "0.0.0.0".to_owned()),
        ("SERVER_CORS".to_owned(), String::new()),
    ])
    .unwrap_err()
    .to_string();

    assert!(error.contains("SERVER_PORT"), "unexpected error: {error}");
    assert!(error.contains("environment"), "unexpected error: {error}");
}

#[test]
fn resolve_prefers_an_existing_configuration_file() {
    let (_root, path) = config_file("SERVER_IP=127.0.0.1\nSERVER_PORT=4123\nSERVER_CORS=\n");

    let config = Config::resolve(&path).unwrap();

    assert_eq!(config.server().address().to_string(), "127.0.0.1:4123");
}

#[test]
fn resolve_falls_back_to_the_process_environment() {
    let root = tempfile::tempdir().unwrap();
    // SAFETY: this is the only test mutating the environment, and nothing else
    // in this binary reads it.
    unsafe {
        std::env::set_var("SERVER_IP", "0.0.0.0");
        std::env::set_var("SERVER_PORT", "3200");
        std::env::set_var("SERVER_CORS", "https://aimer.cottonsofficial.com");
    }

    let config = Config::resolve(root.path().join(".env")).unwrap();

    assert_eq!(config.server().address().to_string(), "0.0.0.0:3200");
    assert_eq!(
        config.server().cors_origins(),
        ["https://aimer.cottonsofficial.com"]
    );

    // SAFETY: see above.
    unsafe {
        std::env::remove_var("SERVER_IP");
        std::env::remove_var("SERVER_PORT");
        std::env::remove_var("SERVER_CORS");
    }
}

#[tokio::test]
async fn router_uses_configured_cors_origins() {
    let (_root, path) = config_file(
        "SERVER_IP=127.0.0.1\nSERVER_PORT=4123\nSERVER_CORS=https://allowed.example.com\n",
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
