//! The handlers a detector was configured with, and the fan-out from
//! [`GestureEvent`] to them.
//!
//! Held behind a single [`std::rc::Rc`] by the detector, so rebuilding a widget
//! costs one refcount bump however many gestures are configured. Storing a
//! callback per gesture in the widget meant ten clones per detector per rebuild
//! — sixteen once the press and long-press lifecycles were added — times every
//! row of a list, times every frame.

use aimer_events::pointer::PointerButton;

use crate::callback::{CallbackExecutor, VoidCallback};
use crate::gesture::{
    DragCallback, DragUpdateCallback, DragUpdateData, GestureEvent, GestureMask, GestureOutput,
    GestureStreamCallback, ScaleCallback, ScaleData, ScrollCallback, ScrollData, SwipeCallback,
};

/// Where an async callback is spawned.
///
/// Re-exported so a detector's handlers and the callbacks they hold name the
/// same type; the spawning policy itself belongs to the callback, not to
/// gestures.
pub use crate::callback::AsyncSpawner;

/// Every handler a [`super::gesture_detector::GestureDetector`] may have, plus
/// the [`GestureMask`] describing which of them are actually set.
///
/// The mask is maintained as handlers are installed rather than derived on
/// demand, so the recognizer's "does anyone want a pinch?" question is one
/// integer test on the hot path instead of eleven `Option` probes.
#[derive(Default)]
pub struct GestureHandlers {
    pub on_tap: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_long_press: VoidCallback,
    pub on_drag_start: DragCallback,
    pub on_drag_update: DragUpdateCallback,
    pub on_drag_end: VoidCallback,
    pub on_right_tap: VoidCallback,
    pub on_swipe: SwipeCallback,
    pub on_scroll: ScrollCallback,
    pub on_scale: ScaleCallback,
    pub on_gesture: GestureStreamCallback,
    mask: GestureMask,
}

impl GestureHandlers {
    /// No handlers at all.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// The gestures these handlers care about.
    #[inline]
    pub const fn mask(&self) -> GestureMask {
        self.mask
    }

    /// Whether a scroll event should be consumed rather than left to fall
    /// through.
    ///
    /// A scroll carries no position, so this cannot be decided by hit test: a
    /// detector with no scroll handler must let the event reach whatever is
    /// behind it. Getting this wrong is why wheel scrolling once appeared dead
    /// — a handler-less detector on a top layer swallowed every scroll before a
    /// `Scrollable` below could see it.
    #[inline]
    pub const fn consumes_scroll(&self) -> bool {
        self.mask.intersects(GestureMask::SCROLL)
    }

    /// Widens the mask, which is how the raw
    /// [`super::gesture_detector::GestureDetector::on_gesture`] handler declares
    /// interest in everything.
    #[inline]
    pub const fn observe(&mut self, gestures: GestureMask) {
        self.mask.insert(gestures);
    }

    /// Delivers one recognized gesture to whichever handlers want it.
    ///
    /// [`Self::on_gesture`] sees every gesture; the individual handlers are
    /// filters over the same stream, never a parallel path — which is what keeps
    /// the two from disagreeing about what happened.
    pub fn dispatch(&self, event: GestureEvent, spawner: &AsyncSpawner) {
        Self::call(&self.on_gesture, event, spawner);

        match event {
            // Only the main button activates. A secondary click goes to
            // `on_right_tap`, and a middle or extra button reaches the raw
            // handler only — before the button travelled with the pointer, a
            // right-click fired `on_tap`, which is the bug this replaced.
            GestureEvent::Tap { pointer } => match pointer.button {
                PointerButton::Primary => Self::call(&self.on_tap, (), spawner),
                PointerButton::Secondary => Self::call(&self.on_right_tap, (), spawner),
                _ => {}
            },
            GestureEvent::DoubleTap { pointer } if pointer.is_primary() => {
                Self::call(&self.on_double_press, (), spawner)
            }
            GestureEvent::LongPress { pointer } if pointer.is_primary() => {
                Self::call(&self.on_long_press, (), spawner)
            }
            GestureEvent::DragStart { pointer } => Self::call(&self.on_drag_start, pointer, spawner),
            GestureEvent::DragUpdate {
                pointer,
                delta_x,
                delta_y,
            } => Self::call(
                &self.on_drag_update,
                DragUpdateData {
                    position: pointer,
                    delta_x,
                    delta_y,
                },
                spawner,
            ),
            GestureEvent::DragEnd { .. } => Self::call(&self.on_drag_end, (), spawner),
            GestureEvent::Swipe { direction, .. } => Self::call(&self.on_swipe, direction, spawner),
            GestureEvent::Scroll { delta_x, delta_y } => {
                Self::call(&self.on_scroll, ScrollData { delta_x, delta_y }, spawner)
            }
            GestureEvent::ScaleUpdate {
                focal_x,
                focal_y,
                scale,
                delta_scale,
            } => Self::call(
                &self.on_scale,
                ScaleData {
                    focal_x,
                    focal_y,
                    scale,
                    delta_scale,
                },
                spawner,
            ),
            // The press and long-press lifecycles, drag cancellation, and the
            // pinch boundaries have no sugar setter of their own: they are read
            // through `on_gesture`, which has already been called above.
            _ => {}
        }
    }

    /// Delivers every gesture a pointer event produced, in order.
    #[inline]
    pub fn dispatch_all(&self, output: GestureOutput, spawner: &AsyncSpawner) {
        for event in output {
            self.dispatch(event, spawner);
        }
    }

    /// Invokes one callback, sync or async, whatever its argument type.
    ///
    /// Generic over [`CallbackExecutor`] rather than over a concrete callback
    /// type, because a no-argument [`VoidCallback`] and a parameterised one are
    /// different types that share only that trait. A gesture handler returns
    /// nothing, so there is no synchronous result to pass back on.
    #[inline]
    fn call<C>(callback: &C, arg: C::Args, spawner: &AsyncSpawner)
    where
        C: CallbackExecutor<Output = ()>,
    {
        callback.execute(arg, spawner);
    }
}

/// Generates the mask-maintaining setters, so a handler cannot be installed
/// without the recognizer being told to look for its gesture.
macro_rules! handler_setters {
    ($($setter:ident => $field:ident : $callback:ty, $gestures:expr, $doc:expr;)*) => {
        impl GestureHandlers {
            $(
                #[doc = $doc]
                #[inline]
                pub fn $setter(&mut self, callback: impl Into<$callback>) {
                    self.$field = callback.into();
                    self.mask.insert($gestures);
                }
            )*
        }
    };
}

handler_setters! {
    set_on_tap => on_tap: VoidCallback, GestureMask::TAP,
        "Installs the handler for a completed primary tap.";
    set_on_double_press => on_double_press: VoidCallback,
        GestureMask::TAP.union(GestureMask::DOUBLE_TAP),
        "Installs the handler for two qualifying taps inside the double-tap timeout.";
    set_on_long_press => on_long_press: VoidCallback, GestureMask::LONG_PRESS,
        "Installs the handler for a press held past the long-press threshold.";
    set_on_drag_start => on_drag_start: DragCallback, GestureMask::DRAG,
        "Installs the handler for movement crossing the device's slop.";
    set_on_drag_update => on_drag_update: DragUpdateCallback, GestureMask::DRAG,
        "Installs the handler for movement during an active drag.";
    set_on_drag_end => on_drag_end: VoidCallback, GestureMask::DRAG,
        "Installs the handler for the release that ends a drag.";
    set_on_right_tap => on_right_tap: VoidCallback, GestureMask::TAP,
        "Installs the handler for a completed secondary-button tap.";
    set_on_swipe => on_swipe: SwipeCallback,
        GestureMask::DRAG.union(GestureMask::SWIPE),
        "Installs the handler for a drag fast enough to read as a flick.";
    set_on_scroll => on_scroll: ScrollCallback, GestureMask::SCROLL,
        "Installs the handler for wheel and trackpad scrolling, which also makes the detector consume those events.";
    set_on_scale => on_scale: ScaleCallback, GestureMask::SCALE,
        "Installs the handler for a two-pointer pinch.";
    set_on_gesture => on_gesture: GestureStreamCallback, GestureMask::EVERY_POINTER_GESTURE,
        "Installs the handler that sees every recognized gesture, including the ones with no sugar setter of their own.\n\nScrolls are still reported to it, but observing one does not claim it — see [`GestureMask::EVERY_POINTER_GESTURE`].";
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use aimer_attribute::position::Vec2d;
    use aimer_events::pointer::PointerInfo;

    use super::*;
    use crate::gesture::SwipeDirection;

    #[cfg(not(target_arch = "wasm32"))]
    fn spawner() -> AsyncSpawner {
        None
    }

    #[cfg(target_arch = "wasm32")]
    fn spawner() -> AsyncSpawner {}

    fn mouse(button: PointerButton) -> PointerInfo {
        PointerInfo::mouse(Vec2d { x: 4.0, y: 4.0 }, button)
    }

    /// Records which handler names fired, in order.
    fn recorder() -> (Rc<RefCell<Vec<&'static str>>>, GestureHandlers) {
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let mut handlers = GestureHandlers::new();

        let tap = log.clone();
        handlers.set_on_tap(move || tap.borrow_mut().push("tap"));
        let right = log.clone();
        handlers.set_on_right_tap(move || right.borrow_mut().push("right_tap"));
        let double = log.clone();
        handlers.set_on_double_press(move || double.borrow_mut().push("double"));
        let long = log.clone();
        handlers.set_on_long_press(move || long.borrow_mut().push("long"));
        let start = log.clone();
        handlers.set_on_drag_start(move |_: PointerInfo| start.borrow_mut().push("drag_start"));
        let end = log.clone();
        handlers.set_on_drag_end(move || end.borrow_mut().push("drag_end"));
        let swipe = log.clone();
        handlers.set_on_swipe(move |_: SwipeDirection| swipe.borrow_mut().push("swipe"));

        (log, handlers)
    }

    #[test]
    fn no_handlers_means_an_empty_mask_and_no_scroll_consumed() {
        let handlers = GestureHandlers::new();

        assert!(handlers.mask().is_empty());
        assert!(!handlers.consumes_scroll());
    }

    #[test]
    fn installing_a_handler_declares_interest_in_its_gesture() {
        let mut handlers = GestureHandlers::new();
        handlers.set_on_scale(|_: ScaleData| {});

        assert!(handlers.mask().contains(GestureMask::SCALE));
        assert!(!handlers.mask().contains(GestureMask::TAP));
    }

    // A swipe is only ever produced by a drag, so listening for one has to make
    // the recognizer track drags even when nothing listens for the drag itself.
    #[test]
    fn listening_for_a_swipe_also_asks_for_drag_tracking() {
        let mut handlers = GestureHandlers::new();
        handlers.set_on_swipe(|_: SwipeDirection| {});

        assert!(handlers.mask().contains(GestureMask::DRAG));
        assert!(handlers.mask().contains(GestureMask::SWIPE));
    }

    // The raw handler observes everything, but observing a scroll is not the same
    // as claiming it: a `Button` that watches the press lifecycle must not start
    // swallowing the wheel scrolls of the list it sits in.
    #[test]
    fn the_raw_handler_asks_for_every_gesture_but_claims_no_scroll() {
        let mut handlers = GestureHandlers::new();
        handlers.set_on_gesture(|_: GestureEvent| {});

        assert!(handlers.mask().contains(GestureMask::SCALE));
        assert!(handlers.mask().contains(GestureMask::LONG_PRESS));
        assert!(handlers.mask().contains(GestureMask::PRESS));
        assert!(!handlers.consumes_scroll());
    }

    #[test]
    fn only_a_scroll_handler_makes_a_detector_consume_scrolls() {
        let mut handlers = GestureHandlers::new();
        handlers.set_on_tap(|| {});

        assert!(
            !handlers.consumes_scroll(),
            "a handler-less detector must let the scroll reach lower layers"
        );

        handlers.set_on_scroll(|_: ScrollData| {});

        assert!(handlers.consumes_scroll());
    }

    // The headline bug: a right-click used to fire `on_tap`, because the button
    // never reached the recognizer at all.
    #[test]
    fn a_secondary_tap_fires_only_the_secondary_handler() {
        let (log, handlers) = recorder();

        handlers.dispatch(
            GestureEvent::Tap {
                pointer: mouse(PointerButton::Secondary),
            },
            &spawner(),
        );

        assert_eq!(*log.borrow(), vec!["right_tap"]);
    }

    #[test]
    fn a_primary_tap_fires_only_the_primary_handler() {
        let (log, handlers) = recorder();

        handlers.dispatch(
            GestureEvent::Tap {
                pointer: mouse(PointerButton::Primary),
            },
            &spawner(),
        );

        assert_eq!(*log.borrow(), vec!["tap"]);
    }

    // Middle-click was dropped by the platform layer entirely. It now arrives,
    // and reaches the raw handler without pretending to be an activation.
    #[test]
    fn a_middle_tap_fires_no_sugar_handler() {
        let (log, handlers) = recorder();

        handlers.dispatch(
            GestureEvent::Tap {
                pointer: mouse(PointerButton::Middle),
            },
            &spawner(),
        );

        assert!(log.borrow().is_empty());
    }

    #[test]
    fn the_raw_handler_sees_gestures_that_have_no_sugar_setter() {
        let seen: Rc<RefCell<Vec<GestureEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let mut handlers = GestureHandlers::new();
        let recorded = seen.clone();
        handlers.set_on_gesture(move |event: GestureEvent| recorded.borrow_mut().push(event));

        let pointer = mouse(PointerButton::Primary);
        handlers.dispatch(GestureEvent::TapDown { pointer }, &spawner());
        handlers.dispatch(GestureEvent::TapCancel, &spawner());
        handlers.dispatch(GestureEvent::DragCancel, &spawner());
        handlers.dispatch(GestureEvent::LongPressStart { pointer }, &spawner());

        assert_eq!(
            *seen.borrow(),
            vec![
                GestureEvent::TapDown { pointer },
                GestureEvent::TapCancel,
                GestureEvent::DragCancel,
                GestureEvent::LongPressStart { pointer },
            ]
        );
    }

    // The old state machine called `on_drag_end` and then reported only the
    // swipe. Both handlers must fire, in order, from one release.
    #[test]
    fn a_flick_fires_the_drag_end_and_the_swipe_in_order() {
        let (log, handlers) = recorder();
        let pointer = mouse(PointerButton::Primary);
        let mut output = GestureOutput::new();
        output.push(GestureEvent::DragEnd { pointer });
        output.push(GestureEvent::Swipe {
            direction: SwipeDirection::Right,
            velocity_x: 900.0,
            velocity_y: 0.0,
        });

        handlers.dispatch_all(output, &spawner());

        assert_eq!(*log.borrow(), vec!["drag_end", "swipe"]);
    }

    #[test]
    fn dispatching_to_a_missing_handler_is_a_no_op() {
        let handlers = GestureHandlers::new();

        handlers.dispatch(
            GestureEvent::Tap {
                pointer: mouse(PointerButton::Primary),
            },
            &spawner(),
        );
    }
}
