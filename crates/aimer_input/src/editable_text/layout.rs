use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EditableGeometryKey {
    pub revision: u64,
    pub font_size_bits: u32,
    pub width_bits: u32,
    pub obscure: bool,
}

pub(crate) struct EditableGeometry {
    pub display: Arc<str>,
    pub text_width: f32,
    pub visual_lines: Vec<VisualLine>,
    prefix_widths: RefCell<Vec<Option<f32>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VisualLine {
    pub byte_start: usize,
    pub byte_end: usize,
    pub grapheme_start: usize,
    pub grapheme_end: usize,
    pub width: f32,
}

impl EditableGeometry {
    pub(crate) fn new(
        display: Arc<str>,
        text_width: f32,
        visual_lines: Vec<VisualLine>,
    ) -> Self {
        let count = display.graphemes(true).count();
        let mut prefix_widths = vec![None; count + 1];
        prefix_widths[0] = Some(0.0);
        prefix_widths[count] = Some(text_width);
        Self {
            display,
            text_width,
            visual_lines,
            prefix_widths: RefCell::new(prefix_widths),
        }
    }

    pub(crate) fn prefix_width(
        &self,
        offset: usize,
        measure: impl FnOnce(&str) -> f32,
    ) -> f32 {
        let offset = offset.min(self.prefix_widths.borrow().len() - 1);
        if let Some(width) = self.prefix_widths.borrow()[offset] {
            return width;
        }
        let byte = self
            .display
            .grapheme_indices(true)
            .nth(offset)
            .map_or(self.display.len(), |(byte, _)| byte);
        let width = measure(&self.display[..byte]);
        self.prefix_widths.borrow_mut()[offset] = Some(width);
        width
    }
}

pub(crate) fn wrap_visual_lines(
    text: &str,
    max_width: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Vec<VisualLine> {
    let mut lines = Vec::new();
    let mut byte_start = 0;
    let mut grapheme_start = 0;
    let mut byte_end = 0;
    let mut grapheme_end = 0;
    let mut width = 0.0;

    for (byte, grapheme) in text.grapheme_indices(true) {
        if grapheme == "\n" {
            lines.push(VisualLine {
                byte_start,
                byte_end: byte,
                grapheme_start,
                grapheme_end,
                width,
            });
            byte_start = byte + grapheme.len();
            grapheme_start = grapheme_end + 1;
            byte_end = byte_start;
            grapheme_end = grapheme_start;
            width = 0.0;
            continue;
        }

        let grapheme_width = measure(grapheme);
        if width > 0.0 && width + grapheme_width > max_width {
            lines.push(VisualLine {
                byte_start,
                byte_end,
                grapheme_start,
                grapheme_end,
                width,
            });
            byte_start = byte;
            grapheme_start = grapheme_end;
            width = 0.0;
        }
        width += grapheme_width;
        byte_end = byte + grapheme.len();
        grapheme_end += 1;
    }
    lines.push(VisualLine {
        byte_start,
        byte_end,
        grapheme_start,
        grapheme_end,
        width,
    });
    lines
}

pub(crate) fn vertical_target(
    lines: &[VisualLine],
    current: usize,
    direction: isize,
) -> usize {
    let Some(line_index) = lines.iter().position(|line| {
        current >= line.grapheme_start && current <= line.grapheme_end
    }) else {
        return current;
    };
    let target_index = line_index.saturating_add_signed(direction);
    let Some(target) = lines.get(target_index) else {
        return current;
    };
    let column = current.saturating_sub(lines[line_index].grapheme_start);
    target.grapheme_start + column.min(target.grapheme_end - target.grapheme_start)
}

#[derive(Default)]
pub(crate) struct EditableGeometryCache {
    cached: RefCell<Option<(EditableGeometryKey, Rc<EditableGeometry>)>>,
}

impl EditableGeometryCache {
    pub(crate) fn resolve(
        &self,
        key: EditableGeometryKey,
        build: impl FnOnce() -> EditableGeometry,
    ) -> Rc<EditableGeometry> {
        if let Some((cached_key, geometry)) = self.cached.borrow().as_ref()
            && *cached_key == key
        {
            return geometry.clone();
        }

        let geometry = Rc::new(build());
        self.cached.replace(Some((key, geometry.clone())));
        geometry
    }

    pub(crate) fn latest(&self) -> Option<Rc<EditableGeometry>> {
        self.cached
            .borrow()
            .as_ref()
            .map(|(_, geometry)| geometry.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Arc;

    use super::{EditableGeometry, EditableGeometryCache, EditableGeometryKey};
    use super::{vertical_target, wrap_visual_lines};

    #[test]
    fn unchanged_geometry_is_reused_and_revisions_invalidate_it() {
        let cache = EditableGeometryCache::default();
        let builds = Cell::new(0);
        let key = EditableGeometryKey {
            revision: 3,
            font_size_bits: 14.0f32.to_bits(),
            width_bits: 200.0f32.to_bits(),
            obscure: false,
        };
        let build = || {
            builds.set(builds.get() + 1);
            EditableGeometry::new(Arc::from("hello"), 40.0, Vec::new())
        };

        let first = cache.resolve(key, build);
        let second = cache.resolve(key, build);
        let measurements = Cell::new(0);
        let first_prefix = first.prefix_width(2, |_| {
            measurements.set(measurements.get() + 1);
            17.0
        });
        let second_prefix = first.prefix_width(2, |_| {
            measurements.set(measurements.get() + 1);
            99.0
        });
        let changed = cache.resolve(
            EditableGeometryKey {
                revision: 4,
                ..key
            },
            build,
        );

        assert!(std::rc::Rc::ptr_eq(&first, &second));
        assert!(!std::rc::Rc::ptr_eq(&second, &changed));
        assert_eq!(builds.get(), 2);
        assert_eq!(first_prefix, 17.0);
        assert_eq!(second_prefix, 17.0);
        assert_eq!(measurements.get(), 1);
    }

    #[test]
    fn visual_lines_wrap_softly_preserve_hard_breaks_and_move_vertically() {
        let lines = wrap_visual_lines("abcd\nef", 2.0, |_| 1.0);

        assert_eq!(
            lines
                .iter()
                .map(|line| (line.grapheme_start, line.grapheme_end))
                .collect::<Vec<_>>(),
            vec![(0, 2), (2, 4), (5, 7)]
        );
        assert_eq!(vertical_target(&lines, 3, -1), 1);
        assert_eq!(vertical_target(&lines, 1, 1), 3);
        assert_eq!(vertical_target(&lines, 3, 1), 6);
    }
}