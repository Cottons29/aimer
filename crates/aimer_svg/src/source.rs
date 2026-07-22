use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::SvgDocument;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SvgSource {
    Memory(Arc<[u8]>),
    /// A bundled asset registered under `[assets]` in `aimer.toml`.
    Asset(Arc<str>),
    File(PathBuf),
    Network(Arc<str>),
}

#[derive(Clone)]
pub enum SvgLoadState {
    Loading,
    Ready(SvgDocument),
    Error(Arc<str>),
}

impl std::fmt::Debug for SvgLoadState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading => formatter.write_str("Loading"),
            Self::Ready(_) => formatter.write_str("Ready(SvgDocument)"),
            Self::Error(error) => formatter
                .debug_tuple("Error")
                .field(error)
                .finish(),
        }
    }
}

#[derive(Clone)]
pub struct SvgLoader {
    source: SvgSource,
    state: Arc<Mutex<SvgLoadState>>,
}

impl SvgLoader {
    pub fn new(source: SvgSource) -> Self {
        Self {
            source,
            state: Arc::new(Mutex::new(SvgLoadState::Loading)),
        }
    }

    pub fn state(&self) -> SvgLoadState {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub async fn load(&self) -> SvgLoadState {
        *self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = SvgLoadState::Loading;
        let state = match load_bytes(&self.source).await {
            Ok(bytes) => match SvgDocument::from_svg(bytes) {
                Ok(document) => SvgLoadState::Ready(document),
                Err(error) => SvgLoadState::Error(Arc::from(error.to_string())),
            },
            Err(error) => SvgLoadState::Error(Arc::from(error)),
        };
        *self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = state.clone();
        state
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn load_bytes(source: &SvgSource) -> Result<Vec<u8>, String> {
    match source {
        SvgSource::Memory(bytes) => Ok(bytes.to_vec()),
        SvgSource::Asset(key) => load_asset_bytes(key),
        SvgSource::File(path) => std::fs::read(path).map_err(|error| error.to_string()),
        SvgSource::Network(url) => {
            let response = reqwest::get(url.as_ref())
                .await
                .map_err(|error| error.to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "SVG request failed with status {}",
                    response.status()
                ));
            }
            response
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(|error| error.to_string())
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn load_bytes(source: &SvgSource) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    if let SvgSource::Memory(bytes) = source {
        return Ok(bytes.to_vec());
    }
    let url = match source {
        SvgSource::Asset(key) => asset_url(key),
        SvgSource::File(path) => path
            .to_string_lossy()
            .into_owned(),
        SvgSource::Network(url) => url.to_string(),
        SvgSource::Memory(_) => unreachable!(),
    };
    let window = web_sys::window().ok_or_else(|| "browser window is unavailable".to_owned())?;
    let response = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(js_error)?
        .dyn_into::<web_sys::Response>()
        .map_err(js_error)?;
    if !response.ok() {
        return Err(format!(
            "SVG request failed with status {}",
            response.status()
        ));
    }
    let buffer = JsFuture::from(
        response
            .array_buffer()
            .map_err(js_error)?,
    )
    .await
    .map_err(js_error)?;
    let bytes = js_sys::Uint8Array::new(&buffer);
    let mut output = vec![0; bytes.length() as usize];
    bytes.copy_to(&mut output);
    Ok(output)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn asset_url(key: &str) -> String {
    if key.starts_with('/') {
        key.to_owned()
    } else {
        format!("/{key}")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_asset_bytes(key: &str) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "android")]
    {
        use std::ffi::CString;
        use std::io::Read;

        let app = aimer_events::android_app::get_android_app()
            .ok_or("Android app handle not available")?;
        let manager = app.asset_manager();
        let key_cstr =
            CString::new(key).map_err(|error| format!("invalid asset key '{key}': {error}"))?;
        let mut asset = manager
            .open(&key_cstr)
            .ok_or_else(|| format!("asset '{key}' not found in APK"))?;
        let mut bytes = Vec::new();
        asset
            .read_to_end(&mut bytes)
            .map_err(|error| format!("failed to read asset '{key}': {error}"))?;
        Ok(bytes)
    }

    #[cfg(not(target_os = "android"))]
    {
        for path in asset_candidate_paths(key) {
            if let Ok(bytes) = std::fs::read(path) {
                return Ok(bytes);
            }
        }
        Err(format!("asset '{key}' not found"))
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn asset_candidate_paths(key: &str) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(key)];
    if let Ok(executable) = std::env::current_exe()
        && let Some(executable_directory) = executable.parent()
    {
        if let Some(contents_directory) = executable_directory.parent() {
            paths.push(
                contents_directory
                    .join("Resources")
                    .join(key),
            );
        }
        paths.push(executable_directory.join(key));
    }
    paths
}

#[cfg(target_arch = "wasm32")]
fn js_error(error: wasm_bindgen::JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn memory_source_transitions_from_loading_to_ready() {
        use super::*;
        let loader = SvgLoader::new(SvgSource::Memory(Arc::from(
            br#"<svg width="2" height="3" xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#
                .as_slice(),
        )));
        assert!(matches!(loader.state(), SvgLoadState::Loading));

        let state = loader.load().await;

        let SvgLoadState::Ready(document) = state else {
            panic!("memory SVG should load")
        };
        assert_eq!(
            document
                .scene()
                .viewport
                .width,
            2.0
        );
        assert!(matches!(loader.state(), SvgLoadState::Ready(_)));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn asset_source_loads_valid_svg_and_reports_missing_or_malformed_assets() {
        use std::sync::Arc;

        use super::*;

        let directory = tempfile::tempdir().unwrap();
        let valid_path = directory
            .path()
            .join("valid.svg");
        std::fs::write(
            &valid_path,
            br#"<svg width="7" height="5" xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#,
        )
        .unwrap();
        let valid = SvgLoader::new(SvgSource::Asset(Arc::from(
            valid_path
                .to_string_lossy()
                .as_ref(),
        )));
        let SvgLoadState::Ready(document) = valid.load().await else {
            panic!("valid SVG asset should load");
        };
        assert_eq!(
            document
                .scene()
                .viewport
                .width,
            7.0
        );

        let missing = SvgLoader::new(SvgSource::Asset(Arc::from(
            directory
                .path()
                .join("missing.svg")
                .to_string_lossy()
                .as_ref(),
        )));
        assert!(matches!(missing.load().await, SvgLoadState::Error(_)));

        let malformed_path = directory
            .path()
            .join("malformed.svg");
        std::fs::write(&malformed_path, b"<svg>").unwrap();
        let malformed = SvgLoader::new(SvgSource::Asset(Arc::from(
            malformed_path
                .to_string_lossy()
                .as_ref(),
        )));
        assert!(matches!(malformed.load().await, SvgLoadState::Error(_)));
    }

    #[test]
    fn asset_url_is_root_relative_on_web() {
        use super::asset_url;

        assert_eq!(asset_url("assets/icon.svg"), "/assets/icon.svg");
        assert_eq!(asset_url("/assets/icon.svg"), "/assets/icon.svg");
    }
}
