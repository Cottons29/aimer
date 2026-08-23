//! The blog API behind [aimers.dev](https://aimers.dev), running as a
//! Cloudflare Worker compiled to WebAssembly.
//!
//! | Route                 | Description                        |
//! |-----------------------|------------------------------------|
//! | `GET /api/blogs`      | Every blog summary, newest first.   |
//! | `GET /api/blogs/{id}` | One blog, including its markdown.   |
//!
//! The markdown in `content/blogs` is embedded at compile time by `build.rs`,
//! because a Worker has no filesystem. [`BlogStore::embedded`] validates that
//! content and [`app`] turns it into an `axum` [`Router`](axum::Router); the
//! fetch handler wiring them together lives in `entry`, which is only compiled
//! for `wasm32`. The same router is what the tests exercise natively.

mod blog;
mod config;
mod content;

#[cfg(target_arch = "wasm32")]
mod entry;

pub use blog::{BlogStore, BlogSummary, app};
pub use config::{Config, ConfigError, SERVER_CORS};

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode, header};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::{BlogStore, Config, app};

    #[test]
    fn reads_cors_origins_from_variables() {
        let config = Config::from_vars([
            (
                "SERVER_CORS".to_owned(),
                "https://aimers.dev,http://localhost:3000".to_owned(),
            ),
        ])
        .unwrap();

        assert_eq!(
            config.cors_origins(),
            ["https://aimers.dev", "http://localhost:3000"]
        );
    }

    #[test]
    fn trims_origins_and_ignores_blank_entries() {
        let config = Config::from_origins(" https://aimers.dev , , http://localhost:3000 ").unwrap();

        assert_eq!(
            config.cors_origins(),
            ["https://aimers.dev", "http://localhost:3000"]
        );
    }

    #[test]
    fn allows_an_empty_cors_list() {
        assert!(Config::from_origins("").unwrap().cors_origins().is_empty());
    }

    #[test]
    fn reports_a_missing_variable() {
        let error = Config::from_vars([("SERVER_IP".to_owned(), "0.0.0.0".to_owned())])
            .unwrap_err()
            .to_string();

        assert!(error.contains("SERVER_CORS"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_an_invalid_cors_origin() {
        let error = Config::from_origins("bad\u{7f}origin").unwrap_err().to_string();

        assert!(error.contains("CORS origin"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn router_uses_configured_cors_origins() {
        let config = Config::from_origins("https://allowed.example.com").unwrap();
        let router = app(BlogStore::embedded().unwrap(), config.cors_origins());

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

    #[tokio::test]
    async fn every_listed_blog_serves_its_embedded_markdown() {
        let router = app(BlogStore::embedded().unwrap(), &[]);
        let listing = router
            .clone()
            .oneshot(Request::get("/api/blogs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = to_bytes(listing.into_body(), usize::MAX).await.unwrap();
        let listing: Value = serde_json::from_slice(&body).unwrap();
        let blogs = listing["blogs"].as_array().unwrap();

        assert!(!blogs.is_empty(), "the embedded index must not be empty");

        for blog in blogs {
            let id = blog["id"].as_str().unwrap();
            let response = router
                .clone()
                .oneshot(
                    Request::get(format!("/api/blogs/{id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK, "{id} must be served");
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let detail: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(detail["id"], *id);
            assert!(
                !detail["markdown"].as_str().unwrap().is_empty(),
                "{id} must carry markdown"
            );
        }
    }
}
