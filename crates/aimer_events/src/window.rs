use std::cell::RefCell;
use std::rc::Rc;
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

thread_local! {
    /// A redraw requester that answers for this thread alone.
    ///
    /// The platform requester and the global window are one per process, which
    /// is exactly right for an application that owns the screen. An application
    /// running without a window is not that: several of them can be alive at
    /// once, each with its own frames to schedule, and each on its own thread.
    /// Installing per thread keeps a frame request with the application that
    /// made it.
    static THREAD_REDRAW_REQUESTER: RefCell<Option<Rc<dyn Fn()>>> =
        const { RefCell::new(None) };
    static THREAD_REDRAW_OBSERVER: RefCell<Option<Rc<dyn Fn()>>> =
        const { RefCell::new(None) };
    /// Set while a frame request that scrolling alone caused is being made.
    static SCROLL_FRAME_REQUEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Requests the next frame on behalf of scrolling.
///
/// Identical to [`request_animation_frame`], except that the request says why
/// it was made. A frame that exists only because scroll physics has more
/// distance to cover has nothing to update except what the offset moves, and a
/// frame loop that can tell it apart from a widget's request may do less work
/// for it. Anything else that wants a frame asks with
/// [`request_animation_frame`] and keeps the frame a full one.
#[doc(hidden)]
pub fn request_scroll_frame() {
    let previous = SCROLL_FRAME_REQUEST.with(|flag| flag.replace(true));
    request_animation_frame();
    SCROLL_FRAME_REQUEST.with(|flag| flag.set(previous));
}

/// Whether the frame request being made right now was made by
/// [`request_scroll_frame`]. Meant for the requester that records the reason.
#[doc(hidden)]
pub fn is_scroll_frame_request() -> bool {
    SCROLL_FRAME_REQUEST.with(std::cell::Cell::get)
}

/// Installs an internal observer for direct redraw requests on this thread.
#[doc(hidden)]
pub fn set_thread_redraw_observer<F>(observer: F) -> Option<Rc<dyn Fn()>>
where
    F: Fn() + 'static,
{
    THREAD_REDRAW_OBSERVER.with(|slot| slot.borrow_mut().replace(Rc::new(observer)))
}

/// Restores the observer returned by [`set_thread_redraw_observer`].
#[doc(hidden)]
pub fn restore_thread_redraw_observer(previous: Option<Rc<dyn Fn()>>) {
    THREAD_REDRAW_OBSERVER.with(|slot| *slot.borrow_mut() = previous);
}

/// Notifies the active application's frame scheduler that a window requested a
/// redraw directly, without going through [`request_animation_frame`].
#[doc(hidden)]
pub fn notify_redraw_requested() {
    let observer = THREAD_REDRAW_OBSERVER.with(|slot| slot.borrow().clone());
    if let Some(observer) = observer {
        observer();
    }
}

/// Install a redraw requester for the current thread, replacing any previous
/// one, and hand back the one it replaced.
///
/// Takes precedence over the process-wide requester and the global window, so
/// an application that has no platform window still receives the frame requests
/// its widgets make — a state update, an animation step, an overlay that just
/// opened. Return the previous requester to
/// [`restore_thread_redraw_requester`] once the application is gone, so a
/// dropped application stops receiving them.
///
/// # Examples
///
/// ```
/// use std::cell::Cell;
/// use std::rc::Rc;
///
/// let requests = Rc::new(Cell::new(0));
/// let counted = requests.clone();
/// let previous = aimer_events::window::set_thread_redraw_requester(move || {
///     counted.set(counted.get() + 1);
/// });
///
/// aimer_events::window::request_animation_frame();
/// assert_eq!(requests.get(), 1);
///
/// aimer_events::window::restore_thread_redraw_requester(previous);
/// ```
pub fn set_thread_redraw_requester<F>(requester: F) -> Option<Rc<dyn Fn()>>
where
    F: Fn() + 'static,
{
    THREAD_REDRAW_REQUESTER.with(|slot| slot.borrow_mut().replace(Rc::new(requester)))
}

/// Put back the requester that [`set_thread_redraw_requester`] replaced.
pub fn restore_thread_redraw_requester(previous: Option<Rc<dyn Fn()>>) {
    THREAD_REDRAW_REQUESTER.with(|slot| *slot.borrow_mut() = previous);
}

/// The requester installed for this thread, if any.
#[cfg(not(aimer_portable_guest))]
fn thread_redraw_requester() -> Option<Rc<dyn Fn()>> {
    THREAD_REDRAW_REQUESTER.with(|slot| slot.borrow().clone())
}

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
#[cfg(aimer_portable_guest)]
pub fn request_animation_frame() {
    // Portable guests do not own a browser or native event loop. The reload
    // host polls the guest scheduler at its safe point, so forwarding this
    // request into winit would pull the browser event-loop ABI into the
    // capability-only module.
}

#[cfg(not(aimer_portable_guest))]
pub fn request_animation_frame() {
    // An application without a platform window schedules its own frames, and
    // says so per thread, so its request never reaches the display link or a
    // window belonging to somebody else.
    if let Some(requester) = thread_redraw_requester() {
        requester();
        return;
    }

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
#[cfg(not(aimer_portable_guest))]
fn request_animation_frame_inner() {
    if let Some(requester) = REDRAW_REQUESTER.get() {
        requester();
        return;
    }
    if let Some(window) = GLOBAL_WINDOW.get() {
        window.request_redraw();
    }
}

#[cfg(all(test, not(aimer_portable_guest)))]
mod scroll_request_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    /// Runs `request` with a requester that records whether each request it
    /// received was a scroll request.
    fn observed(request: impl FnOnce()) -> Vec<bool> {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let previous = set_thread_redraw_requester({
            let seen = seen.clone();
            move || seen.borrow_mut().push(is_scroll_frame_request())
        });
        request();
        restore_thread_redraw_requester(previous);
        seen.take()
    }

    #[test]
    fn a_scroll_request_says_it_is_one_and_an_ordinary_request_does_not() {
        assert_eq!(observed(request_scroll_frame), [true]);
        assert_eq!(observed(request_animation_frame), [false]);
        assert!(!is_scroll_frame_request());
    }

    #[test]
    fn the_mark_does_not_outlive_the_request() {
        let seen = observed(|| {
            request_scroll_frame();
            request_animation_frame();
            request_scroll_frame();
        });
        assert_eq!(seen, [true, false, true]);
        assert!(!is_scroll_frame_request());
    }

    #[test]
    fn nested_scroll_requests_restore_the_outer_mark() {
        let seen = observed(|| {
            let previous = SCROLL_FRAME_REQUEST.with(|flag| flag.replace(true));
            request_scroll_frame();
            assert!(is_scroll_frame_request());
            SCROLL_FRAME_REQUEST.with(|flag| flag.set(previous));
        });
        assert_eq!(seen, [true]);
        assert!(!is_scroll_frame_request());
    }
}
