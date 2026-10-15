/// Selects where a native application encodes and presents recorded frames.
///
/// Widget updates, layout, input, and command recording always stay on the UI
/// thread. Browser and headless applications render inline in either mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderMode {
    /// Encode on a worker, allowing one frame ahead without blocking input.
    Threaded,
    /// Encode synchronously on the UI thread, useful for profiling and debugging.
    Inline,
}

impl Default for RenderMode {
    fn default() -> Self {
        if cfg!(target_arch = "wasm32") {
            Self::Inline
        } else {
            Self::Threaded
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{AimerApp, RenderMode};

    #[test]
    fn native_apps_default_to_threaded_and_allow_inline_configuration() {
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(
            AimerApp::new().configured_render_mode(),
            RenderMode::Threaded
        );
        let app = AimerApp::new().render_mode(RenderMode::Inline);
        assert_eq!(app.configured_render_mode(), RenderMode::Inline);
    }
}
