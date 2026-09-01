use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
#[cfg(not(target_arch = "wasm32"))]
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use aimer_utils::error;
use aimer_widget::base::{BuildContext, WindowHandle};
use crossbeam::channel::{Receiver, Sender, TryRecvError, unbounded};
use once_cell::sync::Lazy;

use crate::ImageResult::Success;
use crate::{ImageProvider, ImageResult};

/// Decoded pixels are much larger than the small `Loaded(texture_id, ...)`
/// records. Keep a generous working set, then release cold decoded entries;
/// the GPU cache has its own byte budget and protects textures referenced by
/// the current draw list.
const DECODED_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
const DECODED_CACHE_MAX_ENTRIES: usize = 512;
const DECODED_CACHE_IDLE_ACCESSES: u64 = 128;
static DECODED_CACHE_ACCESS_CLOCK: AtomicU64 = AtomicU64::new(0);

#[inline]
fn next_cache_access() -> u64 {
    DECODED_CACHE_ACCESS_CLOCK
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1)
}

#[inline]
fn touch_cache_key<K: Clone + Eq + Hash>(access: &mut HashMap<K, u64>, key: &K) -> u64 {
    let tick = next_cache_access();
    access.insert(key.clone(), tick);
    tick
}

#[inline]
fn remove_cache_key<K: Eq + Hash>(access: &mut HashMap<K, u64>, key: &K) {
    access.remove(key);
}

#[derive(Clone, Debug)]
enum ImageCacheState {
    Loading,
    Ready(Vec<u8>, u32, u32, u32, u32),
    Loaded(u32, u32, u32),
    Error(String),
}

enum ImageCacheUpdate {
    File {
        path: PathBuf,
        state: ImageCacheState,
    },
    Network {
        url: String,
        state: ImageCacheState,
    },
    #[cfg(not(target_arch = "wasm32"))]
    Asset {
        key: String,
        state: ImageCacheState,
    },
}

struct ImageCacheMailbox {
    sender: Sender<ImageCacheUpdate>,
    receiver: Receiver<ImageCacheUpdate>,
}

impl ImageCacheMailbox {
    fn new() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }
}

static IMAGE_CACHE_MAILBOX: Lazy<ImageCacheMailbox> = Lazy::new(ImageCacheMailbox::new);

/// Image caches are owned by the thread that owns the render `BuildContext`.
/// Background decoders never access these maps directly; they publish completed
/// states through `IMAGE_CACHE_MAILBOX`, and the render thread drains it before
/// looking up an image. `BuildContext` contains the non-`Send` canvas/widget
/// state, so image lookup is already confined to that render thread.
#[derive(Default)]
struct ImageCaches {
    file: HashMap<PathBuf, ImageCacheState>,
    file_access: HashMap<PathBuf, u64>,
    network: HashMap<String, ImageCacheState>,
    network_access: HashMap<String, u64>,
    #[cfg(not(target_arch = "wasm32"))]
    asset: HashMap<String, ImageCacheState>,
    #[cfg(not(target_arch = "wasm32"))]
    asset_access: HashMap<String, u64>,
}

thread_local! {
    static IMAGE_CACHES: RefCell<ImageCaches> = RefCell::new(ImageCaches::default());
}

enum CacheLookup {
    StartLoad,
    Loading,
    Ready {
        bytes: Vec<u8>,
        upload_width: u32,
        upload_height: u32,
        width: u32,
        height: u32,
    },
    Loaded {
        id: u32,
        width: u32,
        height: u32,
    },
    Error(String),
}

/// Evicts only cold entries. A visible image is touched by its provider on
/// every draw, so it remains inside the idle grace period and cannot be
/// evicted merely because another image was decoded. If the active working
/// set itself exceeds the budget, it is retained until entries become cold;
/// correctness and visible-content residency take precedence over a hard
/// byte cap.
fn prune_decoded_cache<K, S>(
    cache: &mut HashMap<K, S>,
    access: &mut HashMap<K, u64>,
    now: u64,
    decoded_bytes: impl Fn(&S) -> usize,
    is_loading: impl Fn(&S) -> bool,
) where
    K: Clone + Eq + Hash,
{
    let mut total_bytes = cache.values().map(&decoded_bytes).sum::<usize>();
    let mut candidates = cache
        .iter()
        .filter_map(|(key, state)| {
            if is_loading(state) {
                return None;
            }
            Some((
                key.clone(),
                access.get(key).copied().unwrap_or(0),
                decoded_bytes(state),
            ))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|(_, last_access, _)| *last_access);

    for (key, last_access, bytes) in candidates {
        let idle = now.saturating_sub(last_access);
        if idle < DECODED_CACHE_IDLE_ACCESSES {
            continue;
        }
        if bytes > 0 || cache.len() > DECODED_CACHE_MAX_ENTRIES {
            if cache.remove(&key).is_some() {
                total_bytes = total_bytes.saturating_sub(bytes);
                remove_cache_key(access, &key);
            }
        }
        if total_bytes <= DECODED_CACHE_BUDGET_BYTES
            && cache.len() <= DECODED_CACHE_MAX_ENTRIES
        {
            // Do not scan the remaining cold tail once both bounds are met;
            // keeping it would only make a later visible hit pay a decode.
            break;
        }
    }

    access.retain(|key, _| cache.contains_key(key));
}

#[inline]
fn image_decoded_bytes(state: &ImageCacheState) -> usize {
    match state {
        ImageCacheState::Ready(bytes, ..) => bytes.capacity(),
        ImageCacheState::Loading | ImageCacheState::Loaded(..) | ImageCacheState::Error(..) => 0,
    }
}

#[inline]
fn image_is_loading(state: &ImageCacheState) -> bool {
    matches!(state, ImageCacheState::Loading)
}

#[inline]
fn prune_image_cache<K: Clone + Eq + Hash>(
    cache: &mut HashMap<K, ImageCacheState>,
    access: &mut HashMap<K, u64>,
    now: u64,
) {
    prune_decoded_cache(cache, access, now, image_decoded_bytes, image_is_loading);
}

fn lookup_cache<K: Clone + Eq + Hash>(
    cache: &mut HashMap<K, ImageCacheState>,
    access: &mut HashMap<K, u64>,
    key: &K,
) -> CacheLookup {
    if !cache.contains_key(key) {
        let now = touch_cache_key(access, key);
        cache.insert(key.clone(), ImageCacheState::Loading);
        prune_image_cache(cache, access, now);
        return CacheLookup::StartLoad;
    }

    let now = touch_cache_key(access, key);
    prune_image_cache(cache, access, now);
    if matches!(cache.get(key), Some(ImageCacheState::Ready(..))) {
        let state = std::mem::replace(
            cache.get_mut(key).expect("cache entry was touched above"),
            ImageCacheState::Loading,
        );
        return match state {
            ImageCacheState::Ready(bytes, upload_width, upload_height, width, height) => {
                CacheLookup::Ready {
                    bytes,
                    upload_width,
                    upload_height,
                    width,
                    height,
                }
            }
            _ => unreachable!("cache entry changed while it was borrowed"),
        };
    }

    match cache.get(key).expect("cache entry was touched above") {
        ImageCacheState::Loaded(id, width, height) => CacheLookup::Loaded {
            id: *id,
            width: *width,
            height: *height,
        },
        ImageCacheState::Loading => CacheLookup::Loading,
        ImageCacheState::Error(error) => CacheLookup::Error(error.clone()),
        ImageCacheState::Ready(..) => unreachable!("ready cache entry was handled above"),
    }
}

#[inline]
fn set_cache_state<K: Clone + Eq + Hash>(
    cache: &mut HashMap<K, ImageCacheState>,
    access: &mut HashMap<K, u64>,
    key: K,
    state: ImageCacheState,
) {
    let now = touch_cache_key(access, &key);
    cache.insert(key, state);
    prune_image_cache(cache, access, now);
}

#[inline]
fn remove_cache_entry<K: Eq + Hash>(
    cache: &mut HashMap<K, ImageCacheState>,
    access: &mut HashMap<K, u64>,
    key: &K,
) {
    cache.remove(key);
    remove_cache_key(access, key);
}

impl ImageCaches {
    fn apply_update(&mut self, update: ImageCacheUpdate) {
        match update {
            ImageCacheUpdate::File { path, state } => {
                set_cache_state(&mut self.file, &mut self.file_access, path, state);
            }
            ImageCacheUpdate::Network { url, state } => {
                set_cache_state(&mut self.network, &mut self.network_access, url, state);
            }
            #[cfg(not(target_arch = "wasm32"))]
            ImageCacheUpdate::Asset { key, state } => {
                set_cache_state(&mut self.asset, &mut self.asset_access, key, state);
            }
        }
    }

    fn lookup_file(&mut self, path: &PathBuf) -> CacheLookup {
        lookup_cache(&mut self.file, &mut self.file_access, path)
    }

    fn file_texture_id(&self, path: &PathBuf) -> Option<u32> {
        match self.file.get(path) {
            Some(ImageCacheState::Loaded(id, ..)) => Some(*id),
            _ => None,
        }
    }

    fn remove_file(&mut self, path: &PathBuf) {
        remove_cache_entry(&mut self.file, &mut self.file_access, path);
    }

    fn set_file_loaded(&mut self, path: PathBuf, id: u32, width: u32, height: u32) {
        set_cache_state(
            &mut self.file,
            &mut self.file_access,
            path,
            ImageCacheState::Loaded(id, width, height),
        );
    }

    fn lookup_network(&mut self, url: &String) -> CacheLookup {
        lookup_cache(&mut self.network, &mut self.network_access, url)
    }

    fn network_texture_id(&self, url: &String) -> Option<u32> {
        match self.network.get(url) {
            Some(ImageCacheState::Loaded(id, ..)) => Some(*id),
            _ => None,
        }
    }

    fn remove_network(&mut self, url: &String) {
        remove_cache_entry(&mut self.network, &mut self.network_access, url);
    }

    fn set_network_loaded(&mut self, url: String, id: u32, width: u32, height: u32) {
        set_cache_state(
            &mut self.network,
            &mut self.network_access,
            url,
            ImageCacheState::Loaded(id, width, height),
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn lookup_asset(&mut self, key: &String) -> CacheLookup {
        lookup_cache(&mut self.asset, &mut self.asset_access, key)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn asset_texture_id(&self, key: &String) -> Option<u32> {
        match self.asset.get(key) {
            Some(ImageCacheState::Loaded(id, ..)) => Some(*id),
            _ => None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn remove_asset(&mut self, key: &String) {
        remove_cache_entry(&mut self.asset, &mut self.asset_access, key);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_asset_loaded(&mut self, key: String, id: u32, width: u32, height: u32) {
        set_cache_state(
            &mut self.asset,
            &mut self.asset_access,
            key,
            ImageCacheState::Loaded(id, width, height),
        );
    }
}

fn drain_cache_updates() {
    IMAGE_CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        loop {
            match IMAGE_CACHE_MAILBOX.receiver.try_recv() {
                Ok(update) => caches.apply_update(update),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
    });
}

#[inline]
fn send_cache_update(update: ImageCacheUpdate) {
    if IMAGE_CACHE_MAILBOX.sender.send(update).is_err() {
        error!("Image cache update channel is disconnected");
    }
}
#[allow(dead_code)]
const BROWSER_IMAGE_MAX_DIMENSION: u32 = 2048;
#[allow(dead_code)]
fn constrained_browser_image_size(width: u32, height: u32) -> (u32, u32) {
    if width <= BROWSER_IMAGE_MAX_DIMENSION && height <= BROWSER_IMAGE_MAX_DIMENSION {
        return (width, height);
    }

    if width >= height {
        (
            BROWSER_IMAGE_MAX_DIMENSION,
            ((height as u64 * BROWSER_IMAGE_MAX_DIMENSION as u64) / width as u64).max(1) as u32,
        )
    } else {
        (
            ((width as u64 * BROWSER_IMAGE_MAX_DIMENSION as u64) / height as u64).max(1) as u32,
            BROWSER_IMAGE_MAX_DIMENSION,
        )
    }
}
#[allow(dead_code)]
fn image_mime_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x47, 0x49, 0x46]) {
        "image/gif"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/png"
    }
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
        drain_cache_updates();

        let stale_id = IMAGE_CACHES.with(|caches| caches.borrow().file_texture_id(path));
        if stale_id.is_some_and(|id| !ctx.canvas.is_texture_available(id)) {
            IMAGE_CACHES.with(|caches| caches.borrow_mut().remove_file(path));
        }

        match IMAGE_CACHES.with(|caches| caches.borrow_mut().lookup_file(path)) {
            CacheLookup::Loaded {
                id,
                width,
                height,
            } => {
                ctx.canvas.set_texture_size(id, width, height);
                Success(id)
            }
            CacheLookup::Ready {
                bytes,
                upload_width,
                upload_height,
                width,
                height,
            } => {
                // Decoded on a background thread; upload to the GPU here (on
                // the render thread, where the canvas/GPU lives) and cache id.
                let id = ctx.canvas.load_image(&bytes, upload_width, upload_height);
                ctx.canvas.set_texture_size(id, width, height);
                IMAGE_CACHES.with(|caches| {
                    caches
                        .borrow_mut()
                        .set_file_loaded(path.clone(), id, width, height)
                });
                Success(id)
            }
            CacheLookup::Loading => ImageResult::Loading,
            CacheLookup::Error(error) => ImageResult::Error(error),
            CacheLookup::StartLoad => {
                // Cache miss: decode off the render thread so scrolling a large
                // image into view does not block the frame for hundreds of
                // milliseconds.
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let path_buf = path.clone();
                    let window = ctx.window.clone();
                    ctx.async_handle.spawn_blocking(move || {
                        let state = match image::open(&path_buf) {
                            Ok(image) => {
                                let rgba = image.to_rgba8();
                                let (width, height) = (rgba.width(), rgba.height());
                                ImageCacheState::Ready(
                                    rgba.into_raw(),
                                    width,
                                    height,
                                    width,
                                    height,
                                )
                            }
                            Err(_) => ImageCacheState::Error("Failed to load image".into()),
                        };
                        send_cache_update(ImageCacheUpdate::File {
                            path: path_buf,
                            state,
                        });
                        window.request_redraw();
                    });
                }

                // wasm: fetch and decode asynchronously via the browser's
                // native decoder (much faster than the Rust `image` crate).
                #[cfg(target_arch = "wasm32")]
                {
                    let url = path.to_string_lossy().to_string();
                    let path_buf = path.clone();
                    let window = ctx.window.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let state = match Self::fetch_bytes(&url).await {
                            Ok(bytes) => match Self::decode_image_browser(&bytes).await {
                                Ok((rgba, upload_w, upload_h, w, h)) => {
                                    ImageCacheState::Ready(rgba, upload_w, upload_h, w, h)
                                }
                                Err(error) => ImageCacheState::Error(error),
                            },
                            Err(error) => ImageCacheState::Error(error),
                        };
                        send_cache_update(ImageCacheUpdate::File {
                            path: path_buf,
                            state,
                        });
                        window.request_redraw();
                    });
                }

                ImageResult::Loading
            }
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
        let key_owned = key.to_string();
        drain_cache_updates();

        let stale_id = IMAGE_CACHES.with(|caches| caches.borrow().asset_texture_id(&key_owned));
        if stale_id.is_some_and(|id| !ctx.canvas.is_texture_available(id)) {
            IMAGE_CACHES.with(|caches| caches.borrow_mut().remove_asset(&key_owned));
        }

        match IMAGE_CACHES.with(|caches| caches.borrow_mut().lookup_asset(&key_owned)) {
            CacheLookup::Loaded {
                id,
                width,
                height,
            } => {
                ctx.canvas.set_texture_size(id, width, height);
                Success(id)
            }
            CacheLookup::Ready {
                bytes,
                upload_width,
                upload_height,
                width,
                height,
            } => {
                // Decoded on a background thread; upload on the render thread.
                let id = ctx.canvas.load_image(&bytes, upload_width, upload_height);
                ctx.canvas.set_texture_size(id, width, height);
                IMAGE_CACHES.with(|caches| {
                    caches.borrow_mut().set_asset_loaded(
                        key_owned.clone(),
                        id,
                        width,
                        height,
                    )
                });
                Success(id)
            }
            CacheLookup::Loading => ImageResult::Loading,
            CacheLookup::Error(error) => ImageResult::Error(error),
            CacheLookup::StartLoad => {
                // Cache miss: read + decode the asset off the render thread so
                // scrolling it into view does not block the frame.
                let task_key = key_owned;
                let window = ctx.window.clone();
                ctx.async_handle.spawn_blocking(move || {
                    let state = match Self::load_asset_bytes(&task_key) {
                        Ok(bytes) => match image::load_from_memory(&bytes) {
                            Ok(image) => {
                                let rgba = image.to_rgba8();
                                let (width, height) = (rgba.width(), rgba.height());
                                ImageCacheState::Ready(
                                    rgba.into_raw(),
                                    width,
                                    height,
                                    width,
                                    height,
                                )
                            }
                            Err(_) => ImageCacheState::Error(format!(
                                "Failed to decode asset image '{task_key}'"
                            )),
                        },
                        Err(error) => ImageCacheState::Error(error),
                    };
                    send_cache_update(ImageCacheUpdate::Asset {
                        key: task_key,
                        state,
                    });
                    window.request_redraw();
                });
                ImageResult::Loading
            }
        }
    }

    /// Load a bundled asset on web.
    ///
    /// Assets are served from the site root (Vite `public/`), so they are
    /// fetched asynchronously through the same machinery as network images.
    #[cfg(target_arch = "wasm32")]
    pub fn load_asset_image(ctx: &BuildContext, key: &str) -> ImageResult {
        let url = if key.starts_with('/') {
            key.to_string()
        } else {
            format!("/{key}")
        };
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
                paths.push(contents.join("Resources").join(key));
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
        drain_cache_updates();
        let url_owned = url.to_string();

        let stale_id = IMAGE_CACHES.with(|caches| caches.borrow().network_texture_id(&url_owned));
        if stale_id.is_some_and(|id| !ctx.canvas.is_texture_available(id)) {
            IMAGE_CACHES.with(|caches| caches.borrow_mut().remove_network(&url_owned));
        }

        match IMAGE_CACHES.with(|caches| caches.borrow_mut().lookup_network(&url_owned)) {
            CacheLookup::Loaded {
                id,
                width,
                height,
            } => {
                ctx.canvas.set_texture_size(id, width, height);
                Success(id)
            }
            CacheLookup::Ready {
                bytes,
                upload_width,
                upload_height,
                width,
                height,
            } => {
                let id = ctx.canvas.load_image(&bytes, upload_width, upload_height);
                ctx.canvas.set_texture_size(id, width, height);
                IMAGE_CACHES.with(|caches| {
                    caches.borrow_mut().set_network_loaded(
                        url_owned.clone(),
                        id,
                        width,
                        height,
                    )
                });
                Success(id)
            }
            CacheLookup::Loading => ImageResult::Loading,
            CacheLookup::Error(error) => ImageResult::Error(error),
            CacheLookup::StartLoad => {
                let headers = headers.clone();
                let window = ctx.window.clone();

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let task_url = url_owned;
                    ctx.async_handle.spawn(async move {
                        let state = match Self::fetch_full_image_with_headers(&task_url, &headers)
                            .await
                        {
                            Ok(state) => state,
                            Err(error) => {
                                error!("Error to fetch network image : {}", error);
                                ImageCacheState::Error(error)
                            }
                        };
                        send_cache_update(ImageCacheUpdate::Network {
                            url: task_url,
                            state,
                        });
                        window.request_redraw();
                    });
                }

                #[cfg(target_arch = "wasm32")]
                {
                    wasm_bindgen_futures::spawn_local(async move {
                        let state = match Self::fetch_full_image_with_headers(&url_owned, &headers)
                            .await
                        {
                            Ok(state) => state,
                            Err(error) => {
                                error!(
                                    "Failed to fetch network image ({}): {}",
                                    url_owned, error
                                );
                                ImageCacheState::Error(error)
                            }
                        };
                        send_cache_update(ImageCacheUpdate::Network {
                            url: url_owned,
                            state,
                        });
                        window.request_redraw();
                    });
                }

                ImageResult::Loading
            }
        }
    }

    #[allow(dead_code)]
    async fn fetch_full_image(url: &str, window: WindowHandle) -> Result<(), String> {
        let state = Self::fetch_full_image_with_headers(url, &HashMap::new()).await?;
        send_cache_update(ImageCacheUpdate::Network {
            url: url.to_string(),
            state,
        });
        window.request_redraw();
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    async fn fetch_full_image_with_headers(
        url: &str,
        maps: &HashMap<String, String>,
    ) -> Result<ImageCacheState, String> {
        let bytes = if maps.is_empty() {
            Self::fetch_bytes(url).await?
        } else {
            Self::fetch_bytes_with_headers(url, maps).await?
        };

        let (rgba, upload_width, upload_height, width, height) =
            Self::decode_image_browser(&bytes).await?;

        Ok(ImageCacheState::Ready(
            rgba,
            upload_width,
            upload_height,
            width,
            height,
        ))
    }

    /// Fetch raw bytes from a URL using the browser's `fetch` API.
    #[cfg(target_arch = "wasm32")]
    async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
        use wasm_bindgen::JsCast;

        let web_window = web_sys::window().ok_or("No window found")?;
        let resp_value = wasm_bindgen_futures::JsFuture::from(web_window.fetch_with_str(url))
            .await
            .map_err(|e| format!("{:?}", e))?;
        let resp: web_sys::Response = resp_value.dyn_into().map_err(|e| format!("{:?}", e))?;
        if !resp.ok() {
            return Err(format!("HTTP error: {}", resp.status()));
        }
        let buf = wasm_bindgen_futures::JsFuture::from(
            resp.array_buffer().map_err(|e| format!("{:?}", e))?,
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
        let resp: web_sys::Response = resp_value.dyn_into().map_err(|e| format!("{:?}", e))?;
        if !resp.ok() {
            return Err(format!("HTTP error: {}", resp.status()));
        }
        let buf = wasm_bindgen_futures::JsFuture::from(
            resp.array_buffer().map_err(|e| format!("{:?}", e))?,
        )
        .await
        .map_err(|e| format!("{:?}", e))?;
        Ok(js_sys::Uint8Array::new(&buf).to_vec())
    }

    /// Decode raw image bytes (PNG/JPEG/WebP/GIF) to RGBA using the browser's
    /// native decoder via `createImageBitmap` + `OffscreenCanvas`.
    #[cfg(target_arch = "wasm32")]
    async fn decode_image_browser(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32, u32, u32), String> {
        use wasm_bindgen::JsCast;

        let blob_parts = js_sys::Array::new();
        blob_parts.push(&js_sys::Uint8Array::from(bytes));
        let blob_opts = web_sys::BlobPropertyBag::new();
        blob_opts.set_type(image_mime_type(bytes));
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &blob_opts)
            .map_err(|e| format!("Blob creation failed: {:?}", e))?;

        // createImageBitmap asks the browser to decode asynchronously, rather
        // than completing HtmlImageElement decoding on the event loop.
        let window = web_sys::window().ok_or("No window found")?;
        let bitmap = wasm_bindgen_futures::JsFuture::from(
            window
                .create_image_bitmap_with_blob(&blob)
                .map_err(|e| format!("Image bitmap creation failed: {:?}", e))?,
        )
        .await
        .map_err(|e| format!("Image decode failed: {:?}", e))?
        .dyn_into::<web_sys::ImageBitmap>()
        .map_err(|e| format!("Image bitmap cast failed: {:?}", e))?;
        let w = bitmap.width();
        let h = bitmap.height();
        let (upload_width, upload_height) = constrained_browser_image_size(w, h);

        let bitmap = if (upload_width, upload_height) == (w, h) {
            bitmap
        } else {
            let options = web_sys::ImageBitmapOptions::new();
            options.set_resize_width(upload_width);
            options.set_resize_height(upload_height);
            options.set_resize_quality(web_sys::ResizeQuality::High);
            let resized = wasm_bindgen_futures::JsFuture::from(
                window
                    .create_image_bitmap_with_image_bitmap_and_image_bitmap_options(
                        &bitmap, &options,
                    )
                    .map_err(|e| format!("Image bitmap resize failed: {:?}", e))?,
            )
            .await
            .map_err(|e| format!("Image bitmap resize failed: {:?}", e))?
            .dyn_into::<web_sys::ImageBitmap>()
            .map_err(|e| format!("Resized image bitmap cast failed: {:?}", e))?;
            bitmap.close();
            resized
        };

        // Let the browser scale before pixel readback. This avoids copying the
        // full decoded image into WASM only to resize it again on the render path.
        let canvas = web_sys::OffscreenCanvas::new(upload_width, upload_height)
            .map_err(|e| format!("OffscreenCanvas creation failed: {:?}", e))?;
        let ctx = canvas
            .get_context("2d")
            .map_err(|e| format!("get_context failed: {:?}", e))?
            .ok_or("No 2d context")?
            .dyn_into::<web_sys::OffscreenCanvasRenderingContext2d>()
            .map_err(|e| format!("Context cast failed: {:?}", e))?;
        ctx.draw_image_with_image_bitmap(&bitmap, 0.0, 0.0)
            .map_err(|e| format!("drawImage failed: {:?}", e))?;
        let image_data = ctx
            .get_image_data(0.0, 0.0, upload_width as f64, upload_height as f64)
            .map_err(|e| format!("getImageData failed: {:?}", e))?;
        let rgba = image_data.data().to_vec();

        bitmap.close();
        Ok((rgba, upload_width, upload_height, w, h))
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
    ) -> Result<ImageCacheState, String> {
        let client = Self::create_client()?;

        let mut request_builder = client.get(url);
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }

        let response = request_builder.send().await.map_err(|e| {
            format!("Network Error: {:?},  Source: {:?}", e, e.source())
            // format!("Failed to fetch image:
            // {}", e)
        })?;

        if !response.status().is_success() {
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

                Ok(ImageCacheState::Ready(
                    rgba_bytes,
                    width,
                    height,
                    width,
                    height,
                ))
            }
            Err(_) => {
                // error!("Failed to decode image: {}", e);
                Err("Failed to decode image".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{
        ImageCacheState, ImageCacheUpdate, IMAGE_CACHES, DECODED_CACHE_BUDGET_BYTES,
        DECODED_CACHE_IDLE_ACCESSES, constrained_browser_image_size, drain_cache_updates,
        image_mime_type, prune_decoded_cache, send_cache_update,
    };

    #[test]
    fn detects_browser_supported_image_mime_types() {
        assert_eq!(image_mime_type(b"\x89PNG\r\n\x1a\n"), "image/png");
        assert_eq!(image_mime_type(b"\xff\xd8\xff\xe0"), "image/jpeg");
        assert_eq!(image_mime_type(b"GIF89a"), "image/gif");
        assert_eq!(image_mime_type(b"RIFF\x04\x00\x00\x00WEBP"), "image/webp");
    }

    #[test]
    fn unknown_image_data_uses_browser_decode_fallback() {
        assert_eq!(image_mime_type(b"unknown"), "image/png");
    }

    #[test]
    fn browser_decode_constrains_oversized_images_before_pixel_readback() {
        assert_eq!(constrained_browser_image_size(4096, 2048), (2048, 1024));
        assert_eq!(constrained_browser_image_size(2048, 4096), (1024, 2048));
        assert_eq!(constrained_browser_image_size(1024, 512), (1024, 512));
    }

    #[test]
    fn decoded_cache_prunes_cold_entries_but_keeps_visible_and_loading_work() {
        struct Entry {
            bytes: usize,
            loading: bool,
        }

        let now = DECODED_CACHE_IDLE_ACCESSES + 1;
        let mut cache = HashMap::from([
            (
                "cold",
                Entry {
                    bytes: DECODED_CACHE_BUDGET_BYTES,
                    loading: false,
                },
            ),
            (
                "visible",
                Entry {
                    bytes: DECODED_CACHE_BUDGET_BYTES,
                    loading: false,
                },
            ),
            (
                "loading",
                Entry {
                    bytes: DECODED_CACHE_BUDGET_BYTES,
                    loading: true,
                },
            ),
        ]);
        let mut access = HashMap::from([("cold", 0), ("visible", now), ("loading", 0)]);

        prune_decoded_cache(
            &mut cache,
            &mut access,
            now,
            |entry| entry.bytes,
            |entry| entry.loading,
        );

        assert!(!cache.contains_key("cold"));
        assert!(cache.contains_key("visible"));
        assert!(cache.contains_key("loading"));
        assert!(!access.contains_key("cold"));
    }

    #[test]
    fn background_decode_updates_are_delivered_through_the_channel() {
        let path = PathBuf::from("__aimer_assets_channel_test__.png");
        IMAGE_CACHES.with(|caches| {
            caches.borrow_mut().remove_file(&path);
        });

        let update_path = path.clone();
        std::thread::spawn(move || {
            send_cache_update(ImageCacheUpdate::File {
                path: update_path,
                state: ImageCacheState::Ready(vec![1, 2, 3, 4], 1, 1, 1, 1),
            });
        })
        .join()
        .unwrap();

        IMAGE_CACHES.with(|caches| {
            assert!(!caches.borrow().file.contains_key(&path));
        });
        drain_cache_updates();
        IMAGE_CACHES.with(|caches| {
            assert!(matches!(
                caches.borrow().file.get(&path),
                Some(ImageCacheState::Ready(bytes, 1, 1, 1, 1)) if bytes == &[1, 2, 3, 4]
            ));
            caches.borrow_mut().remove_file(&path);
        });
    }
}
