//! Pinch (scale) recognition.
//!
//! Measured between the two contacts that arrived first, which is why
//! [`crate::gesture::state::ActiveTouches`] preserves arrival order: reading the
//! pair from a hash map, as this once did, could swap the reference points
//! between two moves of the same pinch and make the reported scale jump.

use crate::gesture::state::{GestureState, Pinch};
use crate::gesture::{GestureEvent, GestureOutput};

/// Starts a pinch from the two contacts currently down.
///
/// Reports nothing if a second contact is not actually down — the caller checked,
/// but the recognizer does not depend on it having done so.
pub fn begin(state: &mut GestureState) -> GestureOutput {
    let Some((first, second)) = state.touches.pinch_pair() else {
        return GestureOutput::new();
    };

    state.pinch = Some(Pinch {
        initial_distance: first.distance_to(&second),
        scale: 1.0,
    });

    let focal = first.midpoint(&second);
    GestureOutput::once(GestureEvent::ScaleStart {
        focal_x: focal.pos.x,
        focal_y: focal.pos.y,
    })
}

/// Reports how far the two contacts have spread or closed since the pinch began.
///
/// `scale` is relative to the initial separation; `delta_scale` is relative to
/// the previous report, which is what a transform wants to apply incrementally.
pub fn update(state: &mut GestureState) -> GestureOutput {
    let Some((first, second)) = state.touches.pinch_pair() else {
        return GestureOutput::new();
    };
    let Some(pinch) = state.pinch.as_mut() else {
        return GestureOutput::new();
    };
    // Two contacts at exactly the same point have no separation to scale
    // against, and dividing by it would report an infinite scale.
    if pinch.initial_distance <= 0.0 {
        return GestureOutput::new();
    }

    let scale = first.distance_to(&second) / pinch.initial_distance;
    let delta_scale = if pinch.scale > 0.0 {
        scale / pinch.scale
    } else {
        1.0
    };
    pinch.scale = scale;

    let focal = first.midpoint(&second);
    GestureOutput::once(GestureEvent::ScaleUpdate {
        focal_x: focal.pos.x,
        focal_y: focal.pos.y,
        scale,
        delta_scale,
    })
}

/// Ends the pinch, because fewer than two contacts remain.
pub fn end(state: &mut GestureState) -> GestureOutput {
    state.clear_pinch();
    GestureOutput::once(GestureEvent::ScaleEnd)
}

#[cfg(test)]
mod tests {
    use aimer_attribute::position::Vec2d;
    use aimer_events::pointer::PointerInfo;

    use super::*;

    fn touch(x: f32, y: f32, id: u64) -> PointerInfo {
        PointerInfo::touch(Vec2d { x, y }, id)
    }

    fn pinching(first: PointerInfo, second: PointerInfo) -> GestureState {
        let mut state = GestureState::default();
        state.touches.insert(first);
        state.touches.insert(second);
        state
    }

    #[test]
    fn a_pinch_begins_at_the_midpoint_of_the_two_contacts() {
        let mut state = pinching(touch(0.0, 0.0, 1), touch(100.0, 40.0, 2));

        let output = begin(&mut state);

        assert_eq!(
            output.first(),
            Some(GestureEvent::ScaleStart {
                focal_x: 50.0,
                focal_y: 20.0,
            })
        );
        assert_eq!(
            state.pinch.map(|pinch| pinch.scale),
            Some(1.0),
            "a pinch starts at its own scale"
        );
    }

    #[test]
    fn a_pinch_cannot_begin_with_one_contact() {
        let mut state = GestureState::default();
        state.touches.insert(touch(0.0, 0.0, 1));

        assert!(begin(&mut state).is_empty());
        assert!(state.pinch.is_none());
    }

    #[test]
    fn spreading_the_contacts_reports_the_scale_relative_to_the_start() {
        let mut state = pinching(touch(0.0, 0.0, 1), touch(100.0, 0.0, 2));
        begin(&mut state);

        state.touches.insert(touch(200.0, 0.0, 2));
        let doubled = update(&mut state);

        assert_eq!(
            doubled.first(),
            Some(GestureEvent::ScaleUpdate {
                focal_x: 100.0,
                focal_y: 0.0,
                scale: 2.0,
                delta_scale: 2.0,
            })
        );

        state.touches.insert(touch(300.0, 0.0, 2));
        let tripled = update(&mut state);

        assert_eq!(
            tripled.first(),
            Some(GestureEvent::ScaleUpdate {
                focal_x: 150.0,
                focal_y: 0.0,
                scale: 3.0,
                delta_scale: 1.5,
            }),
            "the delta is against the previous report, the scale against the start"
        );
    }

    // Two contacts reported at the same point give a zero initial separation;
    // scaling against it would be a division by zero.
    #[test]
    fn two_contacts_at_the_same_point_report_no_scale() {
        let mut state = pinching(touch(50.0, 50.0, 1), touch(50.0, 50.0, 2));
        begin(&mut state);

        state.touches.insert(touch(150.0, 50.0, 2));

        assert!(update(&mut state).is_empty());
    }

    #[test]
    fn updating_without_a_pinch_in_progress_reports_nothing() {
        let mut state = pinching(touch(0.0, 0.0, 1), touch(100.0, 0.0, 2));

        assert!(update(&mut state).is_empty());
    }

    #[test]
    fn ending_a_pinch_forgets_it() {
        let mut state = pinching(touch(0.0, 0.0, 1), touch(100.0, 0.0, 2));
        begin(&mut state);

        let output = end(&mut state);

        assert_eq!(output.first(), Some(GestureEvent::ScaleEnd));
        assert!(state.pinch.is_none());
    }
}
