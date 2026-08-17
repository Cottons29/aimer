//! The gesture state machine, as pure functions.
//!
//! [`recognize`] is the whole recognizer: pointer event in, [`GestureOutput`]
//! out, state threaded through a `&mut`. It fires no callbacks, holds no `Rc`,
//! knows nothing about widgets, and reads the clock only from its `now`
//! parameter.
//!
//! That last point is what makes the recognizer testable. A five-hundred
//! millisecond long-press threshold checked against `AnimInstant::now()` inside
//! the state machine can only be tested by sleeping; checked against a `now`
//! handed in from outside, it is exercised by subtracting a `Duration`.
//!
//! # Examples
//!
//! ```
//! use aimer_attribute::position::Vec2d;
//! use aimer_events::pointer::{PointerEvent, PointerInfo};
//! use aimer_input::gesture::recognize::recognize;
//! use aimer_input::gesture::state::GestureState;
//! use aimer_input::gesture::{GestureEvent, GestureMask};
//! use aimer_utils::AnimInstant;
//!
//! let mut state = GestureState::default();
//! let now = AnimInstant::now();
//! let pointer = PointerInfo::touch(Vec2d { x: 10.0, y: 10.0 }, 0);
//!
//! let down = recognize(&mut state, &PointerEvent::Down(pointer), now, GestureMask::ALL);
//! assert_eq!(down.first(), Some(GestureEvent::TapDown { pointer }));
//!
//! let up = recognize(&mut state, &PointerEvent::Up(pointer), now, GestureMask::ALL);
//! assert!(up.contains(&GestureEvent::Tap { pointer }));
//! ```

pub mod drag;
pub mod scale;
pub mod tap;

use aimer_events::pointer::{PointerEvent, PointerInfo};
use aimer_utils::AnimInstant;

use super::{GestureEvent, GestureMask, GestureOutput};
use crate::gesture::STALE_GESTURE_TOUCH_MS;
use crate::gesture::state::GestureState;

/// Advances the state machine by one pointer event and reports what the user
/// did.
///
/// `now` is the time the event happened. `mask` says which gestures anybody is
/// listening for, and is used only to skip work whose result would be thrown
/// away — a pinch measures two distances on every move, a swipe a velocity on
/// every release. Pass [`GestureMask::ALL`] to recognize everything.
pub fn recognize(
    state: &mut GestureState,
    event: &PointerEvent,
    now: AnimInstant,
    mask: GestureMask,
) -> GestureOutput {
    match event {
        PointerEvent::Down(pointer) => {
            forget_stale_touches(state, now);
            state.touches.insert(*pointer);

            // A second contact turns the interaction into a pinch, and the
            // single-pointer recognizers stand down for its duration.
            if state.touches.len() == 2 && mask.intersects(GestureMask::SCALE) {
                return scale::begin(state);
            }

            if state.touches.len() == 1 {
                return tap::press(state, *pointer, now);
            }

            GestureOutput::new()
        }

        PointerEvent::Up(pointer) => {
            state.touches.remove(pointer.source, pointer.id);

            if state.pinch.is_some() && state.touches.len() < 2 {
                return scale::end(state);
            }

            release(state, *pointer, now, mask)
        }

        PointerEvent::Move(pointer) => {
            // Only a contact that went down on this detector is updated. A
            // mouse gliding over the surface is a hover, not a contact:
            // recording it would leave the detector believing it owns a
            // pointer that never pressed — and, downstream, claiming (and
            // repainting for) every hover move that merely crosses it.
            if state.touches.contains(pointer.source, pointer.id) {
                state.touches.insert(*pointer);
            }

            if state.pinch.is_some()
                && state.touches.len() >= 2
                && mask.intersects(GestureMask::SCALE)
            {
                return scale::update(state);
            }

            drag::moved(state, *pointer, now, mask)
        }

        PointerEvent::Cancel => cancel(state),

        PointerEvent::Scroll { delta_x, delta_y } => GestureOutput::once(GestureEvent::Scroll {
            delta_x: *delta_x,
            delta_y: *delta_y,
        }),
    }
}

/// Reports the gestures that become true purely with the passage of time.
///
/// Only the long press: it must fire while the pointer is *still down*, which no
/// pointer event can trigger. Before this existed, a long press was reported on
/// release after 500 ms, which is a slow tap rather than a long press.
///
/// Cheap and idempotent, so it is safe to call once per frame; it produces the
/// long press exactly once per press.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use aimer_attribute::position::Vec2d;
/// use aimer_events::pointer::{PointerEvent, PointerInfo};
/// use aimer_input::gesture::recognize::{poll, recognize};
/// use aimer_input::gesture::state::GestureState;
/// use aimer_input::gesture::{GestureEvent, GestureMask, LONG_PRESS_DURATION};
/// use aimer_utils::AnimInstant;
///
/// let mut state = GestureState::default();
/// let down_at = AnimInstant::now();
/// let pointer = PointerInfo::touch(Vec2d { x: 4.0, y: 4.0 }, 0);
/// recognize(&mut state, &PointerEvent::Down(pointer), down_at, GestureMask::ALL);
///
/// // Nothing yet.
/// assert!(poll(&mut state, down_at + Duration::from_millis(100), GestureMask::ALL).is_empty());
///
/// // The threshold passes while the finger is still down.
/// let held = poll(&mut state, down_at + LONG_PRESS_DURATION, GestureMask::ALL);
/// assert!(held.contains(&GestureEvent::LongPress { pointer }));
///
/// // ...and only once.
/// assert!(poll(&mut state, down_at + Duration::from_secs(2), GestureMask::ALL).is_empty());
/// ```
pub fn poll(state: &mut GestureState, now: AnimInstant, mask: GestureMask) -> GestureOutput {
    tap::poll_long_press(state, now, mask)
}

/// Decides what a release meant, and reports every gesture it concluded.
///
/// A press ends exactly one way, and this is where that is settled. The order is
/// always long press, then drag, then tap — outermost gesture first — and the
/// press always terminates in either [`GestureEvent::TapUp`] followed by a tap,
/// or [`GestureEvent::TapCancel`]. A consumer showing a pressed visual can
/// therefore watch for `TapDown` and drop it on either terminator, without
/// having to enumerate every way a press can fail.
fn release(
    state: &mut GestureState,
    pointer: PointerInfo,
    now: AnimInstant,
    mask: GestureMask,
) -> GestureOutput {
    // No press being tracked: this is the second finger of a pinch lifting, or
    // a release for a press that was already cancelled.
    let Some(press) = state.press.take() else {
        return GestureOutput::new();
    };
    let drag = state.drag.take();
    let mut output = GestureOutput::new();

    // Held long enough to be a long press, but nothing ever polled — the whole
    // press collapses into one burst rather than being mistaken for a slow tap.
    if drag.is_none() && tap::is_unreported_long_press(&press, now, mask) {
        state.last_tap = None;
        output.push(GestureEvent::LongPress {
            pointer: press.pointer,
        });
        output.push(GestureEvent::LongPressStart {
            pointer: press.pointer,
        });
        output.push(GestureEvent::LongPressEnd { pointer });
        output.push(GestureEvent::TapCancel);
        return output;
    }

    if press.long_pressed {
        output.push(GestureEvent::LongPressEnd { pointer });
    }

    if let Some(drag) = drag {
        for event in drag::end(&drag, pointer, now, mask) {
            output.push(event);
        }
        output.push(GestureEvent::TapCancel);
        state.last_tap = None;
        return output;
    }

    if press.long_pressed {
        output.push(GestureEvent::TapCancel);
        state.last_tap = None;
        return output;
    }

    for event in tap::tap_or_cancel(state, &press, pointer, now, mask) {
        output.push(event);
    }
    output
}

/// Abandons every gesture in progress, reporting each one that had been
/// announced so no consumer is left believing it is still running.
fn cancel(state: &mut GestureState) -> GestureOutput {
    let mut output = GestureOutput::new();

    if state.is_dragging() {
        output.push(GestureEvent::DragCancel);
    }
    if let Some(press) = state.press {
        if press.long_pressed {
            output.push(GestureEvent::LongPressEnd {
                pointer: press.long_press_last.unwrap_or(press.pointer),
            });
        }
        output.push(GestureEvent::TapCancel);
    }
    if state.pinch.is_some() {
        output.push(GestureEvent::ScaleEnd);
    }

    state.clear_press();
    state.clear_pinch();
    state.touches.clear();
    state.last_tap = None;

    output
}

/// Drops contacts left over from an interaction that was never finished.
///
/// An app backgrounded mid-touch gets no `Up` and no `Cancel`, so its contacts
/// stay down forever — and the next single tap would land as the second contact
/// of a pinch. Anything older than a second is treated as gone.
fn forget_stale_touches(state: &mut GestureState, now: AnimInstant) {
    let stale = state.press.is_none_or(|press| {
        now.duration_since(press.down_at).as_millis() > STALE_GESTURE_TOUCH_MS as u128
    });

    if !state.touches.is_empty() && stale {
        state.touches.clear();
        state.clear_pinch();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use aimer_attribute::position::Vec2d;
    use aimer_events::pointer::{PointerButton, PointerInfo};

    use super::*;
    use crate::gesture::{DOUBLE_TAP_TIMEOUT, LONG_PRESS_DURATION, SwipeDirection};

    /// A clock a test drives by hand, so a 500 ms threshold costs no wall time.
    struct Clock {
        now: AnimInstant,
    }

    impl Clock {
        fn new() -> Self {
            Self {
                now: AnimInstant::now(),
            }
        }

        fn advance(&mut self, by: Duration) -> AnimInstant {
            self.now += by;
            self.now
        }
    }

    fn touch(x: f32, y: f32) -> PointerInfo {
        PointerInfo::touch(Vec2d { x, y }, 0)
    }

    fn second_touch(x: f32, y: f32) -> PointerInfo {
        PointerInfo::touch(Vec2d { x, y }, 1)
    }

    fn mouse(x: f32, y: f32, button: PointerButton) -> PointerInfo {
        PointerInfo::mouse(Vec2d { x, y }, button)
    }

    fn down(state: &mut GestureState, pointer: PointerInfo, now: AnimInstant) -> GestureOutput {
        recognize(state, &PointerEvent::Down(pointer), now, GestureMask::ALL)
    }

    fn moved(state: &mut GestureState, pointer: PointerInfo, now: AnimInstant) -> GestureOutput {
        recognize(state, &PointerEvent::Move(pointer), now, GestureMask::ALL)
    }

    fn up(state: &mut GestureState, pointer: PointerInfo, now: AnimInstant) -> GestureOutput {
        recognize(state, &PointerEvent::Up(pointer), now, GestureMask::ALL)
    }

    #[test]
    fn a_press_and_release_in_place_is_a_tap() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);

        let pressed = down(&mut state, pointer, clock.now);
        let released = up(&mut state, pointer, clock.advance(Duration::from_millis(40)));

        assert_eq!(pressed.first(), Some(GestureEvent::TapDown { pointer }));
        assert_eq!(
            released.iter().copied().collect::<Vec<_>>(),
            vec![
                GestureEvent::TapUp { pointer },
                GestureEvent::Tap { pointer },
            ]
        );
    }

    // The bug this whole redesign started from: a right press reached the
    // detector as an ordinary press with no button on it, so `RightClick` was
    // never constructed and a right-click fired `on_tap`.
    #[test]
    fn a_secondary_press_is_a_tap_that_reports_the_secondary_button() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = mouse(5.0, 5.0, PointerButton::Secondary);

        down(&mut state, pointer, clock.now);
        let released = up(&mut state, pointer, clock.advance(Duration::from_millis(20)));

        let tap = released
            .iter()
            .copied()
            .find(|event| matches!(event, GestureEvent::Tap { .. }))
            .expect("a secondary click still completes a tap");

        assert_eq!(tap, GestureEvent::Tap { pointer });
        assert_eq!(tap.button(), PointerButton::Secondary);
    }

    #[test]
    fn a_middle_press_is_a_tap_that_reports_the_middle_button() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = mouse(5.0, 5.0, PointerButton::Middle);

        down(&mut state, pointer, clock.now);
        let released = up(&mut state, pointer, clock.advance(Duration::from_millis(20)));

        assert!(released.contains(&GestureEvent::Tap { pointer }));
        assert_eq!(
            released
                .iter()
                .find(|event| matches!(event, GestureEvent::Tap { .. }))
                .map(GestureEvent::button),
            Some(PointerButton::Middle)
        );
    }

    #[test]
    fn two_quick_taps_in_the_same_place_are_a_double_tap() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);

        down(&mut state, pointer, clock.now);
        up(&mut state, pointer, clock.advance(Duration::from_millis(30)));
        down(&mut state, pointer, clock.advance(Duration::from_millis(60)));
        let second = up(&mut state, pointer, clock.advance(Duration::from_millis(30)));

        assert!(second.contains(&GestureEvent::DoubleTap { pointer }));
        assert!(!second.contains(&GestureEvent::Tap { pointer }));
    }

    #[test]
    fn a_second_tap_after_the_timeout_is_just_another_tap() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);

        down(&mut state, pointer, clock.now);
        up(&mut state, pointer, clock.advance(Duration::from_millis(30)));
        down(&mut state, pointer, clock.advance(DOUBLE_TAP_TIMEOUT));
        let second = up(&mut state, pointer, clock.advance(Duration::from_millis(30)));

        assert!(second.contains(&GestureEvent::Tap { pointer }));
        assert!(!second.contains(&GestureEvent::DoubleTap { pointer }));
    }

    #[test]
    fn a_second_tap_far_from_the_first_is_just_another_tap() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let first = touch(10.0, 10.0);
        let far = touch(200.0, 200.0);

        down(&mut state, first, clock.now);
        up(&mut state, first, clock.advance(Duration::from_millis(30)));
        down(&mut state, far, clock.advance(Duration::from_millis(60)));
        let second = up(&mut state, far, clock.advance(Duration::from_millis(30)));

        assert!(second.contains(&GestureEvent::Tap { pointer: far }));
        assert!(!second.contains(&GestureEvent::DoubleTap { pointer: far }));
    }

    #[test]
    fn a_double_tap_is_not_reported_when_nobody_listens_for_one() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);
        let mask = GestureMask::TAP;

        recognize(&mut state, &PointerEvent::Down(pointer), clock.now, mask);
        recognize(
            &mut state,
            &PointerEvent::Up(pointer),
            clock.advance(Duration::from_millis(30)),
            mask,
        );
        recognize(
            &mut state,
            &PointerEvent::Down(pointer),
            clock.advance(Duration::from_millis(60)),
            mask,
        );
        let second = recognize(
            &mut state,
            &PointerEvent::Up(pointer),
            clock.advance(Duration::from_millis(30)),
            mask,
        );

        assert!(second.contains(&GestureEvent::Tap { pointer }));
        assert!(!second.contains(&GestureEvent::DoubleTap { pointer }));
    }

    // The second bug: the old state machine returned `Swipe` *instead of*
    // `DragEnd`, having already called `on_drag_end`. The callback path and the
    // reported event disagreed.
    #[test]
    fn a_fast_flick_reports_both_the_drag_end_and_the_swipe() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let start = touch(0.0, 0.0);
        let end = touch(200.0, 0.0);

        down(&mut state, start, clock.now);
        moved(&mut state, end, clock.advance(Duration::from_millis(20)));
        let released = up(&mut state, end, clock.advance(Duration::from_millis(20)));

        let recognized: Vec<_> = released.iter().copied().collect();

        assert!(
            recognized.contains(&GestureEvent::DragEnd { pointer: end }),
            "a flick still ends the drag: {recognized:?}"
        );
        assert!(
            recognized
                .iter()
                .any(|event| matches!(event, GestureEvent::Swipe { .. })),
            "and is also a swipe: {recognized:?}"
        );
        assert_eq!(
            recognized
                .iter()
                .position(|event| matches!(event, GestureEvent::DragEnd { .. })),
            Some(0),
            "the drag ends before the swipe is reported"
        );
    }

    #[test]
    fn a_slow_drag_ends_without_a_swipe() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let start = touch(0.0, 0.0);
        let end = touch(40.0, 0.0);

        down(&mut state, start, clock.now);
        moved(&mut state, end, clock.advance(Duration::from_millis(400)));
        let released = up(&mut state, end, clock.advance(Duration::from_millis(400)));

        assert!(released.contains(&GestureEvent::DragEnd { pointer: end }));
        assert!(
            !released
                .iter()
                .any(|event| matches!(event, GestureEvent::Swipe { .. })),
            "300 px/s is the floor and this is far under it"
        );
    }

    #[test]
    fn a_swipe_reports_the_direction_it_travelled() {
        let cases = [
            ((200.0, 10.0), SwipeDirection::Right),
            ((-200.0, 10.0), SwipeDirection::Left),
            ((10.0, 200.0), SwipeDirection::Down),
            ((10.0, -200.0), SwipeDirection::Up),
        ];

        for ((dx, dy), expected) in cases {
            let mut clock = Clock::new();
            let mut state = GestureState::default();
            let start = touch(300.0, 300.0);
            let end = touch(300.0 + dx, 300.0 + dy);

            down(&mut state, start, clock.now);
            moved(&mut state, end, clock.advance(Duration::from_millis(20)));
            let released = up(&mut state, end, clock.advance(Duration::from_millis(20)));

            let swipe = released
                .iter()
                .copied()
                .find_map(|event| match event {
                    GestureEvent::Swipe { direction, .. } => Some(direction),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("({dx}, {dy}) should swipe {expected:?}"));

            assert_eq!(swipe, expected, "for delta ({dx}, {dy})");
        }
    }

    #[test]
    fn a_drag_starts_where_the_press_did_not_where_the_slop_was_crossed() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let start = touch(0.0, 0.0);
        let crossed = touch(30.0, 0.0);

        down(&mut state, start, clock.now);
        let moves = moved(&mut state, crossed, clock.advance(Duration::from_millis(16)));

        assert_eq!(
            moves.first(),
            Some(GestureEvent::DragStart { pointer: start }),
            "reporting the crossing position would make the dragged thing jump"
        );
    }

    #[test]
    fn a_drag_update_reports_the_delta_since_the_previous_position() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let start = touch(0.0, 0.0);
        let crossed = touch(30.0, 0.0);
        let further = touch(45.0, 10.0);

        down(&mut state, start, clock.now);
        moved(&mut state, crossed, clock.advance(Duration::from_millis(16)));
        let updates = moved(&mut state, further, clock.advance(Duration::from_millis(16)));

        assert_eq!(
            updates.first(),
            Some(GestureEvent::DragUpdate {
                pointer: further,
                delta_x: 15.0,
                delta_y: 10.0,
            })
        );
    }

    #[test]
    fn a_release_after_leaving_the_slop_is_not_a_tap() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let start = touch(0.0, 0.0);
        let away = touch(30.0, 0.0);
        let mask = GestureMask::TAP;

        recognize(&mut state, &PointerEvent::Down(start), clock.now, mask);
        // No drag is recognized because nobody listens for one...
        recognize(
            &mut state,
            &PointerEvent::Move(away),
            clock.advance(Duration::from_millis(16)),
            mask,
        );
        let released = recognize(
            &mut state,
            &PointerEvent::Up(away),
            clock.advance(Duration::from_millis(16)),
            mask,
        );

        // ...but the press still failed, and must not become a tap.
        assert_eq!(
            released.iter().copied().collect::<Vec<_>>(),
            vec![GestureEvent::TapCancel]
        );
    }

    #[test]
    fn a_long_press_fires_while_the_pointer_is_still_down() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);

        down(&mut state, pointer, clock.now);

        assert!(
            poll(
                &mut state,
                clock.advance(LONG_PRESS_DURATION - Duration::from_millis(1)),
                GestureMask::ALL
            )
            .is_empty(),
            "not yet a long press"
        );

        let held = poll(
            &mut state,
            clock.advance(Duration::from_millis(2)),
            GestureMask::ALL,
        );

        assert_eq!(
            held.iter().copied().collect::<Vec<_>>(),
            vec![
                GestureEvent::LongPress { pointer },
                GestureEvent::LongPressStart { pointer },
            ]
        );
        assert!(
            poll(
                &mut state,
                clock.advance(Duration::from_secs(1)),
                GestureMask::ALL
            )
            .is_empty(),
            "a long press is reported once per press"
        );
    }

    #[test]
    fn releasing_a_long_press_ends_it_rather_than_tapping() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);

        down(&mut state, pointer, clock.now);
        poll(&mut state, clock.advance(LONG_PRESS_DURATION), GestureMask::ALL);
        let released = up(&mut state, pointer, clock.advance(Duration::from_millis(10)));

        assert_eq!(
            released.iter().copied().collect::<Vec<_>>(),
            vec![
                GestureEvent::LongPressEnd { pointer },
                GestureEvent::TapCancel,
            ]
        );
    }

    // Nothing is guaranteed to poll the recognizer, so a release that is late
    // enough must still be recognized as a long press rather than silently
    // becoming a tap.
    #[test]
    fn a_late_release_is_a_long_press_even_if_nothing_polled() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);

        down(&mut state, pointer, clock.now);
        let released = up(
            &mut state,
            pointer,
            clock.advance(LONG_PRESS_DURATION + Duration::from_millis(50)),
        );

        let recognized: Vec<_> = released.iter().copied().collect();

        assert!(recognized.contains(&GestureEvent::LongPress { pointer }));
        assert!(recognized.contains(&GestureEvent::LongPressEnd { pointer }));
        assert!(
            !recognized.contains(&GestureEvent::Tap { pointer }),
            "a held press is not a slow tap: {recognized:?}"
        );
    }

    #[test]
    fn moving_a_held_long_press_reports_the_movement() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);
        let dragged = touch(40.0, 10.0);

        down(&mut state, pointer, clock.now);
        poll(&mut state, clock.advance(LONG_PRESS_DURATION), GestureMask::ALL);
        let moves = moved(&mut state, dragged, clock.advance(Duration::from_millis(16)));

        let recognized: Vec<_> = moves.iter().copied().collect();

        assert!(recognized.contains(&GestureEvent::LongPressMoveUpdate {
            pointer: dragged,
            delta_x: 30.0,
            delta_y: 0.0,
        }));
        assert!(
            recognized.contains(&GestureEvent::DragStart { pointer }),
            "a long press that moves is also the drag aimer_dnd waits for: {recognized:?}"
        );
    }

    #[test]
    fn a_press_that_never_moved_never_starts_a_drag() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let pointer = touch(10.0, 10.0);
        let wobble = touch(14.0, 12.0);

        down(&mut state, pointer, clock.now);
        let moves = moved(&mut state, wobble, clock.advance(Duration::from_millis(16)));

        assert!(moves.is_empty(), "4 px of finger wobble is not a drag");
        assert!(!state.is_dragging());
    }

    #[test]
    fn a_mouse_starts_dragging_after_a_pixel_where_a_finger_would_not() {
        let mut clock = Clock::new();
        let mut mouse_state = GestureState::default();
        let mut touch_state = GestureState::default();
        let mouse_down = mouse(0.0, 0.0, PointerButton::Primary);
        let mouse_moved = mouse(5.0, 0.0, PointerButton::Primary);

        down(&mut mouse_state, mouse_down, clock.now);
        let mouse_moves = moved(
            &mut mouse_state,
            mouse_moved,
            clock.advance(Duration::from_millis(16)),
        );

        down(&mut touch_state, touch(0.0, 0.0), clock.now);
        let touch_moves = moved(
            &mut touch_state,
            touch(5.0, 0.0),
            clock.advance(Duration::from_millis(16)),
        );

        assert!(
            mouse_moves.contains(&GestureEvent::DragStart { pointer: mouse_down }),
            "5 px of mouse travel is a deliberate drag: {mouse_moves:?}"
        );
        assert!(
            touch_moves.is_empty(),
            "5 px of finger travel is still a tap: {touch_moves:?}"
        );
    }

    #[test]
    fn a_second_contact_starts_a_pinch_and_a_lift_ends_it() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let first = touch(0.0, 0.0);
        let second = second_touch(100.0, 0.0);

        down(&mut state, first, clock.now);
        let began = down(&mut state, second, clock.advance(Duration::from_millis(16)));

        assert_eq!(
            began.first(),
            Some(GestureEvent::ScaleStart {
                focal_x: 50.0,
                focal_y: 0.0,
            })
        );

        let spread = moved(
            &mut state,
            second_touch(200.0, 0.0),
            clock.advance(Duration::from_millis(16)),
        );

        assert_eq!(
            spread.first(),
            Some(GestureEvent::ScaleUpdate {
                focal_x: 100.0,
                focal_y: 0.0,
                scale: 2.0,
                delta_scale: 2.0,
            })
        );

        let ended = up(
            &mut state,
            second_touch(200.0, 0.0),
            clock.advance(Duration::from_millis(16)),
        );

        assert_eq!(ended.first(), Some(GestureEvent::ScaleEnd));
        assert!(state.pinch.is_none());
    }

    #[test]
    fn a_cancel_reports_every_gesture_it_abandons() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();
        let start = touch(0.0, 0.0);

        down(&mut state, start, clock.now);
        moved(
            &mut state,
            touch(40.0, 0.0),
            clock.advance(Duration::from_millis(16)),
        );
        let cancelled = recognize(
            &mut state,
            &PointerEvent::Cancel,
            clock.advance(Duration::from_millis(16)),
            GestureMask::ALL,
        );

        assert_eq!(
            cancelled.iter().copied().collect::<Vec<_>>(),
            vec![GestureEvent::DragCancel, GestureEvent::TapCancel],
            "a consumer told a drag started must be told it stopped"
        );
        assert!(!state.is_dragging());
        assert!(state.press.is_none());
        assert!(state.touches.is_empty());
    }

    #[test]
    fn a_cancel_with_nothing_in_progress_reports_nothing() {
        let mut state = GestureState::default();

        let cancelled = recognize(
            &mut state,
            &PointerEvent::Cancel,
            AnimInstant::now(),
            GestureMask::ALL,
        );

        assert!(cancelled.is_empty());
    }

    // An app backgrounded mid-touch never gets an Up, so without this the next
    // single tap would arrive as the second contact of a pinch.
    #[test]
    fn a_contact_left_over_from_a_backgrounded_app_does_not_start_a_pinch() {
        let mut clock = Clock::new();
        let mut state = GestureState::default();

        down(&mut state, touch(0.0, 0.0), clock.now);
        // ...and the app disappears for two seconds without an Up or a Cancel.
        let fresh = second_touch(50.0, 50.0);
        let pressed = down(&mut state, fresh, clock.advance(Duration::from_secs(2)));

        assert_eq!(pressed.first(), Some(GestureEvent::TapDown { pointer: fresh }));
        assert_eq!(state.touches.len(), 1);
        assert!(state.pinch.is_none());
    }

    #[test]
    fn a_scroll_is_reported_verbatim() {
        let mut state = GestureState::default();

        let scrolled = recognize(
            &mut state,
            &PointerEvent::Scroll {
                delta_x: -3.0,
                delta_y: 12.5,
            },
            AnimInstant::now(),
            GestureMask::ALL,
        );

        assert_eq!(
            scrolled.first(),
            Some(GestureEvent::Scroll {
                delta_x: -3.0,
                delta_y: 12.5,
            })
        );
    }
}
