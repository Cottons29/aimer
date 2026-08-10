use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

use crate::content;

/// The metadata of one published blog, exactly as `index.json` spells it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlogSummary {
    pub id: String,
    pub upload_time: String,
    pub title: String,
    pub author: String,
    pub tags: Vec<String>,
}

/// A published blog together with its markdown body.
#[derive(Serialize)]
struct BlogDetail<'a> {
    id: &'a str,
    upload_time: &'a str,
    title: &'a str,
    author: &'a str,
    tags: &'a [String],
    markdown: &'a str,
}

/// The listing returned by `GET /api/blogs`.
#[derive(Serialize)]
struct BlogList<'a> {
    blogs: &'a [BlogSummary],
}

/// The body of every error the API answers with.
#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

/// A published blog: where its summary sits in the listing and the markdown
/// that was embedded for it.
#[derive(Debug)]
struct BlogEntry {
    summary: usize,
    markdown: &'static str,
}

#[derive(Debug)]
struct BlogStoreInner {
    blogs: Vec<BlogSummary>,
    entries: HashMap<String, BlogEntry>,
    listing: Bytes,
}

/// The blogs the API serves, resolved once and shared by every request.
///
/// The store owns no file handles: the index and the markdown are embedded at
/// compile time (see `build.rs`), which is what lets the same code run inside a
/// Cloudflare Worker. Cloning is a reference count bump, so it is cheap enough
/// to hand to `axum` as router state.
///
/// # Examples
///
/// ```
/// use website_backend::BlogStore;
///
/// let store = BlogStore::embedded()?;
///
/// assert!(!store.blogs().is_empty());
/// # Ok::<(), String>(())
/// ```
#[derive(Clone, Debug)]
pub struct BlogStore(Arc<BlogStoreInner>);

impl BlogStore {
    /// Builds the store from the content compiled into this crate.
    ///
    /// This is what the Worker uses; every other constructor exists for tests.
    ///
    /// # Errors
    ///
    /// Returns a message describing the first problem found in the embedded
    /// content, exactly like [`BlogStore::from_sources`].
    #[inline]
    pub fn embedded() -> Result<Self, String> {
        Self::from_sources(content::INDEX, content::EMBEDDED_MARKDOWN)
    }

    /// Builds the store from a blog index and a table of markdown documents.
    ///
    /// `index` is the JSON array of [`BlogSummary`] values and `documents` maps
    /// a blog id to its markdown body. A document that no index entry claims is
    /// an unpublished draft: it is ignored instead of rejected, so work in
    /// progress can live next to the published posts.
    ///
    /// The listing is sorted newest first — ties broken by id — and its JSON
    /// encoding is computed here, because the content never changes afterwards.
    ///
    /// # Examples
    ///
    /// ```
    /// use website_backend::BlogStore;
    ///
    /// let index = r#"[{
    ///     "id": "hello",
    ///     "upload_time": "2026-07-18T02:22:00Z",
    ///     "title": "Hello",
    ///     "author": "Aimer Team",
    ///     "tags": ["Aimer"]
    /// }]"#;
    /// let store = BlogStore::from_sources(index, &[("hello", "# Hello\n")])?;
    ///
    /// assert_eq!(store.blogs().len(), 1);
    /// # Ok::<(), String>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a message when `index` is not a valid blog index, when a blog id
    /// is malformed or repeated, when required metadata is blank, or when an
    /// indexed blog has no markdown in `documents`.
    pub fn from_sources(
        index: &str,
        documents: &[(&str, &'static str)],
    ) -> Result<Self, String> {
        let mut blogs: Vec<BlogSummary> =
            serde_json::from_str(index).map_err(|error| format!("blog index is invalid: {error}"))?;

        blogs.sort_by(|left, right| {
            right
                .upload_time
                .cmp(&left.upload_time)
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut entries = HashMap::with_capacity(blogs.len());
        for (position, blog) in blogs.iter().enumerate() {
            if !is_valid_id(&blog.id) {
                return Err(format!("invalid blog id: {}", blog.id));
            }
            if blog.title.trim().is_empty()
                || blog.upload_time.trim().is_empty()
                || blog.author.trim().is_empty()
                || blog.tags.is_empty()
                || blog.tags.iter().any(|tag| tag.trim().is_empty())
            {
                return Err(format!("blog metadata is incomplete: {}", blog.id));
            }
            let Some(&(_, markdown)) = documents.iter().find(|(id, _)| *id == blog.id) else {
                return Err(format!("markdown is missing for {}", blog.id));
            };
            let entry = BlogEntry {
                summary: position,
                markdown,
            };
            if entries.insert(blog.id.clone(), entry).is_some() {
                return Err(format!("duplicate blog id: {}", blog.id));
            }
        }

        let listing = serde_json::to_vec(&BlogList { blogs: &blogs })
            .map_err(|error| format!("blog index cannot be encoded: {error}"))?;

        Ok(Self(Arc::new(BlogStoreInner {
            blogs,
            entries,
            listing: Bytes::from(listing),
        })))
    }

    /// Returns every published blog summary, newest first.
    #[inline]
    pub fn blogs(&self) -> &[BlogSummary] {
        &self.0.blogs
    }
}

/// Builds the blog API, restricting cross-origin requests to `cors_origins`.
///
/// | Route                 | Description                        |
/// |-----------------------|------------------------------------|
/// | `GET /api/blogs`      | Every blog summary, newest first.   |
/// | `GET /api/blogs/{id}` | One blog, including its markdown.   |
///
/// An empty `cors_origins` allows no cross-origin request at all. Every other
/// path answers `404` with the same JSON error body as the routes themselves.
pub fn app(store: BlogStore, cors_origins: &[HeaderValue]) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_origins.iter().cloned()))
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/blogs", get(list_blogs))
        .route("/api/blogs/{id}", get(get_blog))
        .fallback(not_found)
        .with_state(store)
        .layer(cors)
}

/// Answers with the pre-encoded listing, which costs one reference count bump.
async fn list_blogs(State(store): State<BlogStore>) -> Response {
    json_response(store.0.listing.clone())
}

/// Answers with one blog and its markdown, or with the reason it cannot.
async fn get_blog(State(store): State<BlogStore>, Path(id): Path<String>) -> Response {
    if !is_valid_id(&id) {
        return error_response(StatusCode::BAD_REQUEST, "invalid blog id");
    }
    let Some(entry) = store.0.entries.get(&id) else {
        return error_response(StatusCode::NOT_FOUND, "blog not found");
    };
    let summary = &store.0.blogs[entry.summary];
    let detail = BlogDetail {
        id: &summary.id,
        upload_time: &summary.upload_time,
        title: &summary.title,
        author: &summary.author,
        tags: &summary.tags,
        markdown: entry.markdown,
    };

    match serde_json::to_vec(&detail) {
        Ok(body) => json_response(Bytes::from(body)),
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "unable to encode blog"),
    }
}

/// Answers every request that matches no route.
async fn not_found() -> Response {
    error_response(StatusCode::NOT_FOUND, "not found")
}

/// Wraps an already encoded JSON document in a `200 OK` response.
fn json_response(body: Bytes) -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))],
        body,
    )
        .into_response()
}

/// Answers with `status` and a JSON body naming the failure.
fn error_response(status: StatusCode, error: &'static str) -> Response {
    (status, Json(ErrorBody { error })).into_response()
}

/// Reports whether `id` is a legal blog id: a short, lowercase, dashed slug.
///
/// Ids reach the store from the request path, so they are checked before any
/// lookup: rejecting anything but `[a-z0-9-]` keeps traversal attempts and
/// oversized keys away from the content table.
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
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    const INDEX: &str = r#"[
        {"id":"older-post","upload_time":"2026-06-01T10:00:00Z","title":"Older post","author":"Aimer Team","tags":["Rust"]},
        {"id":"new-post","upload_time":"2026-07-18T02:22:00Z","title":"New post","author":"Cottons","tags":["Aimer","GUI"]}
    ]"#;

    /// The third entry is deliberately absent from `INDEX`: markdown without an
    /// index entry is an unpublished draft.
    const MARKDOWN: &[(&str, &str)] = &[
        ("older-post", "# Older\n"),
        ("new-post", "# New\n\nHello, Aimer!\n"),
        ("draft-post", "# Draft\n"),
    ];

    fn store() -> BlogStore {
        BlogStore::from_sources(INDEX, MARKDOWN).unwrap()
    }

    async fn get(uri: &str) -> (StatusCode, Value) {
        let response = app(store(), &[])
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn list_blogs_returns_metadata_newest_first() {
        let (status, json) = get("/api/blogs").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["blogs"][0]["id"], "new-post");
        assert_eq!(json["blogs"][0]["title"], "New post");
        assert_eq!(json["blogs"][0]["upload_time"], "2026-07-18T02:22:00Z");
        assert_eq!(json["blogs"][0]["author"], "Cottons");
        assert_eq!(
            json["blogs"][0]["tags"],
            serde_json::json!(["Aimer", "GUI"])
        );
        assert_eq!(json["blogs"][1]["id"], "older-post");
        assert_eq!(json["blogs"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn get_blog_returns_metadata_and_markdown() {
        let response = app(store(), &[])
            .oneshot(
                Request::get("/api/blogs/new-post")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["id"], "new-post");
        assert_eq!(json["upload_time"], "2026-07-18T02:22:00Z");
        assert_eq!(json["author"], "Cottons");
        assert_eq!(json["tags"], serde_json::json!(["Aimer", "GUI"]));
        assert_eq!(json["markdown"], "# New\n\nHello, Aimer!\n");
    }

    #[tokio::test]
    async fn get_blog_distinguishes_invalid_and_unknown_ids() {
        let (invalid, _) = get("/api/blogs/invalid_id").await;
        let (unknown, _) = get("/api/blogs/unknown-post").await;

        assert_eq!(invalid, StatusCode::BAD_REQUEST);
        assert_eq!(unknown, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unindexed_markdown_stays_unpublished() {
        let (status, _) = get("/api/blogs/draft-post").await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_paths_answer_with_a_json_error() {
        let (status, json) = get("/").await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json["error"], "not found");
    }

    #[test]
    fn store_rejects_duplicate_ids() {
        let index = r#"[
            {"id":"same","upload_time":"2026-01-01T00:00:00Z","title":"One","author":"Aimer Team","tags":["Rust"]},
            {"id":"same","upload_time":"2026-01-02T00:00:00Z","title":"Two","author":"Aimer Team","tags":["Rust"]}
        ]"#;

        assert!(
            BlogStore::from_sources(index, &[("same", "# Same\n")])
                .unwrap_err()
                .contains("duplicate")
        );
    }

    #[test]
    fn store_rejects_missing_markdown() {
        let index = r#"[{"id":"missing","upload_time":"2026-01-01T00:00:00Z","title":"Missing","author":"Aimer Team","tags":["Rust"]}]"#;

        assert!(
            BlogStore::from_sources(index, &[])
                .unwrap_err()
                .contains("missing")
        );
    }

    #[test]
    fn store_rejects_missing_author_and_empty_tags() {
        let index = r#"[{"id":"missing-author","upload_time":"2026-01-01T00:00:00Z","title":"Missing author","author":"","tags":[]}]"#;

        assert!(
            BlogStore::from_sources(index, &[("missing-author", "# Missing author\n")])
                .unwrap_err()
                .contains("incomplete")
        );
    }

    #[test]
    fn store_rejects_an_invalid_index_and_invalid_ids() {
        assert!(
            BlogStore::from_sources("not json", &[])
                .unwrap_err()
                .contains("invalid")
        );

        let index = r#"[{"id":"Bad_Id","upload_time":"2026-01-01T00:00:00Z","title":"Bad","author":"Aimer Team","tags":["Rust"]}]"#;
        assert!(
            BlogStore::from_sources(index, &[("Bad_Id", "# Bad\n")])
                .unwrap_err()
                .contains("invalid blog id")
        );
    }

    #[test]
    fn embedded_content_is_valid_and_published() {
        let store = BlogStore::embedded().expect("published blog content must be valid");
        let summary = store
            .0
            .blogs
            .iter()
            .find(|blog| blog.id == "migrating-widgets-to-rubick")
            .expect("Rubick migration blog must be indexed");

        assert_eq!(summary.author, "Cottons29");
        assert!(summary.tags.iter().any(|tag| tag == "Rubick"));

        let markdown = store.0.entries["migrating-widgets-to-rubick"].markdown;
        assert!(markdown.contains("# Migrating Aimer Widgets to Rubick"));
        assert!(markdown.contains("### Performance Comparison"));
        assert!(markdown.contains("AnyWidget"));
        assert!(markdown.contains("AnyElement"));
    }

    #[test]
    fn the_consuming_conversion_blog_is_published() {
        let store = BlogStore::embedded().expect("published blog content must be valid");
        let summary = store
            .0
            .blogs
            .iter()
            .find(|blog| blog.id == "consuming-widget-to-element")
            .expect("the consuming conversion blog must be indexed");

        assert_eq!(summary.title, "Widgets Now Give Their Fields Away");
        assert_eq!(summary.author, "Cottons29");
        assert!(summary.tags.iter().any(|tag| tag == "Widget"));

        let markdown = store.0.entries["consuming-widget-to-element"].markdown;
        assert!(markdown.contains("# Widgets Now Give Their Fields Away"));
        assert!(markdown.contains("fn to_element(self, ctx: &BuildContext) -> AnyElement"));
        assert!(markdown.contains("ChildBuilder"));
    }

    #[test]
    fn embedded_content_is_sorted_newest_first() {
        let store = BlogStore::embedded().unwrap();
        let times: Vec<&str> = store
            .0
            .blogs
            .iter()
            .map(|blog| blog.upload_time.as_str())
            .collect();

        assert!(times.windows(2).all(|pair| pair[0] >= pair[1]));
        assert_eq!(store.0.blogs.first().unwrap().id, "consuming-widget-to-element");
    }
}
