use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlogSummary {
    pub id: String,
    pub upload_time: String,
    pub title: String,
    pub author: String,
    pub tags: Vec<String>,
}

#[derive(Serialize)]
struct BlogDetail {
    id: String,
    upload_time: String,
    title: String,
    author: String,
    tags: Vec<String>,
    markdown: String,
}

#[derive(Debug)]
struct BlogStoreInner {
    blogs: Vec<BlogSummary>,
    paths: HashMap<String, PathBuf>,
}

#[derive(Clone, Debug)]
pub struct BlogStore(Arc<BlogStoreInner>);

impl BlogStore {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|error| format!("blog content directory is missing: {error}"))?;
        let index_path = root.join("index.json");
        let index = std::fs::read_to_string(&index_path)
            .map_err(|error| format!("blog index is missing: {error}"))?;
        let mut blogs: Vec<BlogSummary> = serde_json::from_str(&index)
            .map_err(|error| format!("blog index is invalid: {error}"))?;
        let mut ids = HashSet::new();
        let mut paths = HashMap::new();

        for blog in &blogs {
            if !is_valid_id(&blog.id) {
                return Err(format!("invalid blog id: {}", blog.id));
            }
            if !ids.insert(blog.id.clone()) {
                return Err(format!("duplicate blog id: {}", blog.id));
            }
            if blog.title.trim().is_empty()
                || blog
                    .upload_time
                    .trim()
                    .is_empty()
                || blog.author.trim().is_empty()
                || blog.tags.is_empty()
                || blog
                    .tags
                    .iter()
                    .any(|tag| tag.trim().is_empty())
            {
                return Err(format!("blog metadata is incomplete: {}", blog.id));
            }
        }

        for blog in &blogs {
            let path = root.join(format!("{}.md", blog.id));
            let canonical = path
                .canonicalize()
                .map_err(|error| format!("markdown is missing for {}: {error}", blog.id))?;
            if !canonical.starts_with(&root) {
                return Err(format!(
                    "markdown path escapes the content directory: {}",
                    blog.id
                ));
            }
            if !canonical.is_file() {
                return Err(format!("markdown is missing for {}", blog.id));
            }
            paths.insert(blog.id.clone(), canonical);
        }

        blogs.sort_by(|left, right| {
            right
                .upload_time
                .cmp(&left.upload_time)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(Self(Arc::new(BlogStoreInner { blogs, paths })))
    }
}

#[derive(Serialize)]
struct BlogList {
    blogs: Vec<BlogSummary>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

pub fn app(store: BlogStore) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_str("https://aimer.cottonsofficial.com").unwrap(),
            HeaderValue::from_str("http://aimer.cottonsofficial.com").unwrap(),
        ]))
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/blogs", get(list_blogs))
        .route("/api/blogs/{id}", get(get_blog))
        .with_state(store)
        .layer(cors)
}

async fn list_blogs(State(store): State<BlogStore>) -> Json<BlogList> {
    Json(BlogList {
        blogs: store.0.blogs.clone(),
    })
}

async fn get_blog(State(store): State<BlogStore>, AxumPath(id): AxumPath<String>) -> Response {
    if !is_valid_id(&id) {
        return error_response(StatusCode::BAD_REQUEST, "invalid blog id");
    }
    let Some(path) = store.0.paths.get(&id) else {
        return error_response(StatusCode::NOT_FOUND, "blog not found");
    };
    let Some(summary) = store
        .0
        .blogs
        .iter()
        .find(|blog| blog.id == id)
    else {
        return error_response(StatusCode::NOT_FOUND, "blog not found");
    };

    match tokio::fs::read_to_string(path).await {
        Ok(markdown) => Json(BlogDetail {
            id: summary.id.clone(),
            upload_time: summary.upload_time.clone(),
            title: summary.title.clone(),
            author: summary.author.clone(),
            tags: summary.tags.clone(),
            markdown,
        })
        .into_response(),
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "unable to read blog"),
    }
}

fn error_response(status: StatusCode, error: &'static str) -> Response {
    (status, Json(ErrorBody { error })).into_response()
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use std::fs;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use serde_json::Value;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use super::*;

    fn fixture() -> TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("index.json"),
            r#"[
                {"id":"older-post","upload_time":"2026-06-01T10:00:00Z","title":"Older post","author":"Aimer Team","tags":["Rust"]},
                {"id":"new-post","upload_time":"2026-07-18T02:22:00Z","title":"New post","author":"Cottons","tags":["Aimer","GUI"]}
            ]"#,
        )
        .unwrap();
        fs::write(
            root.path()
                .join("older-post.md"),
            "# Older\n",
        )
        .unwrap();
        fs::write(
            root.path()
                .join("new-post.md"),
            "# New\n\nHello, Aimer!\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn published_content_includes_rubick_migration_blog() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("content/blogs");
        let store = BlogStore::load(&root).expect("published blog content must be valid");
        let summary = store
            .0
            .blogs
            .iter()
            .find(|blog| blog.id == "migrating-widgets-to-rubick")
            .expect("Rubick migration blog must be indexed");

        assert_eq!(summary.author, "Cottons29");
        assert!(
            summary
                .tags
                .iter()
                .any(|tag| tag == "Rubick")
        );

        let markdown = fs::read_to_string(root.join("migrating-widgets-to-rubick.md")).unwrap();
        assert!(markdown.contains("# Migrating Aimer Widgets to Rubick"));
        assert!(markdown.contains("### Performance Comparison"));
        assert!(markdown.contains("AnyWidget"));
        assert!(markdown.contains("AnyElement"));
    }

    #[tokio::test]
    async fn list_blogs_returns_metadata_newest_first() {
        let root = fixture();
        let response = app(BlogStore::load(root.path()).unwrap())
            .oneshot(
                Request::get("/api/blogs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["blogs"][0]["id"], "new-post");
        assert_eq!(json["blogs"][0]["title"], "New post");
        assert_eq!(json["blogs"][0]["upload_time"], "2026-07-18T02:22:00Z");
        assert_eq!(json["blogs"][0]["author"], "Cottons");
        assert_eq!(
            json["blogs"][0]["tags"],
            serde_json::json!(["Aimer", "GUI"])
        );
        assert_eq!(json["blogs"][1]["id"], "older-post");
    }

    #[tokio::test]
    async fn get_blog_returns_metadata_and_markdown() {
        let root = fixture();
        let response = app(BlogStore::load(root.path()).unwrap())
            .oneshot(
                Request::get("/api/blogs/new-post")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["id"], "new-post");
        assert_eq!(json["upload_time"], "2026-07-18T02:22:00Z");
        assert_eq!(json["author"], "Cottons");
        assert_eq!(json["tags"], serde_json::json!(["Aimer", "GUI"]));
        assert_eq!(json["markdown"], "# New\n\nHello, Aimer!\n");
    }

    #[tokio::test]
    async fn get_blog_distinguishes_invalid_and_unknown_ids() {
        let root = fixture();
        let router = app(BlogStore::load(root.path()).unwrap());

        let invalid = router
            .clone()
            .oneshot(
                Request::get("/api/blogs/invalid_id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

        let unknown = router
            .oneshot(
                Request::get("/api/blogs/unknown-post")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn store_rejects_duplicate_ids_and_missing_markdown() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("index.json"),
            r#"[
                {"id":"same","upload_time":"2026-01-01T00:00:00Z","title":"One","author":"Aimer Team","tags":["Rust"]},
                {"id":"same","upload_time":"2026-01-02T00:00:00Z","title":"Two","author":"Aimer Team","tags":["Rust"]}
            ]"#,
        )
        .unwrap();
        assert!(
            BlogStore::load(root.path())
                .unwrap_err()
                .contains("duplicate")
        );

        fs::write(
            root.path().join("index.json"),
            r#"[{"id":"missing","upload_time":"2026-01-01T00:00:00Z","title":"Missing","author":"Aimer Team","tags":["Rust"]}]"#,
        )
        .unwrap();
        assert!(
            BlogStore::load(root.path())
                .unwrap_err()
                .contains("missing")
        );
    }

    #[test]
    fn store_rejects_missing_author_and_empty_tags() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path()
                .join("missing-author.md"),
            "# Missing author\n",
        )
        .unwrap();
        fs::write(
            root.path().join("index.json"),
            r#"[{"id":"missing-author","upload_time":"2026-01-01T00:00:00Z","title":"Missing author","author":"","tags":[]}]"#,
        )
        .unwrap();

        assert!(
            BlogStore::load(root.path())
                .unwrap_err()
                .contains("incomplete")
        );
    }
}
