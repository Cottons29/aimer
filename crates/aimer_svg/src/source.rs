use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use aimer_venus::Venus;
use crossbeam::channel::{Receiver, Sender, unbounded};

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
            Self::Error(error) => formatter.debug_tuple("Error").field(error).finish(),
        }
    }
}

pub struct SvgLoader {
    source: SvgSource,
    state: RefCell<SvgLoadState>,
    updates: Receiver<SvgLoadState>,
    updates_tx: Sender<SvgLoadState>,
}

impl Clone for SvgLoader {
    fn clone(&self) -> Self {
        let (updates_tx, updates) = unbounded();
        Self {
            source: self.source.clone(),
            state: RefCell::new(self.state()),
            updates,
            updates_tx,
        }
    }
}

impl SvgLoader {
    pub fn new(source: SvgSource) -> Self {
        let (updates_tx, updates) = unbounded();
        Self {
            source,
            state: RefCell::new(SvgLoadState::Loading),
            updates,
            updates_tx,
        }
    }

    pub fn state(&self) -> SvgLoadState {
        let mut latest = None;
        while let Ok(state) = self.updates.try_recv() {
            latest = Some(state);
        }
        if let Some(state) = latest {
            *self.state.borrow_mut() = state;
        }
        self.state.borrow().clone()
    }

    /// Loads and parses the source, publishing the resulting state.
    ///
    /// Native file, asset, network, and parsing work is dispatched through the
    /// Venus runtime. If no Venus runtime is installed on the calling thread,
    /// the load returns an error instead of blocking that thread.
    pub async fn load(&self) -> SvgLoadState {
        let _ = self.updates_tx.send(SvgLoadState::Loading);
        let state = load_source(&self.source).await;
        let _ = self.updates_tx.send(state.clone());
        let _ = self.state();
        state
    }

    /// Returns the source and a one-way state mailbox for a background loader.
    ///
    /// The returned sender is intended to be moved into the loading task. The
    /// loader itself stays on the render thread and applies messages from its
    /// receiver when [`Self::state`] is queried.
    pub(crate) fn background_parts(&self) -> (SvgSource, Sender<SvgLoadState>) {
        (self.source.clone(), self.updates_tx.clone())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn load_source(source: &SvgSource) -> SvgLoadState {
    let Some(venus) = Venus::current() else {
        return venus_unavailable();
    };
    let source = source.clone();
    venus
        .spawn_blocking(move || load_source_blocking(&source))
        .await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn load_source(source: &SvgSource) -> SvgLoadState {
    load_document(load_bytes(source).await)
}

fn load_document(bytes: Result<Vec<u8>, String>) -> SvgLoadState {
    match bytes {
        Ok(bytes) => match SvgDocument::from_svg(bytes) {
            Ok(document) => SvgLoadState::Ready(document),
            Err(error) => SvgLoadState::Error(Arc::from(error.to_string())),
        },
        Err(error) => SvgLoadState::Error(Arc::from(error)),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn venus_unavailable() -> SvgLoadState {
    SvgLoadState::Error(Arc::from("Venus runtime is unavailable"))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_source_blocking(source: &SvgSource) -> SvgLoadState {
    load_document(load_bytes(source))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_bytes(source: &SvgSource) -> Result<Vec<u8>, String> {
    match source {
        SvgSource::Memory(bytes) => Ok(bytes.to_vec()),
        SvgSource::Asset(key) => load_asset_bytes(key),
        SvgSource::File(path) => std::fs::read(path).map_err(|error| error.to_string()),
        SvgSource::Network(url) => {
            let mut response = match ureq::get(url.as_ref()).call() {
                Ok(response) => response,
                Err(ureq::Error::StatusCode(status)) => {
                    return Err(format!("SVG request failed with status {status}"));
                }
                Err(error) => return Err(error.to_string()),
            };
            if !response.status().is_success() {
                return Err(format!(
                    "SVG request failed with status {}",
                    response.status()
                ));
            }
            let mut bytes = Vec::new();
            response
                .body_mut()
                .as_reader()
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            Ok(bytes)
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
        return Err(format!(
            "SVG request failed with status {}",
            response.status()
        ));
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(js_error)?)
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
            paths.push(contents_directory.join("Resources").join(key));
        }
        paths.push(executable_directory.join(key));
    }
    paths
}

#[cfg(target_arch = "wasm32")]
fn js_error(error: wasm_bindgen::JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn memory_source_transitions_from_loading_to_ready() {
        let venus = aimer_venus::Venus::new();
        venus.install();
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
        assert_eq!(document.scene().viewport.width, 2.0);
        assert!(matches!(loader.state(), SvgLoadState::Ready(_)));
        aimer_venus::Venus::uninstall();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn asset_source_loads_valid_svg_and_reports_missing_or_malformed_assets() {
        use std::sync::Arc;

        let venus = aimer_venus::Venus::new();
        venus.install();
        use super::*;

        let directory = tempfile::tempdir().unwrap();
        let valid_path = directory.path().join("valid.svg");
        std::fs::write(
            &valid_path,
            br#"<svg width="7" height="5" xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#,
        )
        .unwrap();
        let valid = SvgLoader::new(SvgSource::Asset(Arc::from(
            valid_path.to_string_lossy().as_ref(),
        )));
        let SvgLoadState::Ready(document) = valid.load().await else {
            panic!("valid SVG asset should load");
        };
        assert_eq!(document.scene().viewport.width, 7.0);

        let missing = SvgLoader::new(SvgSource::Asset(Arc::from(
            directory
                .path()
                .join("missing.svg")
                .to_string_lossy()
                .as_ref(),
        )));
        assert!(matches!(missing.load().await, SvgLoadState::Error(_)));

        let malformed_path = directory.path().join("malformed.svg");
        std::fs::write(&malformed_path, b"<svg>").unwrap();
        let malformed = SvgLoader::new(SvgSource::Asset(Arc::from(
            malformed_path.to_string_lossy().as_ref(),
        )));
        assert!(matches!(malformed.load().await, SvgLoadState::Error(_)));
        aimer_venus::Venus::uninstall();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn native_file_load_reports_missing_venus_instead_of_blocking() {
        use aimer_venus::Venus;
        use super::*;

        Venus::uninstall();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("native.svg");
        std::fs::write(
            &path,
            br#"<svg width="2" height="2" xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#,
        )
        .unwrap();

        let state = SvgLoader::new(SvgSource::File(path)).load().await;

        assert!(matches!(
            state,
            SvgLoadState::Error(error) if error.contains("Venus runtime is unavailable")
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn native_network_source_loads_through_ureq() {
        use std::io::Write;
        use std::net::TcpListener;
        use super::*;

        let body = br#"<svg width="11" height="13" xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                std::str::from_utf8(body).unwrap()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let venus = aimer_venus::Venus::new();
        venus.install();
        let state = SvgLoader::new(SvgSource::Network(Arc::from(format!(
            "http://{address}/icon.svg"
        ))))
        .load()
        .await;
        aimer_venus::Venus::uninstall();
        server.join().unwrap();

        let SvgLoadState::Ready(document) = state else {
            panic!("network SVG should load");
        };
        assert_eq!(document.scene().viewport.width, 11.0);
        assert_eq!(document.scene().viewport.height, 13.0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn native_network_source_reports_http_status_errors() {
        use std::io::Write;
        use std::net::TcpListener;
        use super::*;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let response =
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(response.as_bytes()).unwrap();
        });

        let venus = aimer_venus::Venus::new();
        venus.install();
        let state = SvgLoader::new(SvgSource::Network(Arc::from(format!(
            "http://{address}/missing.svg"
        ))))
        .load()
        .await;
        aimer_venus::Venus::uninstall();
        server.join().unwrap();

        assert!(matches!(
            state,
            SvgLoadState::Error(error) if error.contains("SVG request failed with status 404")
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn native_network_source_reads_bodies_larger_than_ureq_convenience_limit() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use super::*;

        let body = vec![b' '; 10 * 1024 * 1024 + 1];
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body_length = body.len();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {body_length}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        });

        let venus = aimer_venus::Venus::new();
        venus.install();
        let state = SvgLoader::new(SvgSource::Network(Arc::from(format!(
            "http://{address}/large.svg"
        ))))
        .load()
        .await;
        aimer_venus::Venus::uninstall();
        server.join().unwrap();

        assert!(matches!(
            state,
            SvgLoadState::Error(error) if error.contains("SVG resource limit exceeded")
        ));
    }

    #[test]
    fn asset_url_is_root_relative_on_web() {
        use super::asset_url;

        assert_eq!(asset_url("assets/icon.svg"), "/assets/icon.svg");
        assert_eq!(asset_url("/assets/icon.svg"), "/assets/icon.svg");
    }
}
