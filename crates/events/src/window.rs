use std::sync::OnceLock;
use winit::window::Window;

static GLOBAL_WINDOW: OnceLock<&'static Window> = OnceLock::new();

/// Store the application window reference so other crates can access it.
pub fn set_window(window: &'static Window) {
    let _ = GLOBAL_WINDOW.set(window);
}

/// Retrieve the application window reference, if it has been set.
pub fn get_window() -> Option<&'static Window> {
    GLOBAL_WINDOW.get().copied()
}
