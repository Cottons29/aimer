//! macOS, Windows and Linux, through `arboard`.
//!
//! The handle is opened once and kept for the life of the process. That is not
//! only cheaper than opening one per copy — on X11 the clipboard's contents are
//! *owned by the process that put them there*, so a handle dropped right after
//! the write takes the copied text with it.

use std::sync::{Mutex, OnceLock};

use super::ClipboardError;

static CLIPBOARD: OnceLock<Option<Mutex<arboard::Clipboard>>> = OnceLock::new();

pub(super) fn set_text(text: &str) -> Result<(), ClipboardError> {
    with_clipboard(|clipboard| clipboard.set_text(text).map_err(unavailable))
}

pub(super) fn get_text() -> Result<String, ClipboardError> {
    with_clipboard(|clipboard| clipboard.get_text().map_err(unavailable))
}

/// Runs `call` against the process-wide clipboard handle.
fn with_clipboard<T>(
    call: impl FnOnce(&mut arboard::Clipboard) -> Result<T, ClipboardError>,
) -> Result<T, ClipboardError> {
    let shared = CLIPBOARD
        .get_or_init(|| arboard::Clipboard::new().ok().map(Mutex::new))
        .as_ref()
        .ok_or_else(|| {
            ClipboardError::Unavailable("the platform clipboard could not be opened".into())
        })?;
    let mut clipboard = shared
        .lock()
        .map_err(|_| ClipboardError::Unavailable("the clipboard lock was poisoned".into()))?;
    call(&mut clipboard)
}

fn unavailable(error: arboard::Error) -> ClipboardError {
    ClipboardError::Unavailable(error.to_string())
}
