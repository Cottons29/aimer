use std::collections::HashMap;

use aimer::ProviderHandle;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct BlogSummary {
    pub id: String,
    pub upload_time: String,
    pub title: String,
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
    pub details: HashMap<String, LoadState<String>>,
}

#[cfg(any(test, target_arch = "wasm32"))]
#[derive(Deserialize)]
struct BlogListResponse {
    blogs: Vec<BlogSummary>,
}

#[cfg(any(test, target_arch = "wasm32"))]
pub fn decode_blog_list(json: &str) -> Result<Vec<BlogSummary>, String> {
    let response: BlogListResponse =
        serde_json::from_str(json).map_err(|error| format!("invalid blog response: {error}"))?;
    if response.blogs.iter().any(|blog| {
        !is_valid_id(&blog.id) || blog.title.trim().is_empty() || blog.upload_time.trim().is_empty()
    }) {
        return Err("invalid blog metadata".to_owned());
    }
    Ok(response.blogs)
}

#[cfg(any(test, target_arch = "wasm32"))]
pub fn detail_url(id: &str) -> String {
    debug_assert!(is_valid_id(id));
    format!("{}/api/blogs/{id}", "http://localhost:3200")
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

pub fn request_blog_list(handle: ProviderHandle<BlogStore>) {
    if !matches!(handle.read().list, LoadState::Idle) {
        return;
    }
    handle.update(|store| {
        store.begin_list_load();
    });

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let result = fetch_text(&format!("{}/api/blogs", "http://localhost:3200"))
            .await
            .and_then(|body| decode_blog_list(&body));
        handle.update(move |store| {
            store.list = match result {
                Ok(blogs) => LoadState::Ready(blogs),
                Err(error) => LoadState::Error(error),
            };
        });
    });

    #[cfg(not(target_arch = "wasm32"))]
    handle.update(|store| {
        store.list = LoadState::Error("Blog loading is available in web builds".to_owned());
    });
}

pub fn request_blog_detail(handle: ProviderHandle<BlogStore>, id: String) {
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
        let result = fetch_text(&detail_url(&id)).await;
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
    handle.update(move |store| {
        store
            .details
            .insert(id, LoadState::Error("Blog loading is available in web builds".to_owned()));
    });
}

#[cfg(any(test, target_arch = "wasm32"))]
fn api_base_url() -> &'static str {
    ""
}

#[cfg(any(test, target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
async fn fetch_text(url: &str) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or_else(|| "browser window is unavailable".to_owned())?;
    let response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|error| format!("request failed: {error:?}"))?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| "invalid HTTP response".to_owned())?;
    if !response.ok() {
        return Err(format!("request failed with status {}", response.status()));
    }
    JsFuture::from(
        response
            .text()
            .map_err(|error| format!("response read failed: {error:?}"))?,
    )
    .await
    .map_err(|error| format!("response read failed: {error:?}"))?
    .as_string()
    .ok_or_else(|| "response was not text".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_backend_blog_list_contract() {
        let blogs = decode_blog_list(
            r#"{"blogs":[{"id":"first-post","upload_time":"2026-07-18T02:22:00Z","title":"First post"}]}"#,
        )
        .unwrap();

        assert_eq!(
            blogs,
            vec![BlogSummary {
                id: "first-post".to_owned(),
                upload_time: "2026-07-18T02:22:00Z".to_owned(),
                title: "First post".to_owned(),
            }]
        );
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
        assert_eq!(detail_url("first-post"), "/api/blogs/first-post");
    }
}
