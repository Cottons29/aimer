#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeBackendChoice {
    Vulkan,
    OpenGl,
}

fn choose_native_backend(
    initialize_vulkan: impl FnOnce() -> bool,
    initialize_opengl: impl FnOnce() -> bool,
) -> Option<NativeBackendChoice> {
    if initialize_vulkan() {
        Some(NativeBackendChoice::Vulkan)
    } else if initialize_opengl() {
        Some(NativeBackendChoice::OpenGl)
    } else {
        None
    }
}

#[cfg(all(target_arch = "wasm32", any(feature = "wgpu", feature = "web")))]
mod h5canva;
#[cfg(all(target_arch = "wasm32", any(feature = "wgpu", feature = "web")))]
pub use h5canva::render_ctx::H5CanvasApi;

#[cfg(all(not(target_arch = "wasm32"), feature = "wgpu"))]
mod wgpu_ctx;
#[cfg(all(not(target_arch = "wasm32"), feature = "wgpu"))]
pub use wgpu_ctx::render_ctx::WgpuApi;

#[cfg(all(feature = "native", not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
mod metal_ctx;
#[cfg(all(feature = "native", not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
pub use metal_ctx::render_ctx::MetalApi;

#[cfg(all(feature = "native", not(feature = "wgpu"), target_os = "windows"))]
mod dx12_ctx;
#[cfg(all(feature = "native", not(feature = "wgpu"), target_os = "windows"))]
pub use dx12_ctx::render_ctx::Dx12Api;

#[cfg(all(feature = "native", not(feature = "wgpu"), any(target_os = "linux", target_os = "android")))]
mod opengl_ctx;
#[cfg(all(feature = "native", not(feature = "wgpu"), any(target_os = "linux", target_os = "android")))]
pub use opengl_ctx::render_ctx::OpenGlApi;

#[cfg(all(feature = "native", not(feature = "wgpu"), any(target_os = "linux", target_os = "android")))]
mod native_auto_ctx;
#[cfg(all(feature = "native", not(feature = "wgpu"), any(target_os = "linux", target_os = "android")))]
pub use native_auto_ctx::NativeAutoApi;

#[cfg(all(feature = "native", not(feature = "wgpu"), any(target_os = "linux", target_os = "android")))]
mod vulkan_ctx;
#[cfg(all(feature = "native", not(feature = "wgpu"), any(target_os = "linux", target_os = "android")))]
pub use vulkan_ctx::render_ctx::VulkanApi;

#[cfg(all(feature = "native", not(feature = "wgpu"), feature = "raster-thread", any(target_os = "macos", target_os = "ios")))]
compile_error!("aimer_quiver's Metal renderer does not support `raster-thread` yet");
#[cfg(all(feature = "native", not(feature = "wgpu"), feature = "raster-thread", target_os = "windows"))]
compile_error!("aimer_quiver's DirectX renderer does not support `raster-thread`");
#[cfg(all(feature = "native", not(feature = "wgpu"), feature = "raster-thread", any(target_os = "linux", target_os = "android")))]
compile_error!("aimer_quiver's OpenGL renderer does not support `raster-thread`");
#[cfg(all(feature = "wgpu", feature = "raster-thread", not(target_arch = "wasm32")))]
compile_error!("the WGPU renderer's custom pipeline trait is not `Send`, so it cannot use `raster-thread`");
#[cfg(all(target_arch = "wasm32", not(feature = "wgpu"), not(feature = "web")))]
compile_error!("enable the `web` or `wgpu` feature for wasm rendering");
#[cfg(all(not(target_arch = "wasm32"), not(feature = "wgpu"), not(feature = "native")))]
compile_error!("enable the `native` or `wgpu` feature for native rendering");
#[cfg(all(
    not(feature = "wgpu"),
    not(any(
        all(feature = "native", any(target_os = "macos", target_os = "ios", target_os = "windows", target_os = "linux", target_os = "android")),
        all(feature = "web", target_arch = "wasm32")
    ))
))]
compile_error!("no default renderer is available for this target");

#[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
pub type AimerRenderContext = WgpuApi;
#[cfg(all(feature = "wgpu", target_arch = "wasm32"))]
pub type AimerRenderContext = H5CanvasApi;
#[cfg(all(not(feature = "wgpu"), feature = "native", any(target_os = "macos", target_os = "ios")))]
pub type AimerRenderContext = MetalApi;
#[cfg(all(not(feature = "wgpu"), feature = "native", target_os = "windows"))]
pub type AimerRenderContext = Dx12Api;
#[cfg(all(not(feature = "wgpu"), feature = "native", any(target_os = "linux", target_os = "android")))]
pub type AimerRenderContext = NativeAutoApi;
#[cfg(all(not(feature = "wgpu"), feature = "web", target_arch = "wasm32"))]
pub type AimerRenderContext = H5CanvasApi;

/// What happened to a frame the renderer was handed.
///
/// Presentation used to be a `bool`, which only has an answer while it is the
/// caller's own thread doing the work. With the raster thread enabled the frame
/// is merely queued, and the real outcome arrives on the raster thread a frame
/// later — [`Deferred`] is that third state, and it is why the first-frame
/// notification cannot be driven from the return value alone.
///
/// # Examples
///
/// ```
/// use aimer_quiver::render_ctx::PresentOutcome;
///
/// // Only a frame the renderer definitively failed to put on screen is retried.
/// assert!(PresentOutcome::Dropped.needs_retry());
/// assert!(!PresentOutcome::Deferred.needs_retry());
/// assert!(!PresentOutcome::Presented.needs_retry());
/// ```
///
/// [`Deferred`]: PresentOutcome::Deferred
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentOutcome {
    /// The frame reached the screen on this thread, during this call.
    Presented,
    /// The surface texture could not be acquired, so nothing was shown. The
    /// caller should schedule another redraw rather than leave the window
    /// blank.
    Dropped,
    /// The frame was handed to the raster thread. Its outcome is reported later
    /// through the `on_present` callback, which is where a retry or a
    /// first-frame notification has to come from.
    Deferred,
}

#[cfg(test)]
mod backend_selection_tests {
    use std::cell::RefCell;

    use super::{NativeBackendChoice, choose_native_backend};

    #[test]
    fn native_selection_uses_vulkan_when_it_initializes() {
        let calls = RefCell::new(Vec::new());
        let selected = choose_native_backend(
            || {
                calls.borrow_mut().push("vulkan");
                true
            },
            || {
                calls.borrow_mut().push("opengl");
                true
            },
        );

        assert_eq!(selected, Some(NativeBackendChoice::Vulkan));
        assert_eq!(*calls.borrow(), ["vulkan"]);
    }

    #[test]
    fn native_selection_tries_opengl_after_vulkan_failure() {
        let calls = RefCell::new(Vec::new());
        let selected = choose_native_backend(
            || {
                calls.borrow_mut().push("vulkan");
                false
            },
            || {
                calls.borrow_mut().push("opengl");
                true
            },
        );

        assert_eq!(selected, Some(NativeBackendChoice::OpenGl));
        assert_eq!(*calls.borrow(), ["vulkan", "opengl"]);
    }

    #[test]
    fn native_selection_reports_failure_when_both_backends_fail() {
        assert_eq!(
            choose_native_backend(|| false, || false),
            None,
        );
    }
}

impl PresentOutcome {
    /// Whether the frame is known to have reached the screen already.
    ///
    /// [`Deferred`](PresentOutcome::Deferred) is *not* presented: the answer is
    /// simply not known yet.
    #[inline]
    pub fn is_presented(self) -> bool {
        matches!(self, Self::Presented)
    }

    /// Whether the caller has to request another redraw.
    #[inline]
    pub fn needs_retry(self) -> bool {
        matches!(self, Self::Dropped)
    }

    /// Whether the outcome will be reported asynchronously instead.
    #[inline]
    pub fn is_deferred(self) -> bool {
        matches!(self, Self::Deferred)
    }

    /// Interpret a synchronous present result.
    #[inline]
    pub fn from_presented(presented: bool) -> Self {
        if presented {
            Self::Presented
        } else {
            Self::Dropped
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PresentOutcome;

    #[test]
    fn a_successful_present_is_neither_retried_nor_deferred() {
        let outcome = PresentOutcome::from_presented(true);

        assert_eq!(outcome, PresentOutcome::Presented);
        assert!(outcome.is_presented());
        assert!(!outcome.needs_retry());
        assert!(!outcome.is_deferred());
    }

    #[test]
    fn a_dropped_frame_is_retried() {
        let outcome = PresentOutcome::from_presented(false);

        assert_eq!(outcome, PresentOutcome::Dropped);
        assert!(!outcome.is_presented());
        assert!(outcome.needs_retry());
    }

    #[test]
    fn a_deferred_frame_is_not_presented_yet_and_is_not_retried() {
        let outcome = PresentOutcome::Deferred;

        assert!(!outcome.is_presented());
        assert!(!outcome.needs_retry());
        assert!(outcome.is_deferred());
    }
}
