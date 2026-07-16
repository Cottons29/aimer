use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    Unavailable(String),
    Unsupported,
}

impl Display for ClipboardError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) => write!(formatter, "clipboard is unavailable: {message}"),
            Self::Unsupported => formatter.write_str("clipboard is unsupported on this platform"),
        }
    }
}

impl std::error::Error for ClipboardError {}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub fn set_text(text: &str) -> Result<(), ClipboardError> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| ClipboardError::Unavailable(error.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|error| ClipboardError::Unavailable(error.to_string()))
}

#[cfg(target_arch = "wasm32")]
pub fn set_text(text: &str) -> Result<(), ClipboardError> {
    let window = web_sys::window()
        .ok_or_else(|| ClipboardError::Unavailable("browser window is missing".into()))?;
    let _ = window
        .navigator()
        .clipboard()
        .write_text(text);
    Ok(())
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "macos", target_os = "windows", target_os = "linux"))
))]
pub const fn set_text(_text: &str) -> Result<(), ClipboardError> {
    Err(ClipboardError::Unsupported)
}
