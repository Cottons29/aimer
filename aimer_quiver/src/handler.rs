pub mod event_handler;
/// The file drag the window is currently under.
pub(crate) mod file_drag;
pub mod scroll_classifier;
pub mod scroll_utils;
pub(crate) mod user_events;
/// Gesture segmentation for the phase-less browser wheel stream.
///
/// Compiled for the web target only; the `test` predicate keeps the logic
/// unit-testable from a host build, where no browser exists.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) mod web_scroll_phase;

#[cfg(target_os = "android")]
use crate::aimer_app::ANDROID_APP;
use crate::aimer_app::AimerNativePlatformEvent;
#[cfg(target_os = "android")]
use crate::ffi_utils::android_screen;
#[allow(unused)]
use crate::handler;
use crate::handler::event_handler::WindowEventHandler;
use crate::handler::file_drag::FileDrag;
use crate::handler::scroll_classifier::DualScroller;
use crate::handler::user_events::handle_user_event;
use crate::render_ctx::AimerRenderContext;
use crate::window_attr::WindowAttr;
use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_inspector::InspectorOverlay;
use aimer_venus::Venus;
use aimer_widget::base::{BuildContext, WindowHandle};
use aimer_widget::{AnyElement, EventDispatcher, EventResult, Widget};
use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use winit::application::ApplicationHandler;
#[allow(unused)]
use winit::dpi::{LogicalSize, PhysicalSize, Position};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
#[allow(unused)]
use winit::monitor::MonitorHandle;
#[allow(unused)]
use winit::window::{self, Fullscreen, Window, WindowAttributes, WindowId};
#[cfg(target_os = "android")]
use aimer_utils::debug;

pub(crate) type StartupHook = Box<dyn FnOnce() -> Box<dyn Any>>;

/// Walk the snapshot tree and find a node matching the hovered widget by name
/// and bounds.
#[cfg(debug_assertions)]
fn find_hovered_node(
    node: &aimer_inspector::WidgetNode,
    name: &str,
    start: Vec2d,
    end: Vec2d,
) -> Option<u64> {
    const EPS: f32 = 1.0;
    let w = end.x - start.x;
    let h = end.y - start.y;
    if node.name == name
        && (node.x - start.x).abs() < EPS
        && (node.y - start.y).abs() < EPS
        && (node.width - w).abs() < EPS
        && (node.height - h).abs() < EPS
    {
        return Some(node.id);
    }
    for child in &node.children {
        if let Some(id) = find_hovered_node(child, name, start, end) {
            return Some(id);
        }
    }
    None
}

pub struct AimerApplicationHandler<W: Widget + 'static> {
    /// The window this application draws into and asks for frames.
    ///
    /// A [`WindowHandle`] rather than a `Window`, because a headless
    /// application has no platform window and still has to answer the very
    /// questions the event handlers ask of one: repaint, change the cursor,
    /// report its metrics. Keeping the handle here is what lets both drivers
    /// run the same event and frame code.
    pub window: Option<WindowHandle>,
    pub window_attr: WindowAttr,
    pub(crate) macos_windowing: aimer_native::macos_windowing::MacosWindowing,
    pub render_ctx: AimerRenderContext,
    pub widget_root: Option<AnyElement>,
    pub event_dispatcher: EventDispatcher,
    pub(crate) scroll_smoother: DualScroller,
    /// Gesture boundaries inferred for the phase-less browser wheel stream.
    #[cfg(target_arch = "wasm32")]
    pub(crate) web_scroll_phase: crate::handler::web_scroll_phase::WebScrollPhase,
    pub pending_widget: Option<W>,
    pub cursor_pos: Vec2d,
    /// The mouse button currently held, if any.
    ///
    /// The platform reports a button only when it changes state, but a move or a
    /// release during a drag has to carry the button that started the drag —
    /// otherwise every move looks like a primary-button move and a
    /// secondary-button drag loses its identity halfway through.
    pub pressed_button: Option<aimer_events::pointer::PointerButton>,
    pub current_modifiers: aimer_events::element::Modifiers,
    pub ime_composing: bool,
    pub window_scale: f64,
    pub native_window_size: Option<ResolvedSize>,
    pub pending_resize: Option<PhysicalSize<u32>>,
    pub(crate) startup_hooks: Vec<StartupHook>,
    pub(crate) startup_resources: Vec<Box<dyn Any>>,
    pub start_up_frames: Cell<u8>,
    pub active_touch_id: Option<u64>,
    /// The UI-thread runtime this application's frames are scheduled by.
    ///
    /// Held here rather than reached for through [`Venus::current`] because a
    /// frame drives the runtime it owns: a second window on a second thread is
    /// a second application, and it must drive its own.
    pub venus: Rc<Venus>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Runtime,
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub inspector: Option<aimer_inspector::InspectorAppHandle>,
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    pub inspector: Option<aimer_inspector::InspectorHandle>,
    #[cfg(debug_assertions)]
    pub inspector_change: Cell<bool>,
    #[cfg(debug_assertions)]
    pub inspector_prev_enabled: Cell<bool>,
    #[cfg(debug_assertions)]
    pub inspector_redraw_frames: Cell<u8>,
    /// The batch of files being dragged over the window, so the drag can be
    /// re-reported for every position it is found at rather than only for the
    /// one it came in at.
    pub(crate) file_drag: FileDrag,
}

impl<W: Widget + 'static> AimerApplicationHandler<W> {
    /// The platform window behind this application, if it has one.
    ///
    /// Only code that talks to the platform itself — surface creation, native
    /// appearance queries, AppKit drag polling — needs this; everything else
    /// goes through [`window`](Self::window) and works headlessly too.
    #[inline]
    pub(crate) fn native_window(&self) -> Option<&'static Window> {
        self.window.as_ref().and_then(WindowHandle::native_window)
    }

    /// Tells a headless window the metrics the platform would already have
    /// given a real one.
    ///
    /// `size` is the size the window has just become, or `None` to keep the
    /// one it has and refresh the scale alone. A native window answers for
    /// itself and is left untouched.
    pub(crate) fn sync_headless_metrics(&self, size: Option<PhysicalSize<u32>>) {
        let Some(window) = &self.window else { return };
        let size = size.unwrap_or_else(|| window.inner_size());
        window.update_headless_metrics(size, self.window_scale);
    }

    /// Asks for the frame that continues an animation.
    ///
    /// A native window goes through the platform requester, the only path iOS
    /// honours for a request issued from inside the draw cycle. A headless
    /// window has no platform behind it, so the request is recorded on its
    /// handle for whoever pumps the frames to find.
    #[inline]
    pub(crate) fn request_animation_frame(&self) {
        match &self.window {
            Some(window @ WindowHandle::Headless(_)) => window.request_redraw(),
            _ => aimer_events::window::request_animation_frame(),
        }
    }

    /// Tells the runtime how fast the display it is drawing on actually is.
    ///
    /// winit reports the rate in millihertz, and only for a monitor it can
    /// identify: a window that has not been placed yet, a headless compositor,
    /// or a platform with no notion of a monitor all answer `None`. In that case
    /// nothing is said and the runtime keeps the rate it has, which is strictly
    /// better than dividing by a number the platform declined to give.
    fn tune_runtime_to_display(&self, window: &Window) {
        let Some(millihertz) = window
            .current_monitor()
            .or_else(|| window.primary_monitor())
            .and_then(|monitor| monitor.refresh_rate_millihertz())
        else {
            return;
        };

        self.venus.set_refresh_rate(millihertz as f32 / 1_000.0);
    }

    /// The bookkeeping every frame does before anything is drawn.
    ///
    /// This frame's share of a scroll gesture is delivered here, and a gesture
    /// that is not finished asks for the frame that continues it — which is
    /// what keeps momentum alive without a platform timer. Shared by the
    /// windowed loop and the headless application so a frame costs the same
    /// work in both.
    pub(crate) fn begin_frame(&mut self) {
        // The budget starts here, because everything after this point is spent
        // out of this frame's time.
        self.venus.begin_frame();

        // A browser never reports the end of a scroll, so the gesture is closed
        // here once its stream has gone quiet — before this frame's step is
        // dispatched, so the terminating phase rides along with it.
        #[cfg(target_arch = "wasm32")]
        if self.web_scroll_phase.poll_idle() {
            self.scroll_smoother.end_gesture();
        }

        let _ = self.dispatch_smoothed_scroll();

        // An open web gesture keeps the frame loop alive even with no distance
        // left, because the idle poll above only runs on a rendered frame.
        #[cfg(target_arch = "wasm32")]
        let gesture_open = self.web_scroll_phase.is_open();
        #[cfg(not(target_arch = "wasm32"))]
        let gesture_open = false;
        if self.scroll_smoother.is_active() || gesture_open {
            self.request_animation_frame();
        }

        #[cfg(debug_assertions)]
        self.poll_inspector_frames();

        // Animation ticks first, so the values the tree is about to be built
        // from are this frame's; then every resolved effect, drained to
        // exhaustion. Both happen after the input above and before the build
        // below, which is the whole contract: a `set_state` from a future that
        // resolved is visible to *this* frame, not the next one.
        self.venus.run_frame_tasks();
        self.venus.run_microtasks();
    }

    /// The bookkeeping every frame does once the tree has been drawn.
    ///
    /// Whatever the frame has left over is spent on background work — image
    /// decode, glyph rasterisation, prefetch — and not a microsecond more; a
    /// frame that overran leaves nothing behind and the next one spends
    /// nothing. Work still waiting when the slack runs out asks for the frame
    /// that continues it, which is what keeps a sliced task moving without a
    /// timer.
    pub(crate) fn end_frame(&mut self) {
        let budget = self.venus.idle_budget();
        self.venus.run_idle(&budget);
        self.venus.end_frame();

        if self.venus.has_ready_work() {
            self.request_animation_frame();
        }
    }

    /// Keeps the inspector overlay repainting for a few frames after it is
    /// switched on or off, so the change is not left half-drawn on an idle
    /// application.
    #[cfg(debug_assertions)]
    pub(crate) fn poll_inspector_frames(&self) {
        let current = self.inspector_enabled();
        let prev = self.inspector_prev_enabled.get();
        if current != prev {
            self.inspector_prev_enabled.set(current);
            self.inspector_change.set(true);
            self.inspector_redraw_frames.set(5);
        }
        let frames = self.inspector_redraw_frames.get();
        if frames > 0 {
            self.inspector_redraw_frames.set(frames - 1);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    pub(crate) fn dispatch_element_event(
        &mut self,
        pos: Vec2d,
        event: &aimer_events::element::ElementEvent,
    ) -> EventResult {
        let Some(root) = &self.widget_root else {
            return EventResult::ignored();
        };
        self.event_dispatcher.dispatch(root.as_ref(), pos, event)
    }

    pub(crate) fn cancel_element_events(&mut self) -> EventResult {
        let Some(root) = &self.widget_root else {
            return EventResult::ignored();
        };
        let result = aimer_widget::broadcast_event(
            root.as_ref(),
            &aimer_events::element::ElementEvent::Cancel,
        );
        self.event_dispatcher.clear_captures();
        result
    }

    /// Delivers this frame's share of the pending scroll distance to the
    /// widget tree.
    ///
    /// Each channel carries the gesture phase resolved by the smoother, so a
    /// child sees `Started` when the gesture opens, `Moved` while it glides,
    /// and `Ended` or `Cancelled` when it finishes — instead of an endless
    /// stream of `Moved`.
    pub(crate) fn dispatch_smoothed_scroll(&mut self) -> EventResult {
        let frame = self.scroll_smoother.tick();
        let mut result = EventResult::ignored();

        if let Some(step) = frame.trackpad {
            result = result.merge(self.dispatch_element_event(
                self.cursor_pos,
                &aimer_events::element::ElementEvent::Scroll {
                    delta: Vec2d {
                        x: step.delta.x as f32,
                        y: step.delta.y as f32,
                    },
                    phase: step.phase,
                    kind: aimer_events::element::ScrollDeltaKind::Pixel,
                    is_direct_manipulation: step.is_direct_manipulation,
                },
            ));
        }
        if let Some(step) = frame.wheel {
            result = result.merge(self.dispatch_element_event(
                self.cursor_pos,
                &aimer_events::element::ElementEvent::Scroll {
                    delta: Vec2d {
                        x: step.delta.x as f32,
                        y: step.delta.y as f32,
                    },
                    phase: step.phase,
                    kind: aimer_events::element::ScrollDeltaKind::Line,
                    is_direct_manipulation: step.is_direct_manipulation,
                },
            ));
        }

        result
    }
}

/// Builds and draws the widget tree of a single frame.
///
/// Owns nothing: it borrows the tree, the widget waiting to become one, and
/// the window they are drawn for, which is what allows the same code to paint
/// a platform surface and a headless canvas. The window it carries is the one
/// the tree sees in its [`BuildContext`], so a widget that asks for a repaint
/// or changes the cursor reaches the same handle the event handlers do.
pub(crate) struct FrameDrawer<'a, W: Widget + 'static> {
    widget_root: &'a mut Option<AnyElement>,
    pending_widget: &'a mut Option<W>,
    window: WindowHandle,
    scale: f32,
    cursor_pos: Vec2d,
    #[cfg(not(target_arch = "wasm32"))]
    async_handle: tokio::runtime::Handle,
    #[cfg(debug_assertions)]
    inspector_enabled: bool,
}

impl<'a, W: Widget + 'static> FrameDrawer<'a, W> {
    /// Draws one frame of `width` by `height` physical pixels into `canvas`.
    ///
    /// The root element is created on the first frame and reused afterwards,
    /// exactly as the windowed loop does — a headless application that renders
    /// twice does not rebuild its tree twice. The tree is drawn inside a saved
    /// canvas scope so a widget that leaves a transform behind cannot leak it
    /// into the next frame.
    pub(crate) fn draw(&mut self, canvas: &aimer_canvas::InnerCanvas, width: u32, height: u32) {
        let canvas = aimer_canvas::Canvas::new(canvas);
        let build_ctx = BuildContext {
            parent_size: ResolvedSize {
                width: width as f32,
                height: height as f32,
            },
            canvas,
            scale: self.scale,
            parent_pos: Default::default(),
            cursor_pos: self.cursor_pos,
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: width as f32,
                max_height: height as f32,
            },
            visible_rect: None,
            window: self.window.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: self.async_handle.clone(),
            inherited_states: Default::default(),
        };

        if self.widget_root.is_none()
            && let Some(widget) = self.pending_widget.take()
        {
            *self.widget_root = Some(widget.to_element(&build_ctx));
        }

        let Some(root) = self.widget_root.as_ref() else {
            return;
        };

        #[cfg(debug_assertions)]
        if let Ok(mut hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.write() {
            *hovered = None;
        }

        build_ctx.canvas.save();
        root.draw(&build_ctx);
        build_ctx.canvas.restore();

        #[cfg(debug_assertions)]
        if self.inspector_enabled {
            // Save and restore canvas state to ensure the inspector overlay
            // always renders at the top layer above all widgets,
            // unaffected by any residual transforms.
            build_ctx.canvas.save();
            InspectorOverlay::draw(
                root.as_ref(),
                &build_ctx.canvas,
                self.cursor_pos,
                build_ctx.scale,
            );
            build_ctx.canvas.restore();
        }
    }
}

/// Runs every hook the application registered, once, in registration order,
/// and keeps whatever they return alive for the rest of the run.
pub(crate) fn run_startup_hooks(
    startup_hooks: &mut Vec<StartupHook>,
    startup_resources: &mut Vec<Box<dyn Any>>,
) {
    startup_resources.extend(startup_hooks.drain(..).map(|hook| hook()));
}

impl<W: Widget + 'static> ApplicationHandler<AimerNativePlatformEvent> for AimerApplicationHandler<W> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        run_startup_hooks(&mut self.startup_hooks, &mut self.startup_resources);
        if self.window.is_none() {
            self.macos_windowing = aimer_native::macos_windowing::take_pending();
        }

        #[cfg(target_os = "android")]
        {
            use winit::event_loop::ControlFlow;
            event_loop.set_control_flow(ControlFlow::Poll);
            debug!("Set ControlFlow::Poll for Android");
        }

        #[cfg(target_os = "ios")]
        if let Some((width, height)) = crate::ios_screen::get_screen_resolution_pixels() {
            self.native_window_size = Some(ResolvedSize {
                width: width as f32,
                height: height as f32,
            })
        };

        let window_attributes = {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                self.window_attr.to_winit()
            }
            #[cfg(target_os = "android")]
            {
                self.window_attr.to_winit()
            }
            #[cfg(target_os = "ios")]
            {
                match crate::ios_screen::get_screen_resolution_pixels() {
                    Some((w, h)) => {
                        // println!("IOS TARGET Window Size : {w}x{h}");
                        let phy_size = PhysicalSize::new(w as u32, h as u32);
                        WindowAttributes::default().with_inner_size(phy_size)
                    }
                    None => WindowAttributes::default(),
                }
            }
        };
        let window_attributes = self.macos_windowing.apply_attributes(window_attributes);

        if self.window.is_none() {
            let window = event_loop.create_window(window_attributes).unwrap();
            let window: &'static Window = Box::leak(Box::new(window)); // Leak to static ref
            aimer_events::window::set_window(window);
            self.window = Some(WindowHandle::native(window));
        }

        let window = self
            .native_window()
            .expect("the windowed loop always owns a native window");
        self.macos_windowing.window_created(window);

        // The runtime was built before this window existed, so it has been
        // budgeting frames against an assumed 60 Hz. Told the real rate here —
        // the first moment there is a monitor to ask — a ProMotion or gaming
        // display stops being handed twice the idle time its frames have, and an
        // overrun is recognised after one missed frame instead of two.
        self.tune_runtime_to_display(window);

        // winit's iOS window is created without a `UIWindowScene`. On the
        // iOS 26/27 SDK the scene life cycle is mandatory, so a scene-less
        // window stays invisible (black screen) and never redraws. Attach it to
        // the active window scene so it becomes visible and starts redrawing.
        #[cfg(target_os = "ios")]
        crate::ios_screen::attach_window_to_active_scene(window);

        // The appearance the system is in right now: a theme that follows the
        // system has to start in the right one, not switch into it on the first
        // change. Platforms that do not report an appearance leave the light
        // default in place. Asked after the window is on screen, because UIKit
        // resolves the appearance against a window and has no answer before
        // that.
        crate::system_appearance::announce(window);

        // Where the platform does not deliver appearance changes as a window
        // event — iOS reports them as UIKit trait changes — they are subscribed
        // to here instead.
        crate::system_appearance::start_observing(window);

        // The region the system reserves in the window — the status bar, the
        // notch, the home indicator. Read after the window is on screen for the
        // same reason the appearance is: UIKit resolves it against a laid-out
        // view. A rotation reports it again, as a resize.
        crate::system_safe_area::announce(window);

        // A browser changes the reservation without winit hearing about it, so
        // its own resize notifications are subscribed to here.
        crate::system_safe_area::start_observing(window);

        #[allow(unused_mut)]
        let mut size = window.inner_size();

        #[cfg(target_os = "android")]
        {
            if let Some(android_app) = crate::aimer_app::ANDROID_APP.get() {
                if let Some(native_window) = android_app.native_window() {
                    let width = native_window.width() as u32;
                    let height = native_window.height() as u32;
                    size = winit::dpi::PhysicalSize::new(width, height);
                }
            }
        }

        #[cfg(target_os = "ios")]
        {
            let full = window.outer_size();
            if full.width != 0 && full.height != 0 {
                size = PhysicalSize::new(full.width, full.height);
            }
            if size.width == 0 || size.height == 0 {
                let fallback = self
                    .native_window_size
                    .map(|s| PhysicalSize::new(s.width as u32, s.height as u32))
                    .or_else(|| {
                        crate::ios_screen::get_screen_resolution_pixels()
                            .map(|(w, h)| PhysicalSize::new(w as u32, h as u32))
                    });
                if let Some(fallback) = fallback {
                    println!("iOS zero window size, using native screen resolution: {fallback:?}");
                    size = fallback;
                }
            }
        }

        // debug!("Logical Window Size : {:?}", window.outer_size());
        // debug!("Physical Window Size : {size:?}");

        self.render_ctx.initialize(window, size);

        self.window_scale = window.scale_factor();

        // On Android the surface may be (re-)created with the correct size now.
        // Schedule a resize so the GPU surface matches the actual window dimensions.
        self.pending_resize = Some(size);
        window.request_redraw();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AimerNativePlatformEvent) {
        // debug!("User event {:?}", event);
        handle_user_event(self, event);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        WindowEventHandler::handle_events(self, event_loop, _id, event);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "macos")]
        if let Some(window) = self.native_window() {
            self.macos_windowing.window_redraw_requested(window);
        }

        // A file drag is the one gesture the platform stops describing once it
        // has begun: macOS delivers no cursor motion at all while its own drag
        // session runs. So the position is asked for here instead, and a frame
        // is asked for in turn, which brings us back here for as long as the
        // drag lasts and not one wake-up longer.
        #[cfg(target_os = "macos")]
        if self.file_drag.is_active() {
            WindowEventHandler::poll_file_drag(self);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        if self.start_up_frames.get() > 0 {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            window.request_redraw();
            self.start_up_frames.set(self.start_up_frames.get() - 1);
            // debug!("About to wait, {} frames left",
            // self.start_up_frames.get());
        }
        #[cfg(debug_assertions)]
        self.poll_inspector_frames();
    }
}
#[allow(dead_code)]
impl<W: Widget + 'static> AimerApplicationHandler<W> {
    #[cfg(debug_assertions)]
    pub(crate) fn inspector_enabled(&self) -> bool {
        self.inspector
            .as_ref()
            .is_some_and(|inspector| inspector.is_enabled())
    }
    /// Splits the handler into the renderer that owns the frame and a drawer
    /// for the widget tree that goes into it.
    ///
    /// The two have to be borrowed apart because the tree is drawn from inside
    /// a closure the renderer runs: the closure cannot hold the handler while
    /// the renderer is being borrowed out of it. A headless application takes
    /// the drawer alone through [`frame_drawer`](Self::frame_drawer) and paints
    /// into its own canvas, so the tree is built, laid out, and drawn by
    /// identical code whichever way the frame was asked for.
    pub(crate) fn split_for_frame(
        &mut self,
        window: WindowHandle,
    ) -> (&mut AimerRenderContext, FrameDrawer<'_, W>) {
        #[cfg(debug_assertions)]
        let inspector_enabled = self.inspector_enabled();
        let scale = self.window_scale as f32;
        let cursor_pos = self.cursor_pos;
        let Self {
            render_ctx,
            widget_root,
            pending_widget,
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime,
            ..
        } = self;
        #[cfg(not(target_arch = "wasm32"))]
        let async_handle = async_runtime.handle().clone();

        (
            render_ctx,
            FrameDrawer {
                widget_root,
                pending_widget,
                window,
                scale,
                cursor_pos,
                #[cfg(not(target_arch = "wasm32"))]
                async_handle,
                #[cfg(debug_assertions)]
                inspector_enabled,
            },
        )
    }

    /// Borrows everything a frame needs to build and draw the widget tree.
    #[inline]
    pub(crate) fn frame_drawer(&mut self, window: WindowHandle) -> FrameDrawer<'_, W> {
        self.split_for_frame(window).1
    }

    #[cfg(debug_assertions)]
    fn broadcast_inspector_snapshot(&self) {
        if let Some(inspector) = self
            .inspector
            .as_ref()
            .filter(|inspector| inspector.is_enabled())
        {
            let snapshot = self.widget_root.as_ref().map(|root| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    aimer_inspector::InspectorServer::snapshot_tree(root)
                }
                #[cfg(target_arch = "wasm32")]
                {
                    aimer_inspector::snapshot_tree(root.as_ref())
                }
            });

            let hovered_id =
                if let Ok(hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.read() {
                    if let Some((name, start, end)) = hovered.as_ref() {
                        snapshot
                            .as_ref()
                            .and_then(|s| find_hovered_node(s, name, *start, *end))
                    } else {
                        None
                    }
                } else {
                    None
                };

            inspector.broadcast_tree(snapshot);
            inspector.broadcast_hovered(hovered_id);
        }
    }

    #[allow(unused)]
    pub(crate) fn render(&mut self, event_loop: &ActiveEventLoop) {
        self.begin_frame();

        #[cfg(target_os = "android")]
        {
            if let Some(android_app) = crate::aimer_app::ANDROID_APP.get() {
                let Some(native_window) = android_app.native_window() else {
                    debug!("Android native window is not ready yet");
                    return;
                };
            }
        }

        #[allow(clippy::collapsible_if)]
        if self.render_ctx.is_ready() {
            if let Some(size) = self.pending_resize.take() {
                self.render_ctx.resize(size);
            }
        }

        let Some(window) = self.window.clone() else {
            return;
        };
        let (render_ctx, mut drawer) = self.split_for_frame(window);

        let outcome =
            render_ctx.render_frame(move |canvas, width, height| drawer.draw(canvas, width, height));
        // A deferred frame is still in flight on the raster thread: it reports
        // the first-frame notification and any retry itself, from `on_present`,
        // because the outcome is not known until a frame later.
        crate::first_frame::notify_first_frame_presented(outcome.is_presented());
        if outcome.needs_retry() {
            // Surface texture was not available (e.g. surface outdated or
            // window not ready).  Request a redraw so we retry next frame
            // instead of staying blank.  Critical on web (async GPU init)
            // and iOS (late surface availability).
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        #[cfg(debug_assertions)]
        self.broadcast_inspector_snapshot();

        self.end_frame();
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::run_startup_hooks;

    #[test]
    fn startup_hooks_run_once_in_registration_order() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut hooks: Vec<Box<dyn FnOnce() -> Box<dyn Any>>> = [1_u8, 2, 3]
            .into_iter()
            .map(|call| {
                let calls = calls.clone();
                Box::new(move || {
                    calls.borrow_mut().push(call);
                    Box::new(call) as Box<dyn Any>
                }) as Box<dyn FnOnce() -> Box<dyn Any>>
            })
            .collect();
        let mut resources = Vec::new();

        run_startup_hooks(&mut hooks, &mut resources);
        run_startup_hooks(&mut hooks, &mut resources);

        assert_eq!(*calls.borrow(), vec![1, 2, 3]);
        assert!(hooks.is_empty());
        assert_eq!(
            resources
                .iter()
                .map(|resource| *resource.downcast_ref::<u8>().unwrap())
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn missing_startup_hooks_are_a_no_op() {
        let mut hooks = Vec::new();
        let mut resources = Vec::new();

        run_startup_hooks(&mut hooks, &mut resources);

        assert!(hooks.is_empty());
        assert!(resources.is_empty());
    }
}
