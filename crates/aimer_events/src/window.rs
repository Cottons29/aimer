use std::sync::OnceLock;
use winit::window::Window;

static GLOBAL_WINDOW: OnceLock<&'static Window> = OnceLock::new();

type RedrawRequester = Box<dyn Fn() + Send + Sync + 'static>;

/// Optional, platform-supplied redraw requester. When installed it is used to
/// schedule the next frame through the event loop (e.g. an `EventLoopProxy`)
/// instead of calling `Window::request_redraw()` directly.
static REDRAW_REQUESTER: OnceLock<RedrawRequester> = OnceLock::new();

/// Store the application window reference so other crates can access it.
pub fn set_window(window: &'static Window) {
    let _ = GLOBAL_WINDOW.set(window);
}

/// Retrieve the application window reference, if it has been set.
pub fn get_window() -> Option<&'static Window> {
    GLOBAL_WINDOW.get().copied()
}

/// Install a platform redraw requester.
///
/// On some platforms (notably iOS) calling `Window::request_redraw()`
/// synchronously from inside the draw cycle is coalesced and does not schedule
/// the next frame. Routing the request through the event loop (via an
/// `EventLoopProxy`) delivers it after the current frame completes without
/// spawning a thread. The application installs that closure here.
pub fn set_redraw_requester<F>(requester: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let _ = REDRAW_REQUESTER.set(Box::new(requester));
}

/// Request the next animation frame.
///
/// Prefers the installed event-loop-driven requester (safe to call from within
/// the draw cycle); falls back to `Window::request_redraw()` when none was
/// installed.
///
/// On iOS, `request_redraw()` issued from inside the draw cycle (or from a
/// `user_event` that arrives immediately after) is silently coalesced by the
/// system — the next `RedrawRequested` is never delivered and animations stop
/// after a single step.  Routing through the `EventLoopProxy` (`FrameReady`)
/// does not reliably avoid this because iOS can still coalesce the resulting
/// `request_redraw()` when it arrives too close to the previous frame.
///
/// The reliable workaround: **spawn a short-lived thread** that sleeps 1 ms
/// (yielding to the UIKit run loop) then calls this function again.  The 1 ms
/// gap pushes the `request_redraw()` outside the coalescing window so the next
/// frame is genuinely scheduled.  The thread is cheap (reused by the OS thread
/// pool) and only lives for 1 ms.
pub fn request_animation_frame() {
    #[cfg(target_os = "ios")]
    {
        // Push the redraw request 1 ms into the future so iOS does not
        // coalesce it with the current frame.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(1));
            request_animation_frame_inner();
        });
    }
    #[cfg(not(target_os = "ios"))]
    {
        request_animation_frame_inner();
    }
}

/// Inner implementation — actual redraw request (platform-independent).
fn request_animation_frame_inner() {
    if let Some(requester) = REDRAW_REQUESTER.get() {
        requester();
        return;
    }
    if let Some(window) = GLOBAL_WINDOW.get() {
        window.request_redraw();
    }
}
