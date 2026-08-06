//! The hold a finger must complete before it starts selecting text.
//!
//! A mouse press means exactly one thing, so it selects at once. A finger press
//! does not: the same gesture starts a scroll, a fling, a tap on a link and a
//! selection, and only what happens next tells them apart. Selecting
//! immediately would therefore steal every scroll that began on top of a
//! paragraph — which is most of them.
//!
//! So a touch press is *recorded* rather than acted upon, and it is left
//! unconsumed so an enclosing scrollable can still claim it. Only once the
//! finger has rested for [`TOUCH_SELECTION_HOLD`] without wandering further
//! than [`TOUCH_SELECTION_SLOP`] does the press become a selection — the same
//! rule [`aimer_dnd`](https://docs.rs/aimer_dnd) applies to a touch drag.
//!
//! Like the gesture recognizers in `aimer_input`, everything here is pure with
//! respect to time: nothing reads the clock, so a five-hundred-millisecond
//! threshold is exercised by handing in an instant rather than by sleeping.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use aimer_attribute::position::Vec2d;
use aimer_utils::AnimInstant;
use aimer_widget::PointerKey;

use crate::selection::SelectionPoint;
use crate::selection::session::{SelectionSession, SelectionSlot};

/// How long a finger must rest on a text before it starts selecting.
///
/// The same five hundred milliseconds `aimer_input`'s `LONG_PRESS_DURATION`
/// uses. It is restated here rather than imported because `aimer_input` depends
/// on this crate, and the two must not disagree about what a hold is.
pub(crate) const TOUCH_SELECTION_HOLD: Duration = Duration::from_millis(500);

/// How far a finger may travel during the hold and still be resting, in logical
/// pixels.
///
/// Mirrors `aimer_input`'s `TAP_SLOP`, which is Flutter's `kTouchSlop`: a
/// contact patch centimetres wide rolls as it presses, so a finger that meant
/// to stay still does not.
pub(crate) const TOUCH_SELECTION_SLOP: f32 = 18.0;

/// A touch press that has not yet earned a selection.
#[derive(Clone, Copy)]
struct PendingTouch {
    pointer: PointerKey,
    offset: usize,
    at: Vec2d,
    since: AnimInstant,
}

/// What a pending touch press has become.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TouchHold {
    /// Nothing is pending for this pointer; the caller handles the event as it
    /// otherwise would.
    Idle,
    /// The finger is resting, but not for long enough yet.
    Waiting,
    /// The finger wandered before the hold elapsed: it was a scroll, not a
    /// selection, and the press is forgotten.
    Abandoned,
    /// The hold completed. The selection starts at this offset — the one under
    /// the *press*, not under the finger now, because that is the character the
    /// user aimed at.
    Entered(usize),
}

/// The per-element gate that keeps a finger from selecting text until it has
/// rested.
///
/// One press is pending at a time: a second finger landing on the same text
/// replaces the first, which is what a user reaching for a two-finger scroll
/// means.
///
/// # Examples
///
/// ```ignore
/// let gate = TouchHoldGate::new();
/// let now = AnimInstant::now();
/// gate.press(pointer, 4, pos, now);
///
/// assert_eq!(gate.poll(pointer, pos, now), TouchHold::Waiting);
/// assert_eq!(
///     gate.poll(pointer, pos, now + TOUCH_SELECTION_HOLD),
///     TouchHold::Entered(4),
/// );
/// ```
pub(crate) struct TouchHoldGate {
    pending: Cell<Option<PendingTouch>>,
}

impl TouchHoldGate {
    /// Creates a gate with nothing pending.
    #[inline]
    pub fn new() -> Self {
        Self {
            pending: Cell::new(None),
        }
    }

    /// Records a touch press on `offset` that may become a selection.
    #[inline]
    pub fn press(&self, pointer: PointerKey, offset: usize, at: Vec2d, now: AnimInstant) {
        self.pending.set(Some(PendingTouch {
            pointer,
            offset,
            at,
            since: now,
        }));
    }

    /// Forgets whatever was pending, as a cancelled gesture must.
    #[inline]
    pub fn clear(&self) {
        self.pending.set(None);
    }

    /// Judges the pending press against where `pointer` is now.
    ///
    /// [`TouchHold::Entered`] is reported once and only once: the press is
    /// consumed by it, so the caller may act on it without guarding against a
    /// second promotion.
    pub fn poll(&self, pointer: PointerKey, pos: Vec2d, now: AnimInstant) -> TouchHold {
        let Some(pending) = self.pending.get() else {
            return TouchHold::Idle;
        };
        if pending.pointer != pointer {
            return TouchHold::Idle;
        }
        if now.duration_since(pending.since) >= TOUCH_SELECTION_HOLD {
            self.pending.set(None);
            return TouchHold::Entered(pending.offset);
        }
        let dx = pos.x - pending.at.x;
        let dy = pos.y - pending.at.y;
        if (dx * dx + dy * dy).sqrt() >= TOUCH_SELECTION_SLOP {
            self.pending.set(None);
            return TouchHold::Abandoned;
        }
        TouchHold::Waiting
    }

    /// Rewinds the pending press so a test can reach the far side of the hold
    /// without waiting for it.
    #[cfg(test)]
    pub fn backdate(&self, by: Duration) {
        if let Some(mut pending) = self.pending.get() {
            pending.since = pending.since - by;
            self.pending.set(Some(pending));
        }
    }
}

/// The word around `offset`, as a hold selects it.
///
/// A completed hold that only placed a caret would look like nothing happened,
/// so it selects the word under the finger — the behaviour every touch platform
/// has. Whitespace between words is a "word" of its own here, which is what
/// keeps a hold on a gap from selecting the paragraph.
///
/// Offsets that fall inside a character, or past the end of the text, collapse
/// to an empty range rather than panicking.
pub(crate) fn word_range_at(text: &str, offset: usize) -> std::ops::Range<usize> {
    use unicode_segmentation::UnicodeSegmentation;

    if offset > text.len() || !text.is_char_boundary(offset) {
        return offset..offset;
    }
    for (start, word) in text.split_word_bound_indices() {
        let end = start + word.len();
        if offset < end {
            return start..end;
        }
    }
    // Past the last word: the caret sits at the end of the text, so the word
    // that ends there is the one the user is pointing at.
    text.split_word_bound_indices()
        .next_back()
        .map_or(offset..offset, |(start, word)| start..start + word.len())
}

/// Opens the selection a completed hold earned, with the word under the finger
/// already highlighted.
///
/// The gesture stays open and owned by `pointer`, so the finger can go on to
/// extend the selection into the next participant.
pub(crate) fn enter_hold(
    session: &Rc<SelectionSession>,
    slot: &Rc<SelectionSlot>,
    offset: usize,
    pointer: PointerKey,
) {
    let word = word_range_at(&slot.text(), offset);
    session.begin_range(
        SelectionPoint::new(Rc::clone(slot), word.start),
        SelectionPoint::new(Rc::clone(slot), word.end),
        pointer,
    );
}

#[cfg(test)]
mod tests {
    use aimer_events::pointer::PointerSource;

    use super::*;

    fn pointer(id: u64) -> PointerKey {
        PointerKey::new(PointerSource::Touch, id)
    }

    fn at(x: f32, y: f32) -> Vec2d {
        Vec2d { x, y }
    }

    #[test]
    fn a_fresh_press_is_not_a_selection_yet() {
        let gate = TouchHoldGate::new();
        let now = AnimInstant::now();
        gate.press(pointer(0), 3, at(10.0, 10.0), now);

        assert_eq!(gate.poll(pointer(0), at(10.0, 10.0), now), TouchHold::Waiting);
    }

    #[test]
    fn resting_long_enough_enters_the_selection_at_the_pressed_offset() {
        let gate = TouchHoldGate::new();
        let now = AnimInstant::now();
        gate.press(pointer(0), 3, at(10.0, 10.0), now);

        assert_eq!(
            gate.poll(pointer(0), at(12.0, 10.0), now + TOUCH_SELECTION_HOLD),
            TouchHold::Entered(3),
            "the offset is the one under the press, not under the finger now"
        );
    }

    #[test]
    fn entering_consumes_the_press_so_it_cannot_promote_twice() {
        let gate = TouchHoldGate::new();
        let now = AnimInstant::now();
        gate.press(pointer(0), 3, at(10.0, 10.0), now);
        let later = now + TOUCH_SELECTION_HOLD;

        assert_eq!(gate.poll(pointer(0), at(10.0, 10.0), later), TouchHold::Entered(3));
        assert_eq!(gate.poll(pointer(0), at(10.0, 10.0), later), TouchHold::Idle);
    }

    #[test]
    fn a_finger_that_wanders_before_the_hold_was_scrolling() {
        let gate = TouchHoldGate::new();
        let now = AnimInstant::now();
        gate.press(pointer(0), 3, at(10.0, 10.0), now);

        assert_eq!(
            gate.poll(
                pointer(0),
                at(10.0, 10.0 + TOUCH_SELECTION_SLOP),
                now + Duration::from_millis(50)
            ),
            TouchHold::Abandoned
        );
        assert_eq!(
            gate.poll(pointer(0), at(10.0, 10.0), now + TOUCH_SELECTION_HOLD),
            TouchHold::Idle,
            "an abandoned press must not come back once the hold would have elapsed"
        );
    }

    #[test]
    fn a_wobble_within_the_slop_keeps_the_press_alive() {
        let gate = TouchHoldGate::new();
        let now = AnimInstant::now();
        gate.press(pointer(0), 3, at(10.0, 10.0), now);

        assert_eq!(
            gate.poll(pointer(0), at(14.0, 12.0), now + Duration::from_millis(100)),
            TouchHold::Waiting
        );
    }

    #[test]
    fn another_finger_does_not_promote_this_press() {
        let gate = TouchHoldGate::new();
        let now = AnimInstant::now();
        gate.press(pointer(0), 3, at(10.0, 10.0), now);

        assert_eq!(
            gate.poll(pointer(1), at(10.0, 10.0), now + TOUCH_SELECTION_HOLD),
            TouchHold::Idle
        );
        assert_eq!(
            gate.poll(pointer(0), at(10.0, 10.0), now + TOUCH_SELECTION_HOLD),
            TouchHold::Entered(3),
            "the second finger must not have consumed the first one's press"
        );
    }

    #[test]
    fn clearing_forgets_the_press() {
        let gate = TouchHoldGate::new();
        let now = AnimInstant::now();
        gate.press(pointer(0), 3, at(10.0, 10.0), now);
        gate.clear();

        assert_eq!(
            gate.poll(pointer(0), at(10.0, 10.0), now + TOUCH_SELECTION_HOLD),
            TouchHold::Idle
        );
    }

    #[test]
    fn a_hold_selects_the_word_under_the_finger() {
        assert_eq!(word_range_at("Hello brave world", 8), 6..11);
        assert_eq!(word_range_at("Hello brave world", 6), 6..11);
        assert_eq!(word_range_at("Hello brave world", 0), 0..5);
    }

    #[test]
    fn a_hold_on_a_gap_selects_only_the_gap() {
        assert_eq!(word_range_at("Hello brave world", 5), 5..6);
    }

    #[test]
    fn a_hold_at_the_very_end_selects_the_last_word() {
        let text = "Hello world";
        assert_eq!(word_range_at(text, text.len()), 6..11);
    }

    #[test]
    fn an_offset_inside_a_character_selects_nothing() {
        // The four bytes of an emoji: only `0` and `4` are boundaries.
        assert_eq!(word_range_at("🙂", 2), 2..2);
    }

    #[test]
    fn an_empty_text_selects_nothing() {
        assert_eq!(word_range_at("", 0), 0..0);
    }
}
