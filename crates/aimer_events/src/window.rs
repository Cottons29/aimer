use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use winit::window::Window;

static GLOBAL_WINDOW: OnceLock<&'static Window> = OnceLock::new();

/// Whether a frame has been requested since the last display-link tick.
///
/// On iOS the frame loop is driven by a Swift `CADisplayLink` (see
/// `main.swift`). Requesting a frame sets this flag and unpauses the link;
/// each vsync tick consumes it via [`take_frame_requested`]. When a tick finds
/// it cleared, the link is paused again so the app does not render while idle.
static FRAME_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Atomically read-and-clear the pending-frame flag.
///
/// Returns `true` if a frame had been requested since the last call. Used by
/// the iOS display-link tick to decide whether to render this vsync or pause
/// the link.
pub fn take_frame_requested() -> bool {
    FRAME_REQUESTED.swap(false, Ordering::AcqRel)
}

#[cfg(target_os = "ios")]
unsafe extern "C" {
    /// Unpause the Swift `CADisplayLink` so it starts delivering vsync ticks.
    fn aimer_ios_request_frame();
}

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

/// Request the next frame render.
///
/// Prefers the installed event-loop-driven requester (safe to call from within
/// the draw cycle); falls back to `Window::request_redraw()` when none was
/// installed.
///
/// On iOS, `request_redraw()` issued from inside the draw cycle (or from a
/// `user_event` that arrives immediately after) is silently coalesced by the
/// system — the next `RedrawRequested` is never delivered and animations stop
/// after a single step.
///
/// Instead, the frame loop is driven by a Swift `CADisplayLink` synced to the
/// display (up to 120 Hz on ProMotion). Requesting a frame simply raises the
/// [`FRAME_REQUESTED`] flag and unpauses the link; the next vsync tick then
/// delivers a `FrameReady` (see `aimer_ios_frame_tick` in `aimer_quiver`) that
/// routes to `request_redraw()` outside the coalescing window. The link pauses
/// itself once a tick observes no pending request, so the app stays idle when
/// nothing is animating.
pub fn request_animation_frame() {
    #[cfg(target_os = "ios")]
    {
        // Mark a frame as pending and make sure the display link is running so
        // the next vsync delivers it.
        FRAME_REQUESTED.store(true, Ordering::Release);
        unsafe {
            aimer_ios_request_frame();
        }
    }
    #[cfg(not(target_os = "ios"))]
    {
        request_animation_frame_inner();
    }
}

/// Inner implementation — actual redraw request (platform-independent).
#[cfg_attr(target_os = "ios", allow(dead_code))]
fn request_animation_frame_inner() {
    if let Some(requester) = REDRAW_REQUESTER.get() {
        requester();
        return;
    }
    if let Some(window) = GLOBAL_WINDOW.get() {
        window.request_redraw();
    }
}
