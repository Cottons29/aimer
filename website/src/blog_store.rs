use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use serde::Deserialize;

use crate::api::BackendApi;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct BlogSummary {
    pub id: String,
    pub upload_time: String,
    pub title: String,
    pub author: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct BlogDetail {
    pub id: String,
    pub upload_time: String,
    pub title: String,
    pub author: String,
    pub tags: Vec<String>,
    pub markdown: String,
}

impl Default for BlogDetail {
    fn default() -> Self {
        Self {
            id: "detail_id".to_owned(),
            upload_time: "NA".to_owned(),
            title: "NA".to_owned(),
            author: "NA".to_owned(),
            tags: vec![],
            markdown: "# No Content".to_owned(),
        }
    }
}

#[derive(Deserialize)]
struct BlogListResponse {
    blogs: Vec<BlogSummary>,
}

static BLOG_LIST_CACHE: LazyLock<Mutex<Option<Vec<BlogSummary>>>> =
    LazyLock::new(|| Mutex::new(None));

static BLOG_DETAIL_CACHE: LazyLock<Mutex<HashMap<String, BlogDetail>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Locks a cache without propagating poisoning.
///
/// A panic while a cache is locked cannot leave the map in an inconsistent
/// state, so the poisoned guard is recovered instead of aborting the render.
#[inline]
fn lock<T>(cache: &Mutex<T>) -> MutexGuard<'_, T> {
    cache.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn decode_blog_list(json: &str) -> Result<Vec<BlogSummary>, String> {
    let response: BlogListResponse =
        serde_json::from_str(json).map_err(|error| format!("invalid blog response: {error}"))?;
    Ok(response.blogs)
}

pub fn decode_blog_detail(json: &str) -> Result<BlogDetail, String> {
    let detail: BlogDetail =
        serde_json::from_str(json).map_err(|error| format!("invalid blog response: {error}"))?;
    Ok(detail)
}

pub fn detail_url(id: &str) -> String {
    debug_assert!(is_valid_id(id));
    BackendApi::blog_with_id(id)
}

/// Returns the blog archive fetched earlier in this session, when present.
pub fn cached_blog_list() -> Option<Vec<BlogSummary>> {
    lock(&BLOG_LIST_CACHE).clone()
}

/// Remembers the blog archive for the remainder of the session.
pub fn cache_blog_list(blogs: &[BlogSummary]) {
    *lock(&BLOG_LIST_CACHE) = Some(blogs.to_vec());
}

/// Returns the post fetched earlier in this session, when present.
pub fn cached_blog_detail(id: &str) -> Option<BlogDetail> {
    lock(&BLOG_DETAIL_CACHE).get(id).cloned()
}

/// Remembers a single post for the remainder of the session.
pub fn cache_blog_detail(detail: &BlogDetail) {
    lock(&BLOG_DETAIL_CACHE).insert(detail.id.clone(), detail.clone());
}

/// Loads the blog archive without blocking the render thread.
///
/// The first call performs one request and remembers the decoded archive, so
/// later navigations back to the blog page resolve from the session cache
/// instead of hitting the network again.
///
/// Intended to be driven by
/// [`AsyncBuilder`](aimer::AsyncBuilder), which owns the future and cancels it
/// when the page is dropped.
pub async fn fetch_blog_list() -> Result<Vec<BlogSummary>, String> {
    if let Some(cached) = cached_blog_list() {
        return Ok(cached);
    }
    let body = fetch_text(&BackendApi::blogs()).await?;
    let blogs = decode_blog_list(&body)?;
    cache_blog_list(&blogs);
    Ok(blogs)
}

/// Loads a single post without blocking the render thread.
///
/// Behaves like [`fetch_blog_list`], keyed by the post slug: a post that was
/// already read in this session is returned from the cache, so reopening it
/// costs no request.
pub async fn fetch_blog_detail(id: String) -> Result<BlogDetail, String> {
    if let Some(cached) = cached_blog_detail(&id) {
        return Ok(cached);
    }
    let body = fetch_text(&detail_url(&id)).await?;
    let detail = decode_blog_detail(&body)?;
    cache_blog_detail(&detail);
    Ok(detail)
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

async fn fetch_text(url: &str) -> Result<String, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("request failed with status {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|error| format!("response read failed: {error}"))
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    async fn serve_once(status: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            stream.read(&mut request).await.unwrap();
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        format!("http://{address}")
    }

    fn detail(id: &str) -> BlogDetail {
        BlogDetail {
            id: id.to_owned(),
            upload_time: "2026-07-18T02:22:00Z".to_owned(),
            title: "First post".to_owned(),
            author: "Aimer Team".to_owned(),
            tags: vec!["Rust".to_owned()],
            markdown: "# First post".to_owned(),
        }
    }

    #[tokio::test]
    async fn fetch_text_returns_successful_response_body() {
        let url = serve_once("200 OK", "# Cross-platform blog").await;

        assert_eq!(fetch_text(&url).await.unwrap(), "# Cross-platform blog");
    }

    #[tokio::test]
    async fn fetch_text_rejects_non_success_status() {
        let url = serve_once("503 Service Unavailable", "try later").await;

        assert!(fetch_text(&url).await.unwrap_err().contains("503"));
    }

    #[test]
    fn decodes_the_backend_blog_list_contract() {
        let blogs = decode_blog_list(
            r#"{"blogs":[{"id":"first-post","upload_time":"2026-07-18T02:22:00Z","title":"First post","author":"Aimer Team","tags":["Rust","GUI"]}]}"#,
        )
        .unwrap();

        assert_eq!(
            blogs,
            vec![BlogSummary {
                id: "first-post".to_owned(),
                upload_time: "2026-07-18T02:22:00Z".to_owned(),
                title: "First post".to_owned(),
                author: "Aimer Team".to_owned(),
                tags: vec!["Rust".to_owned(), "GUI".to_owned()]
            }]
        );
    }

    #[test]
    fn decodes_the_backend_blog_detail_contract() {
        let detail = decode_blog_detail(
            r##"{"id":"first-post","upload_time":"2026-07-18T02:22:00Z","title":"First post","author":"Aimer Team","tags":["Rust","GUI"],"markdown":"# First post"}"##,
        )
        .unwrap();

        assert_eq!(detail.author, "Aimer Team");
        assert_eq!(detail.tags, vec!["Rust", "GUI"]);
        assert_eq!(detail.markdown, "# First post");
    }

    #[test]
    fn rejects_incomplete_blog_metadata() {
        assert!(decode_blog_list(r#"{"blogs":[{"id":"missing-fields"}]}"#).is_err());
    }

    #[test]
    fn detail_api_url_uses_the_validated_slug() {
        let expected = BackendApi::blog_with_id("first-post");
        assert_eq!(detail_url("first-post"), expected);
    }

    #[test]
    fn detail_cache_round_trips_per_slug() {
        let cached = detail("cache-round-trip");
        assert_eq!(cached_blog_detail("cache-round-trip"), None);

        cache_blog_detail(&cached);

        assert_eq!(cached_blog_detail("cache-round-trip"), Some(cached));
        assert_eq!(cached_blog_detail("another-slug"), None);
    }

    #[tokio::test]
    async fn cached_posts_are_returned_without_a_request() {
        let cached = detail("cached-without-request");
        cache_blog_detail(&cached);

        assert_eq!(
            fetch_blog_detail("cached-without-request".to_owned())
                .await
                .unwrap(),
            cached
        );
    }

    #[tokio::test]
    async fn cached_archive_is_returned_without_a_request() {
        let blogs = vec![BlogSummary {
            id: "cached-archive".to_owned(),
            upload_time: "2026-07-18T02:22:00Z".to_owned(),
            title: "Cached archive".to_owned(),
            author: "Aimer Team".to_owned(),
            tags: vec!["Rust".to_owned()],
        }];
        cache_blog_list(&blogs);

        assert_eq!(fetch_blog_list().await.unwrap(), blogs);
    }
}
