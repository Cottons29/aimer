#[cfg(target_arch = "wasm32")]
mod h5canva;
#[cfg(target_arch = "wasm32")]
pub use h5canva::render_ctx::H5CanvasApi;

#[cfg(not(target_arch = "wasm32"))]
mod wgpu_ctx;
#[cfg(not(target_arch = "wasm32"))]
pub use wgpu_ctx::render_ctx::WgpuApi;

#[cfg(not(target_arch = "wasm32"))]
pub type AimerRenderContext = WgpuApi;
#[cfg(target_arch = "wasm32")]
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
