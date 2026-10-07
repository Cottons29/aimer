use super::*;

/// A decoration selected by retained native state without replacing its owner.
///
/// This is an internal integration for controls such as Button. The returned
/// decoration must remain valid for the borrow, and `revision` must distinguish
/// different current paint values. Changes to border widths must call
/// `aimer_widget::notify_element_tree_changed`; changes to clipping radii or
/// paint outsets must call `aimer_widget::notify_retained_render_structure_changed`.
/// Those notifications refresh cached ancestor layout, hit areas and render geometry.
#[doc(hidden)]
pub trait RetainedBoxDecoration {
    /// Returns the complete currently selected decoration.
    fn decoration(&self) -> &BoxDecoration;

    /// Returns the revision identifying the selected decoration.
    fn revision(&self) -> u64;
}

// Keep ordinary containers in the same pooled allocation class. Only a
// container with a retained decoration source needs these revision cells.
pub(super) struct RetainedDecorationState {
    source: Rc<dyn RetainedBoxDecoration>,
    layout_revision: std::cell::Cell<Option<u64>>,
    paint_revision: std::cell::Cell<Option<u64>>,
}

impl RetainedDecorationState {
    pub(super) fn new(source: Rc<dyn RetainedBoxDecoration>) -> Box<Self> {
        Box::new(Self {
            source,
            layout_revision: std::cell::Cell::new(None),
            paint_revision: std::cell::Cell::new(None),
        })
    }
}

impl<E: Element> RawContainer<E> {
    #[inline]
    pub(super) fn decoration(&self) -> &BoxDecoration {
        self.decoration_source.as_ref()
            .map_or(&self.box_decoration, |state| state.source.decoration())
    }

    #[inline]
    pub(super) fn mark_decoration_recorded(&self) {
        if let Some(state) = &self.decoration_source {
            state.paint_revision.set(Some(state.source.revision()));
        }
    }

    #[inline]
    pub(super) fn retained_decoration_needs_recording(&self) -> bool {
        self.decoration_source.as_ref().is_some_and(|state| {
            state.paint_revision.get() != Some(state.source.revision())
        })
    }

    #[inline]
    pub(super) fn refresh_decoration_layout(&self) {
        if let Some(state) = &self.decoration_source {
            let revision = Some(state.source.revision());
            if state.layout_revision.replace(revision) != revision {
                self.cache.invalidate();
            }
        }
    }
}
