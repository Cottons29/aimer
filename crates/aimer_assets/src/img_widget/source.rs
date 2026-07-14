use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::error::Error;
use std::path::PathBuf;
use std::sync::Mutex;

use aimer_utils::error;
use aimer_widget::base::{BuildContext, WindowHandle};
use once_cell::sync::Lazy;

use crate::ImageResult::Success;
use crate::{ImageProvider, ImageResult};
type FileCacheMap = Mutex<HashMap<PathBuf, DiskImageState>>;
static NETWORK_CACHE: Lazy<Mutex<HashMap<String, NetworkImageState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub(crate) static FILE_CACHE: Lazy<FileCacheMap> = Lazy::new(|| Mutex::new(HashMap::new()));
/// Cache of decoded bundled assets, keyed by their registered lookup key.
#[cfg(not(target_arch = "wasm32"))]
static ASSET_CACHE: Lazy<Mutex<HashMap<String, DiskImageState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug)]
enum NetworkImageState {
    Loading,
    Ready(Vec<u8>, u32, u32),
    Loaded(u32, u32, u32),
    Error(String),
}

/// State machine for an image loaded from disk (a file path or a bundled
/// asset). Decoding is the expensive step (`image::open` / `to_rgba8` can take
/// hundreds of milliseconds for a large screenshot), so it is performed off the
/// render thread. The decoded `Ready` bytes are then uploaded to the GPU on the
/// render thread (cheap) and transitioned to `Loaded`. This mirrors how network
/// images are handled and prevents the ~0.2s frame hitch that occurred when an
/// off-screen image was decoded synchronously the first time it scrolled into
/// view.
#[derive(Clone, Debug)]
// On wasm the asynchronous decode path is compiled out, so some variants are
// only matched (never constructed); silence the resulting dead-code warning.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) enum DiskImageState {
    Loading,
    Ready(Vec<u8>, u32, u32),
    Loaded(u32, u32, u32),
    Error(String),
}

///
/// Represents the source of an image, which can either be identified by an ID,
/// a file path, or a URL.
///
/// # Variants
///
/// * `Id(u32)` - Specifies the image source using a unique numerical
///   identifier.
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
#[derive(Clone, Debug, PartialEq)]
pub enum ImageSource {
    Id(u32),
    /// A bundled asset registered under `[assets]` in `aimer.toml`. The string
    /// is the path declared there (relative to the project root) and is used
    /// verbatim as the per-platform lookup key.
    Asset(String),
    File(PathBuf),
    Network(String),
    NetworkWithHeaders(String, HashMap<String, String>),
}

impl ImageProvider for ImageSource {
    fn get_image(&self, ctx: &BuildContext) -> ImageResult {
        match self {
            ImageSource::Id(id) => Success(*id),
            ImageSource::Asset(key) => Self::load_asset_image(ctx, key),
            ImageSource::File(path) => Self::load_image(ctx, path),
            ImageSource::Network(url) => Self::load_network_image(ctx, url),
            ImageSource::NetworkWithHeaders(url, headers) => {
                Self::load_network_image_with_headers(ctx, url, headers)
            }
        }
    }
}

impl ImageSource {
    pub fn load_image(ctx: &BuildContext, path: &PathBuf) -> ImageResult {
        {
            let mut cache = FILE_CACHE
                .lock()
                .unwrap();
            match cache.get_mut(path) {
                Some(DiskImageState::Loaded(id, width, height)) => {
                    ctx.canvas
                        .set_texture_size(*id, *width, *height);
                    return Success(*id);
                }
                Some(DiskImageState::Ready(bytes, width, height)) => {
                    // Decoded on a background thread; upload to the GPU here (on
                    // the render thread, where the canvas/GPU lives) and cache id.
                    let id = ctx
                        .canvas
                        .load_image(bytes, *width, *height);
                    let (w, h) = (*width, *height);
                    *cache
                        .get_mut(path)
                        .unwrap() = DiskImageState::Loaded(id, w, h);
                    return Success(id);
                }
                Some(DiskImageState::Loading) => return ImageResult::Loading,
                Some(DiskImageState::Error(err)) => return ImageResult::Error(err.clone()),
                None => {}
            }
        }

        // Cache miss: decode off the render thread so scrolling a large image
        // into view does not block the frame for hundreds of milliseconds.
        #[cfg(not(target_arch = "wasm32"))]
        {
            FILE_CACHE
                .lock()
                .unwrap()
                .insert(path.clone(), DiskImageState::Loading);
            let path_buf = path.clone();
            let window = ctx
                .window
                .clone();
            ctx.async_handle
                .spawn_blocking(move || {
                    let state = match image::open(&path_buf) {
                        Ok(image) => {
                            let rgba = image.to_rgba8();
                            let (width, height) = (rgba.width(), rgba.height());
                            DiskImageState::Ready(rgba.into_raw(), width, height)
                        }
                        Err(_) => DiskImageState::Error("Failed to load image".into()),
                    };
                    FILE_CACHE
                        .lock()
                        .unwrap()
                        .insert(path_buf, state);
                    window.request_redraw();
                });
            ImageResult::Loading
        }

        // wasm: fetch and decode asynchronously via the browser's native
        // decoder (much faster than the Rust `image` crate compiled to wasm).
        #[cfg(target_arch = "wasm32")]
        {
            FILE_CACHE
                .lock()
                .unwrap()
                .insert(path.clone(), DiskImageState::Loading);
            let url = path
                .to_string_lossy()
                .to_string();
            let path_buf = path.clone();
            let window = ctx
                .window
                .clone();
            wasm_bindgen_futures::spawn_local(async move {
                let state = match Self::fetch_bytes(&url).await {
                    Ok(bytes) => match Self::decode_image_browser(&bytes).await {
                        Ok((rgba, w, h)) => DiskImageState::Ready(rgba, w, h),
                        Err(e) => DiskImageState::Error(e),
                    },
                    Err(e) => DiskImageState::Error(e),
                };
                FILE_CACHE
                    .lock()
                    .unwrap()
                    .insert(path_buf, state);
                window.request_redraw();
            });
            ImageResult::Loading
        }
    }

    pub fn load_network_image(ctx: &BuildContext, url: &str) -> ImageResult {
        Self::load_network_image_with_headers(ctx, url, &HashMap::new())
    }

    /// Load a bundled asset by its registered key.
    ///
    /// On native targets the bytes are read synchronously from the platform's
    /// asset store (Android `AssetManager`, or the app bundle / project dir on
    /// desktop & iOS/macOS), decoded, uploaded to the GPU and cached by key.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_asset_image(ctx: &BuildContext, key: &str) -> ImageResult {
        {
            let mut cache = ASSET_CACHE
                .lock()
                .unwrap();
            match cache.get_mut(key) {
                Some(DiskImageState::Loaded(id, width, height)) => {
                    ctx.canvas
                        .set_texture_size(*id, *width, *height);
                    return Success(*id);
                }
                Some(DiskImageState::Ready(bytes, width, height)) => {
                    // Decoded on a background thread; upload on the render thread.
                    let id = ctx
                        .canvas
                        .load_image(bytes, *width, *height);
                    let (w, h) = (*width, *height);
                    *cache
                        .get_mut(key)
                        .unwrap() = DiskImageState::Loaded(id, w, h);
                    return Success(id);
                }
                Some(DiskImageState::Loading) => return ImageResult::Loading,
                Some(DiskImageState::Error(err)) => return ImageResult::Error(err.clone()),
                None => {}
            }
        }

        // Cache miss: read + decode the asset off the render thread so scrolling
        // it into view does not block the frame.
        ASSET_CACHE
            .lock()
            .unwrap()
            .insert(key.to_string(), DiskImageState::Loading);
        let key_owned = key.to_string();
        let window = ctx
            .window
            .clone();
        ctx.async_handle
            .spawn_blocking(move || {
                let state = match Self::load_asset_bytes(&key_owned) {
                    Ok(bytes) => match image::load_from_memory(&bytes) {
                        Ok(image) => {
                            let rgba = image.to_rgba8();
                            let (width, height) = (rgba.width(), rgba.height());
                            DiskImageState::Ready(rgba.into_raw(), width, height)
                        }
                        Err(_) => DiskImageState::Error(format!(
                            "Failed to decode asset image '{key_owned}'"
                        )),
                    },
                    Err(err) => DiskImageState::Error(err),
                };
                ASSET_CACHE
                    .lock()
                    .unwrap()
                    .insert(key_owned, state);
                window.request_redraw();
            });
        ImageResult::Loading
    }

    /// Load a bundled asset on web.
    ///
    /// Assets are served from the site root (Vite `public/`), so they are
    /// fetched asynchronously through the same machinery as network images.
    #[cfg(target_arch = "wasm32")]
    pub fn load_asset_image(ctx: &BuildContext, key: &str) -> ImageResult {
        let url = if key.starts_with('/') { key.to_string() } else { format!("/{key}") };
        Self::load_network_image(ctx, &url)
    }

    /// Read the raw bytes of a bundled asset from the platform's asset store.
    #[cfg(not(target_arch = "wasm32"))]
    fn load_asset_bytes(key: &str) -> Result<Vec<u8>, String> {
        #[cfg(target_os = "android")]
        {
            use std::ffi::CString;
            use std::io::Read;

            let app = aimer_events::android_app::get_android_app()
                .ok_or("Android app handle not available")?;
            let manager = app.asset_manager();
            let cstr = CString::new(key).map_err(|e| format!("invalid asset key '{key}': {e}"))?;
            let mut asset = manager
                .open(&cstr)
                .ok_or_else(|| format!("asset '{key}' not found in APK"))?;
            let mut buffer = Vec::new();
            asset
                .read_to_end(&mut buffer)
                .map_err(|e| format!("failed to read asset '{key}': {e}"))?;
            Ok(buffer)
        }
        #[cfg(not(target_os = "android"))]
        {
            for path in Self::asset_candidate_paths(key) {
                if let Ok(bytes) = std::fs::read(&path) {
                    return Ok(bytes);
                }
            }
            Err(format!("asset '{key}' not found"))
        }
    }

    /// Candidate filesystem locations for a bundled asset on desktop and
    /// iOS/macOS, tried in order: the project directory (dev runs), then the
    /// app bundle's resource directory (packaged apps).
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn asset_candidate_paths(key: &str) -> Vec<PathBuf> {
        let mut paths = vec![PathBuf::from(key)];
        if let Ok(exe) = std::env::current_exe()
            && let Some(exe_dir) = exe.parent()
        {
            // macOS: <App>.app/Contents/MacOS/<exe> -> <App>.app/Contents/Resources
            if let Some(contents) = exe_dir.parent() {
                paths.push(
                    contents
                        .join("Resources")
                        .join(key),
                );
            }
            // iOS: <App>.app/<exe> -> <App>.app/<key>
            paths.push(exe_dir.join(key));
        }
        paths
    }

    pub fn load_network_image_with_headers(
        ctx: &BuildContext,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> ImageResult {
        let mut cache = NETWORK_CACHE
            .lock()
            .unwrap();
        match cache.get_mut(url) {
            Some(NetworkImageState::Loaded(id, width, height)) => {
                ctx.canvas
                    .set_texture_size(*id, *width, *height);
                Success(*id)
            }
            Some(NetworkImageState::Ready(bytes, width, height)) => {
                let id = ctx
                    .canvas
                    .load_image(bytes, *width, *height);
                let (w, h) = (*width, *height);
                *cache
                    .get_mut(url)
                    .unwrap() = NetworkImageState::Loaded(id, w, h);
                Success(id)
            }
            Some(NetworkImageState::Loading) => ImageResult::Loading,
            Some(NetworkImageState::Error(err)) => ImageResult::Error(err.to_string()),
            None => {
                cache.insert(url.to_string(), NetworkImageState::Loading);
                let url = url.to_string();
                let headers = headers.clone();
                let window = ctx
                    .window
                    .clone();

                #[cfg(not(target_arch = "wasm32"))]
                ctx.async_handle
                    .spawn(async move {
                        match Self::fetch_full_image_with_headers(&url, &headers, window.clone())
                            .await
                        {
                            Ok(_) => {}
                            Err(err) => {
                                error!("Error to fetch network image : {}", err);
                                // error!("Image URL: {url}");
                                let mut cache = NETWORK_CACHE
                                    .lock()
                                    .unwrap();
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
                        match Self::fetch_full_image_with_headers(
                            &url_clone,
                            &headers,
                            window_clone,
                        )
                        .await
                        {
                            Ok(_) => {}
                            Err(err) => {
                                error!("Failed to fetch network image ({}): {}", url_clone, err);
                                let mut cache = NETWORK_CACHE
                                    .lock()
                                    .unwrap();
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
    async fn fetch_full_image(url: &str, window: WindowHandle) -> Result<(), String> {
        Self::fetch_full_image_with_headers(url, &HashMap::new(), window).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn fetch_full_image_with_headers(
        url: &str,
        maps: &HashMap<String, String>,
        window: WindowHandle,
    ) -> Result<(), String> {
        let bytes = if maps.is_empty() {
            Self::fetch_bytes(url).await?
        } else {
            Self::fetch_bytes_with_headers(url, maps).await?
        };

        let (rgba, width, height) = Self::decode_image_browser(&bytes).await?;

        let mut cache = NETWORK_CACHE
            .lock()
            .unwrap();
        cache.insert(url.to_string(), NetworkImageState::Ready(rgba, width, height));
        drop(cache);
        window.request_redraw();

        Ok(())
    }

    /// Fetch raw bytes from a URL using the browser's `fetch` API.
    #[cfg(target_arch = "wasm32")]
    async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
        use wasm_bindgen::JsCast;

        let web_window = web_sys::window().ok_or("No window found")?;
        let resp_value = wasm_bindgen_futures::JsFuture::from(web_window.fetch_with_str(url))
            .await
            .map_err(|e| format!("{:?}", e))?;
        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|e| format!("{:?}", e))?;
        if !resp.ok() {
            return Err(format!("HTTP error: {}", resp.status()));
        }
        let buf = wasm_bindgen_futures::JsFuture::from(
            resp.array_buffer()
                .map_err(|e| format!("{:?}", e))?,
        )
        .await
        .map_err(|e| format!("{:?}", e))?;
        Ok(js_sys::Uint8Array::new(&buf).to_vec())
    }

    /// Fetch raw bytes with custom headers.
    #[cfg(target_arch = "wasm32")]
    async fn fetch_bytes_with_headers(
        url: &str,
        headers: &HashMap<String, String>,
    ) -> Result<Vec<u8>, String> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::*;
        use web_sys::Headers;

        let js_headers = Headers::new().map_err(|e| format!("{:?}", e))?;
        for (key, value) in headers {
            js_headers
                .append(key, value)
                .map_err(|e| format!("{:?}", e))?;
        }
        let web_window = web_sys::window().ok_or("No window found")?;
        let request_init = web_sys::RequestInit::new();
        request_init.set_method("GET");
        request_init.set_headers(&JsValue::from(js_headers));

        let resp_value = wasm_bindgen_futures::JsFuture::from(
            web_window.fetch_with_str_and_init(url, &request_init),
        )
        .await
        .map_err(|e| format!("{:?}", e))?;
        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|e| format!("{:?}", e))?;
        if !resp.ok() {
            return Err(format!("HTTP error: {}", resp.status()));
        }
        let buf = wasm_bindgen_futures::JsFuture::from(
            resp.array_buffer()
                .map_err(|e| format!("{:?}", e))?,
        )
        .await
        .map_err(|e| format!("{:?}", e))?;
        Ok(js_sys::Uint8Array::new(&buf).to_vec())
    }

    /// Decode raw image bytes (PNG/JPEG/WebP/GIF) to RGBA using the browser's
    /// native decoder via `HtmlImageElement` + `OffscreenCanvas`. This is
    /// dramatically faster than the Rust `image` crate compiled to wasm and
    /// runs asynchronously without blocking the main thread.
    #[cfg(target_arch = "wasm32")]
    async fn decode_image_browser(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::*;

        // Detect mime type from magic bytes so the browser can decode.
        let mime = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            "image/png"
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else if bytes.starts_with(&[0x47, 0x49, 0x46]) {
            "image/gif"
        } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            "image/webp"
        } else {
            "image/png" // fallback — let the browser try
        };

        // Create a Blob with the correct mime type.
        let blob_parts = js_sys::Array::new();
        blob_parts.push(&js_sys::Uint8Array::from(bytes));
        let mut blob_opts = web_sys::BlobPropertyBag::new();
        blob_opts.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &blob_opts)
            .map_err(|e| format!("Blob creation failed: {:?}", e))?;
        let obj_url = web_sys::Url::create_object_url_with_blob(&blob)
            .map_err(|e| format!("Object URL creation failed: {:?}", e))?;

        // Load the image via HtmlImageElement (async, browser-native decode).
        let img = web_sys::HtmlImageElement::new()
            .map_err(|e| format!("Image element creation failed: {:?}", e))?;
        img.set_src(&obj_url);

        let img_ref = img.clone();
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let onload_img = img_ref.clone();
            let onload = Closure::once(move || {
                let _ = onload_img; // prevent premature GC
                resolve
                    .call0(&JsValue::undefined())
                    .unwrap();
            });
            img_ref.set_onload(Some(
                onload
                    .as_ref()
                    .unchecked_ref(),
            ));
            onload.forget();

            let onerror = Closure::once(move || {
                reject
                    .call1(&JsValue::undefined(), &JsValue::from_str("Image load failed"))
                    .unwrap();
            });
            img_ref.set_onerror(Some(
                onerror
                    .as_ref()
                    .unchecked_ref(),
            ));
            onerror.forget();
        });
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|e| format!("Image load failed: {:?}", e))?;

        let w = img.natural_width();
        let h = img.natural_height();

        // Draw to OffscreenCanvas and read back RGBA pixels.
        let canvas = web_sys::OffscreenCanvas::new(w, h)
            .map_err(|e| format!("OffscreenCanvas creation failed: {:?}", e))?;
        let ctx = canvas
            .get_context("2d")
            .map_err(|e| format!("get_context failed: {:?}", e))?
            .ok_or("No 2d context")?
            .dyn_into::<web_sys::OffscreenCanvasRenderingContext2d>()
            .map_err(|e| format!("Context cast failed: {:?}", e))?;
        ctx.draw_image_with_html_image_element(&img, 0.0, 0.0)
            .map_err(|e| format!("drawImage failed: {:?}", e))?;
        let image_data = ctx
            .get_image_data(0.0, 0.0, w as f64, h as f64)
            .map_err(|e| format!("getImageData failed: {:?}", e))?;
        let rgba = image_data
            .data()
            .to_vec();

        web_sys::Url::revoke_object_url(&obj_url).ok();
        Ok((rgba, w, h))
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
            .use_native_tls()
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn fetch_full_image_with_headers(
        url: &str,
        headers: &HashMap<String, String>,
        window: WindowHandle,
    ) -> Result<(), String> {
        let client = Self::create_client()?;

        let mut request_builder = client.get(url);
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| {
                format!("Network Error: {:?},  Source: {:?}", e, e.source())
                // format!("Failed to fetch image: {}", e)
            })?;

        if !response
            .status()
            .is_success()
        {
            return Err(format!("HTTP error: {}", response.status()));
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
            Err(_) => {
                // error!("Failed to decode image: {}", e);
                Err("Failed to decode image".into())
            }
        }
    }
}
