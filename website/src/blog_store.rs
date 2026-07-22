use std::collections::HashMap;

use crate::api::BackendApi;
use aimer::console::error;
use aimer::{BuildContext, ProviderHandle};
use serde::Deserialize;

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

#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LoadState<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Error(String),
}

#[derive(Clone, Debug, Default)]
pub struct BlogStore {
    pub list: LoadState<Vec<BlogSummary>>,
    pub details: HashMap<String, LoadState<BlogDetail>>,
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

impl BlogStore {
    pub fn begin_list_load(&mut self) -> bool {
        if !matches!(self.list, LoadState::Idle) {
            return false;
        }
        self.list = LoadState::Loading;
        true
    }

    pub fn begin_detail_load(&mut self, id: &str) -> bool {
        if self.details.contains_key(id) {
            return false;
        }
        self.details
            .insert(id.to_owned(), LoadState::Loading);
        true
    }
}

pub fn request_blog_list(_ctx: &BuildContext, handle: ProviderHandle<BlogStore>) {
    #[cfg(target_arch = "wasm32")]
    let _ = _ctx;

    if !matches!(handle.read().list, LoadState::Idle) {
        return;
    }
    handle.update(|store| {
        store.begin_list_load();
    });

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let result = fetch_text(&BackendApi::blogs())
            .await
            .and_then(|body| decode_blog_list(&body));
        handle.update(move |store| {
            store.list = match result {
                Ok(blogs) => LoadState::Ready(blogs),
                Err(error) => {
                    error!("Error at request_blog_list  {}", error);
                    LoadState::Error(error)
                }
            };
        });
    });

    #[cfg(not(target_arch = "wasm32"))]
    {
        let result = _ctx
            .async_handle
            .block_on(fetch_text(&BackendApi::blogs()))
            .and_then(|body| decode_blog_list(&body));
        handle.update(move |store| {
            store.list = match result {
                Ok(blogs) => LoadState::Ready(blogs),
                Err(error) => {
                    error!("Error at request_blog_list  {}", error);
                    LoadState::Error(error)
                }
            };
        });
    }
}

pub fn request_blog_detail(ctx: &BuildContext, handle: ProviderHandle<BlogStore>, id: String) {
    #[cfg(target_arch = "wasm32")]
    let _ = ctx;

    if handle
        .read()
        .details
        .contains_key(&id)
    {
        return;
    }
    let loading_id = id.clone();
    handle.update(move |store| {
        store.begin_detail_load(&loading_id);
    });

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let result = fetch_text(&detail_url(&id))
            .await
            .and_then(|body| decode_blog_detail(&body));
        handle.update(move |store| {
            store.details.insert(
                id,
                match result {
                    Ok(markdown) => LoadState::Ready(markdown),
                    Err(error) => LoadState::Error(error),
                },
            );
        });
    });

    #[cfg(not(target_arch = "wasm32"))]
    {
        let result = ctx
            .async_handle
            .block_on(fetch_text(&detail_url(&id)))
            .and_then(|body| decode_blog_detail(&body));
        handle.update(move |store| {
            store.details.insert(
                id,
                match result {
                    Ok(markdown) => LoadState::Ready(markdown),
                    Err(error) => LoadState::Error(error),
                },
            );
        });
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
fn api_base_url() -> &'static str {
    "http://localhost:3200"
}

#[cfg(all(not(test), not(target_arch = "wasm32")))]
fn api_base_url() -> &'static str {
    "http://localhost:3200"
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
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .unwrap();
            let mut request = [0; 1024];
            stream
                .read(&mut request)
                .await
                .unwrap();
            stream
                .write_all(response.as_bytes())
                .await
                .unwrap();
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn fetch_text_returns_successful_response_body() {
        let url = serve_once("200 OK", "# Cross-platform blog").await;

        assert_eq!(
            fetch_text(&url)
                .await
                .unwrap(),
            "# Cross-platform blog"
        );
    }

    #[tokio::test]
    async fn fetch_text_rejects_non_success_status() {
        let url = serve_once("503 Service Unavailable", "try later").await;

        assert!(
            fetch_text(&url)
                .await
                .unwrap_err()
                .contains("503")
        );
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
                tags: vec!["Rust".to_owned(), "GUI".to_owned()],
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
    fn loading_transitions_are_started_only_once() {
        let mut store = BlogStore::default();

        assert!(store.begin_list_load());
        assert!(!store.begin_list_load());
        assert_eq!(store.list, LoadState::Loading);

        assert!(store.begin_detail_load("first-post"));
        assert!(!store.begin_detail_load("first-post"));
        assert_eq!(store.details["first-post"], LoadState::Loading);
    }

    #[test]
    fn detail_api_url_uses_the_validated_slug() {
        let expected = BackendApi::blog_with_id("first-post");
        assert_eq!(detail_url("first-post"), expected);
    }
}
