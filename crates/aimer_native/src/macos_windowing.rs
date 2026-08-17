use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

use winit::window::{Window, WindowAttributes};

type WindowCreated = Box<dyn for<'window> FnOnce(MacosWindowHandle<'window>)>;

#[derive(Clone, Copy, Debug, PartialEq)]
struct TrafficLightPosition {
    x: f64,
    y: f64,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct TrafficLightLayoutState {
    deferred_reapplication: bool,
}

#[cfg(any(target_os = "macos", test))]
impl TrafficLightLayoutState {
    #[inline]
    fn native_layout_changed(&mut self) {
        self.deferred_reapplication = true;
    }

    #[inline]
    fn take_deferred_reapplication(&mut self) -> bool {
        std::mem::take(&mut self.deferred_reapplication)
    }
}

#[cfg(any(target_os = "macos", test))]
struct TrafficLightLayout {
    titlebar_height: f64,
    titlebar_y: f64,
    button_origins: [(f64, f64); 3],
}

#[cfg(any(target_os = "macos", test))]
fn traffic_light_layout(
    position: TrafficLightPosition,
    window_height: f64,
    button_height: f64,
    native_button_x: [f64; 3],
) -> TrafficLightLayout {
    let minimize_offset = native_button_x[1] - native_button_x[0];
    let zoom_offset = native_button_x[2] - native_button_x[0];
    let titlebar_height = button_height + position.y.max(0.0) * 2.0;
    let button_y = titlebar_height - button_height - position.y;

    TrafficLightLayout {
        titlebar_height,
        titlebar_y: window_height - titlebar_height,
        button_origins: [
            (position.x, button_y),
            (position.x + minimize_offset, button_y),
            (position.x + zoom_offset, button_y),
        ],
    }
}

/// Declarative macOS window customization installed during Aimer startup.
///
/// Build the desired configuration and call [`Self::install`] from
/// [`AimerApp::setup`](https://docs.rs/aimer/latest/aimer/struct.AimerApp.html#method.setup).
/// Aimer applies creation-time attributes before creating its first window and
/// then invokes the optional [`Self::on_created`] callback with the underlying
/// `NSWindow` pointer.
///
/// The type and all of its builder methods are available on every platform. On
/// platforms other than macOS, installing it has no effect, so application code
/// does not need conditional compilation around common window configuration.
///
/// # Examples
///
/// ```no_run
/// use aimer_native::macos_windowing::MacosWindowing;
///
/// let windowing = MacosWindowing::new()
///     .titlebar_transparent(true)
///     .title_hidden(true)
///     .fullsize_content_view(true)
///     .movable_by_window_background(true)
///     .traffic_light_position(16.0, 14.0);
///
/// windowing.install();
/// ```
pub struct MacosWindowing {
    movable_by_window_background: Option<bool>,
    titlebar_transparent: Option<bool>,
    title_hidden: Option<bool>,
    titlebar_hidden: Option<bool>,
    titlebar_buttons_hidden: Option<bool>,
    fullsize_content_view: Option<bool>,
    has_shadow: Option<bool>,
    accepts_first_mouse: Option<bool>,
    traffic_light_position: Option<TrafficLightPosition>,
    #[cfg(any(target_os = "macos", test))]
    traffic_light_layout_state: TrafficLightLayoutState,
    on_created: Option<WindowCreated>,
}

impl MacosWindowing {
    /// Creates a configuration that leaves every `winit` default unchanged.
    #[inline]
    pub fn new() -> Self {
        Self {
            movable_by_window_background: None,
            titlebar_transparent: None,
            title_hidden: None,
            titlebar_hidden: None,
            titlebar_buttons_hidden: None,
            fullsize_content_view: None,
            has_shadow: None,
            accepts_first_mouse: None,
            traffic_light_position: None,
            #[cfg(any(target_os = "macos", test))]
            traffic_light_layout_state: TrafficLightLayoutState::default(),
            on_created: None,
        }
    }

    /// Controls whether dragging the window background moves the window.
    #[inline]
    pub fn movable_by_window_background(mut self, movable: bool) -> Self {
        self.movable_by_window_background = Some(movable);
        self
    }

    /// Controls whether the titlebar draws a background.
    ///
    /// This is commonly combined with [`Self::fullsize_content_view`] so the
    /// application's content is visible behind the titlebar controls.
    #[inline]
    pub fn titlebar_transparent(mut self, transparent: bool) -> Self {
        self.titlebar_transparent = Some(transparent);
        self
    }

    /// Controls whether the text in the titlebar is hidden.
    #[inline]
    pub fn title_hidden(mut self, hidden: bool) -> Self {
        self.title_hidden = Some(hidden);
        self
    }

    /// Controls whether the complete native titlebar is hidden.
    #[inline]
    pub fn titlebar_hidden(mut self, hidden: bool) -> Self {
        self.titlebar_hidden = Some(hidden);
        self
    }

    /// Controls whether the close, minimize, and zoom buttons are hidden.
    #[inline]
    pub fn titlebar_buttons_hidden(mut self, hidden: bool) -> Self {
        self.titlebar_buttons_hidden = Some(hidden);
        self
    }

    /// Controls whether content occupies the full window area behind the titlebar.
    #[inline]
    pub fn fullsize_content_view(mut self, fullsize: bool) -> Self {
        self.fullsize_content_view = Some(fullsize);
        self
    }

    /// Controls whether AppKit draws the standard window shadow.
    #[inline]
    pub fn has_shadow(mut self, has_shadow: bool) -> Self {
        self.has_shadow = Some(has_shadow);
        self
    }

    /// Controls whether the window accepts the first click while inactive.
    #[inline]
    pub fn accepts_first_mouse(mut self, accepts: bool) -> Self {
        self.accepts_first_mouse = Some(accepts);
        self
    }

    /// Positions the standard close, minimize, and zoom buttons on macOS.
    ///
    /// `x` and `y` are logical AppKit points from the top-left corner of the
    /// window to the top-left corner of the close button. The minimize and zoom
    /// buttons keep their native spacing from the close button. Aimer reapplies
    /// the position when AppKit lays out the window again during resizing or a
    /// fullscreen transition.
    ///
    /// This setting has no effect outside macOS. Both coordinates must be
    /// finite; negative coordinates are supported when deliberately placing
    /// the controls partly outside the window.
    ///
    /// # Panics
    ///
    /// Panics if either coordinate is NaN or infinite.
    #[inline]
    pub fn traffic_light_position(mut self, x: f64, y: f64) -> Self {
        assert!(
            x.is_finite() && y.is_finite(),
            "traffic-light coordinates must be finite"
        );
        self.traffic_light_position = Some(TrafficLightPosition { x, y });
        self
    }

    /// Registers advanced one-time customization for the created `NSWindow`.
    ///
    /// The callback runs on Aimer's event-loop thread immediately after window
    /// creation and before rendering begins. It is never invoked off macOS.
    /// The handle is borrowed for the callback and cannot be sent to another
    /// thread or retained beyond the call.
    #[inline]
    pub fn on_created(
        mut self,
        callback: impl for<'window> FnOnce(MacosWindowHandle<'window>) + 'static,
    ) -> Self {
        self.on_created = Some(Box::new(callback));
        self
    }

    /// Installs this configuration for Aimer's next macOS window.
    ///
    /// Call this from `AimerApp::setup`, which runs on the native event-loop
    /// thread before the window is created. If several setup callbacks install
    /// configurations, the last installed value is used.
    #[cfg(target_os = "macos")]
    #[inline]
    pub fn install(self) {
        PENDING.with(|pending| pending.replace(Some(self)));
    }

    /// Does nothing on platforms that do not create AppKit windows.
    #[cfg(not(target_os = "macos"))]
    #[inline]
    pub fn install(self) {}

    #[doc(hidden)]
    #[cfg(target_os = "macos")]
    pub fn apply_attributes(&self, mut attributes: WindowAttributes) -> WindowAttributes {
        use winit::platform::macos::WindowAttributesExtMacOS;

        if let Some(value) = self.movable_by_window_background {
            attributes = attributes.with_movable_by_window_background(value);
        }
        if let Some(value) = self.titlebar_transparent {
            attributes = attributes.with_titlebar_transparent(value);
        }
        if let Some(value) = self.title_hidden {
            attributes = attributes.with_title_hidden(value);
        }
        if let Some(value) = self.titlebar_hidden {
            attributes = attributes.with_titlebar_hidden(value);
        }
        if let Some(value) = self.titlebar_buttons_hidden {
            attributes = attributes.with_titlebar_buttons_hidden(value);
        }
        if let Some(value) = self.fullsize_content_view {
            attributes = attributes.with_fullsize_content_view(value);
        }
        if let Some(value) = self.has_shadow {
            attributes = attributes.with_has_shadow(value);
        }
        if let Some(value) = self.accepts_first_mouse {
            attributes = attributes.with_accepts_first_mouse(value);
        }
        attributes
    }

    #[doc(hidden)]
    #[cfg(not(target_os = "macos"))]
    pub fn apply_attributes(&self, attributes: WindowAttributes) -> WindowAttributes {
        attributes
    }

    #[doc(hidden)]
    #[cfg(target_os = "macos")]
    pub fn window_created(&mut self, window: &Window) {
        if self.traffic_light_position.is_none() && self.on_created.is_none() {
            return;
        }
        let Some(native_window) = native_window(window) else {
            return;
        };

        if let Some(position) = self.traffic_light_position {
            apply_traffic_light_position(&native_window, position);
        }
        let Some(callback) = self.on_created.take() else {
            return;
        };
        let pointer = NonNull::from(&*native_window).cast::<c_void>();

        callback(MacosWindowHandle {
            pointer,
            _lifetime: PhantomData,
            _main_thread: PhantomData,
        });
    }

    #[doc(hidden)]
    #[cfg(not(target_os = "macos"))]
    pub fn window_created(&mut self, _window: &Window) {}

    #[doc(hidden)]
    #[cfg(target_os = "macos")]
    pub fn window_layout_changed(&mut self, window: &Window) {
        let Some(position) = self.traffic_light_position else {
            return;
        };
        self.traffic_light_layout_state.native_layout_changed();
        let Some(native_window) = native_window(window) else {
            return;
        };

        apply_traffic_light_position(&native_window, position);
    }

    #[doc(hidden)]
    #[cfg(not(target_os = "macos"))]
    pub fn window_layout_changed(&mut self, _window: &Window) {}

    #[doc(hidden)]
    #[cfg(target_os = "macos")]
    pub fn window_redraw_requested(&mut self, window: &Window) {
        if !self
            .traffic_light_layout_state
            .take_deferred_reapplication()
        {
            return;
        }
        let Some(position) = self.traffic_light_position else {
            return;
        };
        let Some(native_window) = native_window(window) else {
            return;
        };

        apply_traffic_light_position(&native_window, position);
    }

    #[doc(hidden)]
    #[cfg(not(target_os = "macos"))]
    pub fn window_redraw_requested(&mut self, _window: &Window) {}
}

#[cfg(target_os = "macos")]
fn native_window(window: &Window) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    use objc2::rc::Retained;
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };

    // SAFETY: `window_handle` guarantees that AppKit's `ns_view` pointer
    // remains valid for the lifetime of the borrowed winit window. Retaining
    // the view keeps it alive while its owning NSWindow is being queried.
    let view = unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }?;
    view.window()
}

#[cfg(target_os = "macos")]
fn apply_traffic_light_position(
    window: &objc2_app_kit::NSWindow,
    position: TrafficLightPosition,
) {
    use objc2_app_kit::NSWindowButton;
    use objc2_foundation::NSPoint;

    let Some(close) = window.standardWindowButton(NSWindowButton::CloseButton) else {
        return;
    };
    let Some(minimize) = window.standardWindowButton(NSWindowButton::MiniaturizeButton) else {
        return;
    };
    let Some(zoom) = window.standardWindowButton(NSWindowButton::ZoomButton) else {
        return;
    };

    // SAFETY: AppKit owns the titlebar hierarchy. The retained views returned
    // here remain valid for this main-thread layout operation.
    let Some(buttons_view) = (unsafe { close.superview() }) else {
        return;
    };
    // SAFETY: Same ownership invariant as above; this is the titlebar container
    // that AppKit may recreate or resize during a native window relayout.
    let Some(titlebar_view) = (unsafe { buttons_view.superview() }) else {
        return;
    };

    let close_frame = close.frame();
    let minimize_frame = minimize.frame();
    let zoom_frame = zoom.frame();
    let layout = traffic_light_layout(
        position,
        window.frame().size.height,
        close_frame.size.height,
        [
            close_frame.origin.x,
            minimize_frame.origin.x,
            zoom_frame.origin.x,
        ],
    );

    let mut titlebar_frame = titlebar_view.frame();
    titlebar_frame.size.height = layout.titlebar_height;
    titlebar_frame.origin.y = layout.titlebar_y;
    titlebar_view.setFrame(titlebar_frame);

    close.setFrameOrigin(NSPoint {
        x: layout.button_origins[0].0,
        y: layout.button_origins[0].1,
    });
    minimize.setFrameOrigin(NSPoint {
        x: layout.button_origins[1].0,
        y: layout.button_origins[1].1,
    });
    zoom.setFrameOrigin(NSPoint {
        x: layout.button_origins[2].0,
        y: layout.button_origins[2].1,
    });
}

impl Default for MacosWindowing {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// A main-thread-only borrowed pointer to the AppKit `NSWindow`.
///
/// The wrapper deliberately exposes only a raw pointer: typed AppKit access is
/// an advanced operation whose safety requirements depend on the Objective-C
/// messages the caller sends. The pointer is non-null and valid only for the
/// duration of the callback passed to [`MacosWindowing::on_created`].
#[derive(Clone, Copy)]
pub struct MacosWindowHandle<'window> {
    pointer: NonNull<c_void>,
    _lifetime: PhantomData<&'window Window>,
    _main_thread: PhantomData<Rc<()>>,
}

impl MacosWindowHandle<'_> {
    /// Returns the borrowed `NSWindow` pointer.
    ///
    /// The caller must not release the object, transfer it to another thread,
    /// or retain the pointer after the callback returns.
    #[inline]
    pub fn as_ptr(self) -> *mut c_void {
        self.pointer.as_ptr()
    }

    /// Returns the non-null borrowed `NSWindow` pointer.
    #[inline]
    pub fn as_non_null(self) -> NonNull<c_void> {
        self.pointer
    }
}

#[cfg(target_os = "macos")]
thread_local! {
    static PENDING: std::cell::RefCell<Option<MacosWindowing>> = const {
        std::cell::RefCell::new(None)
    };
}

#[doc(hidden)]
#[cfg(target_os = "macos")]
pub fn take_pending() -> MacosWindowing {
    PENDING.with(|pending| pending.take().unwrap_or_default())
}

#[doc(hidden)]
#[cfg(not(target_os = "macos"))]
pub fn take_pending() -> MacosWindowing {
    MacosWindowing::new()
}

#[cfg(test)]
mod tests {
    use super::MacosWindowing;

    #[test]
    fn macos_windowing_defaults_do_not_override_winit() {
        let windowing = MacosWindowing::new();

        assert_eq!(windowing.movable_by_window_background, None);
        assert_eq!(windowing.titlebar_transparent, None);
        assert_eq!(windowing.title_hidden, None);
        assert_eq!(windowing.titlebar_hidden, None);
        assert_eq!(windowing.titlebar_buttons_hidden, None);
        assert_eq!(windowing.fullsize_content_view, None);
        assert_eq!(windowing.has_shadow, None);
        assert_eq!(windowing.accepts_first_mouse, None);
        assert_eq!(windowing.traffic_light_position, None);
        assert!(windowing.on_created.is_none());
    }

    #[test]
    fn macos_windowing_builders_preserve_every_customization() {
        let windowing = MacosWindowing::new()
            .movable_by_window_background(true)
            .titlebar_transparent(true)
            .title_hidden(true)
            .titlebar_hidden(false)
            .titlebar_buttons_hidden(true)
            .fullsize_content_view(true)
            .has_shadow(false)
            .accepts_first_mouse(true)
            .traffic_light_position(16.0, 14.0)
            .on_created(|_| {});

        assert_eq!(windowing.movable_by_window_background, Some(true));
        assert_eq!(windowing.titlebar_transparent, Some(true));
        assert_eq!(windowing.title_hidden, Some(true));
        assert_eq!(windowing.titlebar_hidden, Some(false));
        assert_eq!(windowing.titlebar_buttons_hidden, Some(true));
        assert_eq!(windowing.fullsize_content_view, Some(true));
        assert_eq!(windowing.has_shadow, Some(false));
        assert_eq!(windowing.accepts_first_mouse, Some(true));
        assert_eq!(
            windowing.traffic_light_position,
            Some(super::TrafficLightPosition { x: 16.0, y: 14.0 })
        );
        assert!(windowing.on_created.is_some());
    }

    #[test]
    fn traffic_light_position_accepts_the_window_origin() {
        let windowing = MacosWindowing::new().traffic_light_position(0.0, 0.0);

        assert_eq!(
            windowing.traffic_light_position,
            Some(super::TrafficLightPosition { x: 0.0, y: 0.0 })
        );
    }

    #[test]
    fn traffic_light_layout_uses_top_left_coordinates_and_native_spacing() {
        let layout = super::traffic_light_layout(
            super::TrafficLightPosition { x: 16.0, y: 14.0 },
            780.0,
            14.0,
            [7.0, 27.0, 47.0],
        );

        assert_eq!(layout.titlebar_height, 42.0);
        assert_eq!(layout.titlebar_y, 738.0);
        assert_eq!(
            layout.button_origins,
            [(16.0, 14.0), (36.0, 14.0), (56.0, 14.0)]
        );
    }

    #[test]
    fn traffic_light_layout_supports_zero_insets() {
        let layout = super::traffic_light_layout(
            super::TrafficLightPosition { x: 0.0, y: 0.0 },
            780.0,
            14.0,
            [7.0, 27.0, 47.0],
        );

        assert_eq!(layout.titlebar_height, 14.0);
        assert_eq!(layout.titlebar_y, 766.0);
        assert_eq!(
            layout.button_origins,
            [(0.0, 0.0), (20.0, 0.0), (40.0, 0.0)]
        );
    }

    #[test]
    fn traffic_light_layout_supports_negative_offsets_without_invalid_geometry() {
        let layout = super::traffic_light_layout(
            super::TrafficLightPosition { x: -4.0, y: -10.0 },
            780.0,
            14.0,
            [7.0, 27.0, 47.0],
        );

        assert_eq!(layout.titlebar_height, 14.0);
        assert_eq!(layout.titlebar_y, 766.0);
        assert_eq!(
            layout.button_origins,
            [(-4.0, 10.0), (16.0, 10.0), (36.0, 10.0)]
        );
    }

    #[test]
    fn native_relayout_schedules_exactly_one_deferred_reapplication() {
        let mut state = super::TrafficLightLayoutState::default();

        assert!(!state.take_deferred_reapplication());
        state.native_layout_changed();
        assert!(state.take_deferred_reapplication());
        assert!(!state.take_deferred_reapplication());
    }

    #[test]
    #[should_panic(expected = "traffic-light coordinates must be finite")]
    fn traffic_light_position_rejects_a_non_finite_horizontal_coordinate() {
        let _ = MacosWindowing::new().traffic_light_position(f64::NAN, 14.0);
    }

    #[test]
    #[should_panic(expected = "traffic-light coordinates must be finite")]
    fn traffic_light_position_rejects_a_non_finite_vertical_coordinate() {
        let _ = MacosWindowing::new().traffic_light_position(16.0, f64::INFINITY);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installing_macos_windowing_makes_it_pending_once() {
        drop(super::take_pending());
        MacosWindowing::new().title_hidden(true).install();

        let installed = super::take_pending();
        assert_eq!(installed.title_hidden, Some(true));
        assert_eq!(super::take_pending().title_hidden, None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn installing_macos_windowing_is_a_no_op_on_other_platforms() {
        MacosWindowing::new()
            .titlebar_transparent(true)
            .traffic_light_position(16.0, 14.0)
            .on_created(|_| panic!("a macOS callback must not run on another platform"))
            .install();

        let pending = super::take_pending();
        assert_eq!(pending.titlebar_transparent, None);
        assert_eq!(pending.traffic_light_position, None);
        assert!(pending.on_created.is_none());
    }
}