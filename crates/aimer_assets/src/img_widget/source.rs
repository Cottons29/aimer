use crate::ImageResult::Success;
use crate::{ImageProvider, ImageResult};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Mutex;
use aimer_utils::{debug, error};
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

        // Fetch the raw bytes and decode to RGBA for the GPU pipeline.
        let array_buffer_value = wasm_bindgen_futures::JsFuture::from(
            resp.array_buffer().map_err(|e| format!("{:?}", e))?
        )
            .await
            .map_err(|e| format!("{:?}", e))?;
        let array_buffer = js_sys::Uint8Array::new(&array_buffer_value);
        let compressed_bytes = array_buffer.to_vec();

        let decoded = image::load_from_memory(&compressed_bytes)
            .map_err(|e| format!("Failed to decode image: {}", e))?;
        let rgba = decoded.to_rgba8();
        let width = decoded.width();
        let height = decoded.height();
        let bytes = rgba.into_raw();

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
