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

#[derive(Default)]
pub(crate) struct FirstFrameNotifier {
    notified: bool,
}

impl FirstFrameNotifier {
    pub(crate) fn notify_after_present(&mut self, presented: bool, dispatch: impl FnOnce()) {
        if presented && !self.notified {
            self.notified = true;
            dispatch();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        FirstFrameNotifier, dispatch_first_frame_rendered, set_first_frame_rendered_callback,
    };

    #[test]
    fn failed_present_does_not_dispatch() {
        let dispatches = Cell::new(0);
        let mut notifier = FirstFrameNotifier::default();

        notifier.notify_after_present(false, || dispatches.set(dispatches.get() + 1));

        assert_eq!(dispatches.get(), 0);
    }

    #[test]
    fn first_successful_present_dispatches_once() {
        let dispatches = Cell::new(0);
        let mut notifier = FirstFrameNotifier::default();

        notifier.notify_after_present(false, || dispatches.set(dispatches.get() + 1));
        notifier.notify_after_present(true, || dispatches.set(dispatches.get() + 1));
        notifier.notify_after_present(true, || dispatches.set(dispatches.get() + 1));

        assert_eq!(dispatches.get(), 1);
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
