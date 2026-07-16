use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::SvgDocument;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SvgSource {
    Memory(Arc<[u8]>),
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
        Self { source, state: Arc::new(Mutex::new(SvgLoadState::Loading)) }
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
        SvgSource::File(path) => std::fs::read(path).map_err(|error| error.to_string()),
        SvgSource::Network(url) => {
            let response = reqwest::get(url.as_ref())
                .await
                .map_err(|error| error.to_string())?;
            if !response.status().is_success() {
                return Err(format!("SVG request failed with status {}", response.status()));
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
        SvgSource::File(path) => path.to_string_lossy().into_owned(),
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
        return Err(format!("SVG request failed with status {}", response.status()));
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

#[cfg(target_arch = "wasm32")]
fn js_error(error: wasm_bindgen::JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_source_transitions_from_loading_to_ready() {
        let loader = SvgLoader::new(SvgSource::Memory(Arc::from(
            br#"<svg width="2" height="3" xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#
                .as_slice(),
        )));
        assert!(matches!(loader.state(), SvgLoadState::Loading));

        let state = loader.load().await;

        let SvgLoadState::Ready(document) = state else { panic!("memory SVG should load") };
        assert_eq!(document.scene().viewport.width, 2.0);
        assert!(matches!(loader.state(), SvgLoadState::Ready(_)));
    }
}
