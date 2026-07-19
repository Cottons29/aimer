use std::ops::Range;

use aimer_attribute::Bounds;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextHitRegion {
    pub source_range: Range<usize>,
    pub bounds: Bounds,
}

impl TextHitRegion {
    pub const fn new(source_range: Range<usize>, bounds: Bounds) -> Self {
        Self { source_range, bounds }
    }
}

pub(crate) fn text_offset_at(regions: &[TextHitRegion], x: f32, y: f32) -> Option<usize> {
    let region = regions
        .iter()
        .min_by(|left, right| {
            vertical_distance(left.bounds, y)
                .total_cmp(&vertical_distance(right.bounds, y))
                .then_with(|| {
                    distance_squared(left.bounds, x, y).total_cmp(&distance_squared(
                        right.bounds,
                        x,
                        y,
                    ))
                })
        })?;
    let midpoint = region
        .bounds
        .x
        + region
            .bounds
            .width
            / 2.0;
    Some(if x < midpoint {
        region
            .source_range
            .start
    } else {
        region
            .source_range
            .end
    })
}

fn vertical_distance(bounds: Bounds, y: f32) -> f32 {
    if y < bounds.y {
        bounds.y - y
    } else if y > bounds.y + bounds.height {
        y - (bounds.y + bounds.height)
    } else {
        0.0
    }
}

fn distance_squared(bounds: Bounds, x: f32, y: f32) -> f32 {
    let dx = if x < bounds.x {
        bounds.x - x
    } else if x > bounds.x + bounds.width {
        x - (bounds.x + bounds.width)
    } else {
        0.0
    };
    let dy = if y < bounds.y {
        bounds.y - y
    } else if y > bounds.y + bounds.height {
        y - (bounds.y + bounds.height)
    } else {
        0.0
    };
    dx * dx + dy * dy
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    anchor: usize,
    focus: usize,
}

impl TextSelection {
    pub const fn new(anchor: usize, focus: usize) -> Self {
        Self { anchor, focus }
    }

    pub const fn collapsed(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    pub const fn anchor(self) -> usize {
        self.anchor
    }

    pub const fn focus(self) -> usize {
        self.focus
    }

    pub fn range(self) -> Range<usize> {
        self.anchor
            .min(self.focus)
            ..self
                .anchor
                .max(self.focus)
    }

    pub const fn is_collapsed(self) -> bool {
        self.anchor == self.focus
    }

    pub fn selected_text(self, text: &str) -> Option<&str> {
        text.get(self.range())
    }
}

#[derive(Debug, Default)]
pub(crate) struct SelectionState {
    selection: TextSelection,
    selection_before_gesture: Option<TextSelection>,
    active_pointer: Option<u64>,
    dragged: bool,
}

impl SelectionState {
    pub fn begin(&mut self, offset: usize, pointer: u64) {
        self.selection_before_gesture = Some(self.selection);
        self.selection = TextSelection::collapsed(offset);
        self.active_pointer = Some(pointer);
        self.dragged = false;
    }

    pub fn update(&mut self, offset: usize, pointer: u64) -> bool {
        if self.active_pointer != Some(pointer) {
            return false;
        }
        self.selection = TextSelection::new(
            self.selection
                .anchor(),
            offset,
        );
        self.dragged |= !self
            .selection
            .is_collapsed();
        true
    }

    pub fn end(&mut self, pointer: u64) -> bool {
        if self.active_pointer != Some(pointer) {
            return false;
        }
        self.active_pointer = None;
        self.selection_before_gesture = None;
        true
    }

    pub fn cancel(&mut self) {
        if let Some(selection) = self
            .selection_before_gesture
            .take()
        {
            self.selection = selection;
        }
        self.active_pointer = None;
        self.dragged = false;
    }

    pub fn clear(&mut self) {
        self.selection = TextSelection::default();
        self.selection_before_gesture = None;
        self.active_pointer = None;
        self.dragged = false;
    }

    pub fn select_all(&mut self, text_len: usize) {
        self.selection = TextSelection::new(0, text_len);
        self.selection_before_gesture = None;
        self.active_pointer = None;
        self.dragged = false;
    }

    pub const fn selection(&self) -> TextSelection {
        self.selection
    }

    pub const fn is_active(&self) -> bool {
        self.active_pointer
            .is_some()
    }

    pub const fn active_pointer(&self) -> Option<u64> {
        self.active_pointer
    }

    pub const fn was_dragged(&self) -> bool {
        self.dragged
    }
}

#[cfg(test)]
mod tests {
    use aimer_attribute::Bounds;

    use super::{SelectionState, TextHitRegion, TextSelection, text_offset_at};

    #[test]
    fn reversed_selection_normalizes_without_losing_direction() {
        let selection = TextSelection::new(8, 2);

        assert_eq!(selection.anchor(), 8);
        assert_eq!(selection.focus(), 2);
        assert_eq!(selection.range(), 2..8);
        assert!(!selection.is_collapsed());
    }

    #[test]
    fn collapsed_selection_has_an_empty_range() {
        let selection = TextSelection::collapsed(4);

        assert_eq!(selection.range(), 4..4);
        assert!(selection.is_collapsed());
    }

    #[test]
    fn selected_text_preserves_unicode_and_line_breaks() {
        let text = "Aé\n👩‍💻Z";
        let selection = TextSelection::new(1, text.len() - 1);

        assert_eq!(selection.selected_text(text), Some("é\n👩‍💻"));
    }

    #[test]
    fn selected_text_rejects_non_utf8_boundaries() {
        let selection = TextSelection::new(1, 2);

        assert_eq!(selection.selected_text("é"), None);
    }

    #[test]
    fn hit_testing_chooses_the_nearest_grapheme_boundary() {
        let regions = vec![
            TextHitRegion::new(0..2, Bounds::new(10.0, 20.0, 10.0, 12.0)),
            TextHitRegion::new(2..3, Bounds::new(20.0, 20.0, 10.0, 12.0)),
        ];

        assert_eq!(text_offset_at(&regions, 14.0, 25.0), Some(0));
        assert_eq!(text_offset_at(&regions, 16.0, 25.0), Some(2));
        assert_eq!(text_offset_at(&regions, 26.0, 25.0), Some(3));
    }

    #[test]
    fn hit_testing_clamps_outside_a_line_to_its_nearest_edge() {
        let regions = vec![
            TextHitRegion::new(0..1, Bounds::new(10.0, 20.0, 10.0, 12.0)),
            TextHitRegion::new(1..2, Bounds::new(20.0, 20.0, 10.0, 12.0)),
        ];

        assert_eq!(text_offset_at(&regions, -100.0, 25.0), Some(0));
        assert_eq!(text_offset_at(&regions, 100.0, 25.0), Some(2));
    }

    #[test]
    fn hit_testing_below_short_final_line_reaches_end_of_text() {
        let regions = vec![
            TextHitRegion::new(0..1, Bounds::new(10.0, 20.0, 100.0, 10.0)),
            TextHitRegion::new(1..2, Bounds::new(10.0, 30.0, 10.0, 10.0)),
        ];

        assert_eq!(text_offset_at(&regions, 200.0, 50.0), Some(2));
    }

    #[test]
    fn hit_testing_above_short_first_line_reaches_start_of_text() {
        let regions = vec![
            TextHitRegion::new(0..1, Bounds::new(100.0, 20.0, 10.0, 10.0)),
            TextHitRegion::new(1..2, Bounds::new(10.0, 30.0, 100.0, 10.0)),
        ];

        assert_eq!(text_offset_at(&regions, -100.0, 10.0), Some(0));
    }

    #[test]
    fn selection_drag_tracks_only_the_pointer_that_started_it() {
        let mut state = SelectionState::default();

        state.begin(8, 42);
        assert!(!state.update(2, 7));
        assert_eq!(state.selection(), TextSelection::collapsed(8));
        assert!(state.update(2, 42));
        assert_eq!(state.selection(), TextSelection::new(8, 2));
        assert!(state.was_dragged());
        assert!(!state.end(7));
        assert!(state.end(42));
        assert!(!state.is_active());
    }

    #[test]
    fn select_all_uses_the_complete_visible_utf8_range() {
        let text = "Aé\n👩‍💻";
        let mut state = SelectionState::default();

        state.select_all(text.len());

        assert_eq!(state.selection(), TextSelection::new(0, text.len()));
        assert_eq!(
            state
                .selection()
                .selected_text(text),
            Some(text)
        );
    }

    #[test]
    fn cancelled_drag_restores_selection_from_before_pointer_down() {
        let mut state = SelectionState::default();
        state.select_all(12);

        state.begin(3, 7);
        assert!(state.update(9, 7));
        state.cancel();

        assert_eq!(state.selection(), TextSelection::new(0, 12));
        assert!(!state.is_active());
        assert!(!state.was_dragged());
    }

    #[test]
    fn ended_drag_commits_new_selection() {
        let mut state = SelectionState::default();
        state.select_all(12);

        state.begin(3, 7);
        assert!(state.update(9, 7));
        assert!(state.end(7));
        state.cancel();

        assert_eq!(state.selection(), TextSelection::new(3, 9));
        assert!(!state.is_active());
    }

    #[test]
    fn clear_removes_selection_and_active_pointer() {
        let mut state = SelectionState::default();
        state.select_all(12);
        state.begin(3, 7);
        assert!(state.update(9, 7));

        state.clear();

        assert_eq!(state.selection(), TextSelection::default());
        assert!(!state.is_active());
        assert!(!state.was_dragged());
    }
}
