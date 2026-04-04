use crate::ImageResult::Success;
use crate::{ImageProvider, ImageResult};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Mutex;
use aimer_utils::error;
use aimer_widget::base::BuildContext;

static NETWORK_CACHE: Lazy<Mutex<HashMap<String, NetworkImageState>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static FILE_CACHE: Lazy<Mutex<HashMap<PathBuf, (u32, u32, u32)>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug)]
enum NetworkImageState {
    Loading,
    Ready(Vec<u8>, u32, u32),
    Loaded(u32, u32, u32),
    Error(String),
}

///
/// Represents the source of an image, which can either be identified by an ID, a file path, or a URL.
///
/// # Variants
///
/// * `Id(u32)` - Specifies the image source using a unique numerical identifier.
///   - `u32`: The unique identifier for the image.
///
/// * `File(String)` - Specifies the image source using a file path.
///   - `String`: The file path to the image as a UTF-8 encoded string.
///
/// * `Network(String)` - Specifies the image source using a URL.
///   - `String`: The URL of the image.
///
/// # Traits Derived
///
/// The `ImageSource` enum derives the following traits:
///
/// * `Clone` - Enables producing a copy of an `ImageSource`.
/// * `Debug` - Facilitates formatting and debugging output.
/// * `PartialEq` - Allows comparison of `ImageSource` instances for equality.
///
/// # Example
/// ```rust ignore
/// use your_crate::ImageSource;
///
/// let img_by_id = ImageSource::Id(123);
/// let img_by_file = ImageSource::File(String::from("path/to/file.png"));
/// let img_by_url = ImageSource::Network(String::from("https://example.com/image.png"));
///
/// match img_by_id {
///     ImageSource::Id(id) => println!("Image ID: {}", id),
///     ImageSource::File(path) => println!("Image Path: {}", path),
///     ImageSource::Network(url) => println!("Image URL: {}", url),
/// }
/// ```
///
#[derive(Clone, Debug, PartialEq)]
pub enum ImageSource {
    Id(u32),
    File(PathBuf),
    Network(String),
    NetworkWithHeaders(String, HashMap<String, String>),
}

impl ImageProvider for ImageSource {
    fn get_image(&self, ctx: &BuildContext) -> ImageResult {
        match self {
            ImageSource::Id(id) => Success(*id),
            ImageSource::File(path) => Self::load_image(ctx, path),
            ImageSource::Network(url) => Self::load_network_image(ctx, url),
            ImageSource::NetworkWithHeaders(url, headers) => Self::load_network_image_with_headers(ctx, url, headers),
        }
    }
}

impl ImageSource {
    pub fn load_image(ctx: &BuildContext, path: &PathBuf) -> ImageResult {
        {
            let cache = FILE_CACHE.lock().unwrap();
            if let Some((id, width, height)) = cache.get(path) {
                ctx.canvas.set_texture_size(*id, *width, *height);
                return Success(*id);
            }
        }

        let Ok(image) = image::open(path) else { return ImageResult::Error("Failed to load image".into()) };
        let bytes = image.to_rgba8();
        let width = image.width();
        let height = image.height();
        let id = ctx.canvas.load_image(&bytes, width, height);

        let mut cache = FILE_CACHE.lock().unwrap();
        cache.insert(path.clone(), (id, width, height));

        Success(id)
    }

    pub fn load_network_image(ctx: &BuildContext, url: &str) -> ImageResult {
        Self::load_network_image_with_headers(ctx, url, &HashMap::new())
    }

    pub fn load_network_image_with_headers(ctx: &BuildContext, url: &str, headers: &HashMap<String, String>) -> ImageResult {
        let mut cache = NETWORK_CACHE.lock().unwrap();
        match cache.get_mut(url) {
            Some(NetworkImageState::Loaded(id, width, height)) => {
                ctx.canvas.set_texture_size(*id, *width, *height);
                Success(*id)
            }
            Some(NetworkImageState::Ready(bytes, width, height)) => {
                let id = ctx.canvas.load_image(bytes, *width, *height);
                let (w, h) = (*width, *height);
                *cache.get_mut(url).unwrap() = NetworkImageState::Loaded(id, w, h);
                Success(id)
            }
            Some(NetworkImageState::Loading) => ImageResult::Loading,
            Some(NetworkImageState::Error(err)) => ImageResult::Error(err.to_string()),
            None => {
                cache.insert(url.to_string(), NetworkImageState::Loading);
                let url = url.to_string();
                let headers = headers.clone();
                let window = ctx.window;

                #[cfg(not(target_arch = "wasm32"))]
                ctx.async_handle.spawn(async move {
                    match Self::fetch_full_image_with_headers(&url, &headers, window).await {
                        Ok(_) => {}
                        Err(err) => {
                            error!("Error to fetch network image : {}", err);
                            // error!("Image URL: {url}");
                            let mut cache = NETWORK_CACHE.lock().unwrap();
                            cache.insert(url, NetworkImageState::Error(err.to_string()));
                            window.request_redraw();
                        }
                    }
                });

                #[cfg(target_arch = "wasm32")]
                {
                    let url_clone = url.clone();
                    let window_clone = window;
                    wasm_bindgen_futures::spawn_local(async move {
                        match Self::fetch_full_image_with_headers(&url_clone, &headers, window_clone).await {
                            Ok(_) => {}
                            Err(err) => {
                                error!("Failed to fetch network image ({}): {}", url_clone, err);
                                let mut cache = NETWORK_CACHE.lock().unwrap();
                                cache.insert(url_clone, NetworkImageState::Error(err.to_string()));
                                window_clone.request_redraw();
                            }
                        }
                    });
                }
                ImageResult::Loading
            }
        }
    }

    #[allow(dead_code)]
    async fn fetch_full_image(url: &str, window: &'static winit::window::Window) -> Result<(), String> {
        Self::fetch_full_image_with_headers(url, &HashMap::new(), window).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn fetch_full_image_with_headers(
        url: &str,
        maps: &HashMap<String, String>,
        window: &'static winit::window::Window,
    ) -> Result<(), String> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::*;
        use web_sys::Headers;

        let headers = Headers::new().unwrap(); // Create empty JS Headers

        for (key, value) in maps {
            headers
                .append(&key, &value)
                .expect("Failed to append header");
        }

        let web_window = web_sys::window().ok_or("No window found")?;
        let request_init = web_sys::RequestInit::new();
        request_init.set_method("GET");
        request_init.set_headers(&JsValue::from(headers));

        let resp_value = wasm_bindgen_futures::JsFuture::from(web_window.fetch_with_str_and_init(url, &request_init))
            .await
            .map_err(|e| format!("{:?}", e))?;
        let resp: web_sys::Response = resp_value.dyn_into().map_err(|e| format!("{:?}", e))?;

        if !resp.ok() {
            return Err(format!("HTTP error: {}", resp.status()));
        }

        let blob_value = wasm_bindgen_futures::JsFuture::from(resp.blob().map_err(|e| format!("{:?}", e))?)
            .await
            .map_err(|e| format!("{:?}", e))?;
        let blob: web_sys::Blob = blob_value.dyn_into().map_err(|e| format!("{:?}", e))?;
        let blob_url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| format!("{:?}", e))?;

        let img = web_sys::HtmlImageElement::new().map_err(|e| format!("{:?}", e))?;
        img.set_src(&blob_url);

        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let onload = Closure::wrap(Box::new(move || {
                let _ = resolve.call0(&JsValue::NULL);
            }) as Box<dyn FnMut()>);
            let on_error = Closure::wrap(Box::new(move |e| {
                let _ = reject.call1(&JsValue::NULL, &e);
            }) as Box<dyn FnMut(JsValue)>);

            img.set_onload(Some(onload.as_ref().unchecked_ref()));
            img.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            onload.forget();
            on_error.forget();
        });

        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|e| format!("{:?}", e))?;

        let width = img.natural_width();
        let height = img.natural_height();

        // On WASM, we don't need RGBA bytes, we use the HtmlImageElement directly.
        // But the current cache expects Ready(Vec<u8>, u32, u32) which then calls load_image.
        // We can pass empty bytes and use load_image to create a NEW HtmlImageElement,
        // OR we can modify the cache/registry.
        // To minimize changes to existing logic, we'll let load_image handle it by passing a special signal or just URL.
        // Actually, let's just stick to the current Flow: we want an ID.
        // We'll manually register it here if we are on WASM.

        // However, media crate doesn't have direct access to the registry in canvas crate
        // UNLESS we use the CanvasRendering trait's load_image.
        // But load_image takes &[u8].

        // Let's implement a way to "ready" it with an ID directly or something.
        // For now, let's keep it simple: We'll re-fetch in load_image or pass the Blob URL.
        // Wait, if we already have the `img` element here, we just need to get it into the registry.
        // But registry is in `canvas` crate.

        // Let's modify `load_image` in `wasm_impl.rs` to optionally take a URL? No, that's not in the trait.
        // How about we pass the blob_url as bytes? No, that's hacky.

        // Re-decoding in Rust (image crate) is slow but works.
        // Let's see if we can get bytes from blob.
        let array_buffer_value = wasm_bindgen_futures::JsFuture::from(blob.array_buffer())
            .await
            .map_err(|e| format!("{:?}", e))?;
        let array_buffer = js_sys::Uint8Array::new(&array_buffer_value);
        let bytes = array_buffer.to_vec();

        let mut cache = NETWORK_CACHE.lock().unwrap();
        cache.insert(url.to_string(), NetworkImageState::Ready(bytes, width, height));
        drop(cache);
        window.request_redraw();

        Ok(())
    }

    #[cfg(target_os = "android")]
    fn create_client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .user_agent("aimer/0.1.0")
            .use_native_tls()
            // .tls_built_in_root_certs(true)
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))
    }

    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    fn create_client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .user_agent("aimer/0.1.0")
            .use_rustls_tls()
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn fetch_full_image_with_headers(
        url: &str,
        headers: &HashMap<String, String>,
        window: &'static winit::window::Window,
    ) -> Result<(), String> {
        let client = Self::create_client()?;

        let mut request_builder = client.get(url);
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }

        let response = request_builder.send().await.map_err(|e| {
            format!("Network Error: {:?},  Source: {:?}", e, e.source())
            // format!("Failed to fetch image: {}", e)
        })?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()).into());
        }

        let all_bytes = response
            .bytes()
            .await
            .map_err(|_| "Failed to download bytes")?
            .to_vec();

        match image::load_from_memory(&all_bytes) {
            Ok(image) => {
                let image = image.into_rgba8();
                let width = image.width();
                let height = image.height();
                let rgba_bytes = image.into_raw();

                let mut cache = NETWORK_CACHE
                    .lock()
                    .map_err(|err| format!("Failed to lock network cache: {}", err))?;
                cache.insert(url.to_string(), NetworkImageState::Ready(rgba_bytes, width, height));
                drop(cache);
                window.request_redraw();
                Ok(())
            }
            Err(e) => {
                // error!("Failed to decode image: {}", e);
                Err("Failed to decode image".into())
            }
        }
    }
}
