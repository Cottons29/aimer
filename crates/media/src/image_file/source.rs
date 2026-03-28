use crate::ImageResult::Success;
use crate::{ImageProvider, ImageResult, LoadingResult};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use utils::error;
use widget::base::BuildContext;

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
                            error!("Failed to fetch network image ({}): {}", url, err);
                            let mut cache = NETWORK_CACHE.lock().unwrap();
                            cache.insert(url, NetworkImageState::Error(err.to_string()));
                            window.request_redraw();
                        }
                    }
                });

                ImageResult::Loading
            }
        }
    }

    #[allow(dead_code)]
    async fn fetch_full_image(url: &str, window: &'static winit::window::Window) -> Result<(), &'static str> {
        Self::fetch_full_image_with_headers(url, &HashMap::new(), window).await
    }

    async fn fetch_full_image_with_headers(
        url: &str,
        headers: &HashMap<String, String>,
        window: &'static winit::window::Window,
    ) -> Result<(), &'static str> {
        let client_builder = reqwest::Client::builder().user_agent("aimer-fw/0.1.0");

        let client = client_builder
            .build()
            .map_err(|_| "Failed to create client")?;

        let mut request_builder = client.get(url);
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }

        let response = request_builder.send().await.map_err(|e| {
            error!("Network error: {}", e);
            "Network error"
        })?;

        if !response.status().is_success() {
            error!("HTTP error: {}", response.status());
            return Err("HTTP error");
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

                let mut cache = NETWORK_CACHE.lock().unwrap();
                cache.insert(url.to_string(), NetworkImageState::Ready(rgba_bytes, width, height));
                drop(cache);
                window.request_redraw();
                Ok(())
            }
            Err(e) => {
                error!("Failed to decode image: {}", e);
                Err("Failed to decode image")
            }
        }
    }
}
