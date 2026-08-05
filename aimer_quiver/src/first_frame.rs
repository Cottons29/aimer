use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// Browser event dispatched on `window` after the first successful frame
/// presentation.
pub const FIRST_FRAME_RENDERED_EVENT: &str = "aimer:first-frame-rendered";

type FirstFrameCallback = Box<dyn FnOnce() + Send + 'static>;

static FIRST_FRAME_CALLBACK: LazyLock<Mutex<Option<FirstFrameCallback>>> =
    LazyLock::new(|| Mutex::new(None));

/// Registers a system callback that runs after the first frame is successfully
/// presented. Use it to dismiss a native loading or splash screen, and register
/// it before calling [`crate::AimerApp::start`].
///
/// A later registration replaces an earlier callback that has not run yet.
///
/// # Threading
///
/// The callback runs on whichever thread presented the first frame. That is the
/// UI thread by default, but the raster thread when the `raster-thread` feature
/// is enabled — hence the `Send` bound. Do not touch platform UI objects from it
/// directly; hop to the main thread first.
pub fn set_first_frame_rendered_callback(callback: impl FnOnce() + Send + 'static) {
    *FIRST_FRAME_CALLBACK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(callback));
}

pub(crate) fn dispatch_first_frame_rendered() {
    #[cfg(target_arch = "wasm32")]
    dispatch_browser_event();

    let callback = FIRST_FRAME_CALLBACK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    if let Some(callback) = callback {
        callback();
    }
}

#[cfg(target_arch = "wasm32")]
fn dispatch_browser_event() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(event) = web_sys::Event::new(FIRST_FRAME_RENDERED_EVENT) else {
        return;
    };
    if let Err(error) = window.dispatch_event(&event) {
        aimer_utils::error!("Failed to dispatch {FIRST_FRAME_RENDERED_EVENT}: {error:?}");
    }
}

/// Fires the first-frame notification exactly once, from whichever thread
/// observes the first successful presentation.
///
/// Presentation is not necessarily synchronous: with a raster thread the
/// outcome of a frame arrives on that thread, one frame after it was submitted.
/// The notifier is therefore shared state guarded by an atomic rather than a
/// `bool` field on the UI-thread handler, so both the inline path and the raster
/// thread's `on_present` callback can drive it without racing.
#[derive(Debug, Default)]
pub(crate) struct FirstFrameNotifier {
    notified: AtomicBool,
}

impl FirstFrameNotifier {
    /// Run `dispatch` if `presented` is the first successful presentation.
    ///
    /// A failed present is not a first frame — nothing reached the screen — and
    /// every presentation after the first is ignored. The claim is atomic, so
    /// two threads reporting a successful frame at the same time still dispatch
    /// once.
    pub(crate) fn notify_after_present(&self, presented: bool, dispatch: impl FnOnce()) {
        if !presented {
            return;
        }
        if self
            .notified
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            dispatch();
        }
    }
}

static FIRST_FRAME: FirstFrameNotifier = FirstFrameNotifier {
    notified: AtomicBool::new(false),
};

/// Report the outcome of a presented frame to the first-frame notification.
///
/// Called from the UI thread when the frame was presented inline, and from the
/// raster thread's `on_present` callback when it was not.
pub(crate) fn notify_first_frame_presented(presented: bool) {
    FIRST_FRAME.notify_after_present(presented, dispatch_first_frame_rendered);
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    use super::*;

    #[test]
    fn failed_present_does_not_dispatch() {
        let dispatches = Cell::new(0);
        let notifier = FirstFrameNotifier::default();

        notifier.notify_after_present(false, || dispatches.set(dispatches.get() + 1));

        assert_eq!(dispatches.get(), 0);
    }

    #[test]
    fn first_successful_present_dispatches_once() {
        let dispatches = Cell::new(0);
        let notifier = FirstFrameNotifier::default();

        notifier.notify_after_present(false, || dispatches.set(dispatches.get() + 1));
        notifier.notify_after_present(true, || dispatches.set(dispatches.get() + 1));
        notifier.notify_after_present(true, || dispatches.set(dispatches.get() + 1));

        assert_eq!(dispatches.get(), 1);
    }

    #[test]
    fn concurrent_presents_still_dispatch_once() {
        let notifier = FirstFrameNotifier::default();
        let dispatches = AtomicUsize::new(0);

        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    notifier.notify_after_present(true, || {
                        dispatches.fetch_add(1, Ordering::SeqCst);
                    });
                });
            }
        });

        assert_eq!(dispatches.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn registered_callback_is_dispatched() {
        let dispatches = Arc::new(AtomicUsize::new(0));
        let callback_dispatches = dispatches.clone();
        set_first_frame_rendered_callback(move || {
            callback_dispatches.fetch_add(1, Ordering::SeqCst);
        });

        dispatch_first_frame_rendered();
        dispatch_first_frame_rendered();

        assert_eq!(dispatches.load(Ordering::SeqCst), 1);
    }
}
