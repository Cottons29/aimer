use std::cell::RefCell;
use std::ops::Range;
use std::rc::{Rc, Weak};

use aimer_attribute::{Bounds, CacheBounds, Vec2d};
use aimer_widget::base::{BuildContext, Color, WindowHandle};

use crate::selection::session::{SelectionSession, SelectionSlot};
use crate::selection::{TextHitRegion, text_offset_at};

use aimer_cupid::text_layout::TextInteractionLayout;
use aimer_cupid::utilities::Mat3;
use crate::TextAccessibilitySnapshot;

/// The geometry side of a selection participant.
///
/// A [`crate::selection::SelectionSlot`] owns the participant's *text*, so this
/// trait covers only what needs the laid-out paragraph: where the participant
/// painted, and which offset a position maps to.
///
/// Implementors must be reachable through an [`Rc`] that the element keeps
/// alive, because elements themselves are stored inline in their owner and may
/// move.
pub(crate) trait Selectable {
    /// Absolute logical bounds from the last frame, or `None` if never drawn.
    fn painted_bounds(&self) -> Option<Bounds>;

    /// The top-left corner of those bounds, or the origin if never drawn.
    ///
    /// A touch press is a finger resting on a *glyph*, so it records this and
    /// every later judgement compares it against the same corner: a participant
    /// that has since moved was scrolled out from under the finger, whether or
    /// not the finger itself ever reported a move. See
    /// [`TouchHoldGate::forget_if_content_moved`](crate::selection::touch_hold::TouchHoldGate::forget_if_content_moved).
    #[inline]
    fn painted_origin(&self) -> Vec2d {
        self.painted_bounds().map_or(Vec2d::ZERO, |bounds| Vec2d {
            x: bounds.x,
            y: bounds.y,
        })
    }

    /// The offset nearest `(x, y)`, given in absolute logical coordinates.
    fn offset_at(&self, x: f32, y: f32) -> Option<usize>;

    /// The zero-width caret rectangle at `offset`, in absolute logical
    /// coordinates.
    ///
    /// This is where a selection handle attaches, so it must be the *edge* of
    /// the grapheme rather than its box: the leading edge of the grapheme that
    /// starts there, or the trailing edge of the last one when the offset sits
    /// at the very end of the text.
    fn caret_rect(&self, offset: usize) -> Option<Bounds>;
}

/// Shared, per-frame geometry of one selectable text element.
///
/// The element writes into it while drawing and reads from it while handling
/// events; the session reads it through a [`Weak`] reference, which keeps the
/// element free to move without invalidating the registration.
///
/// # Examples
///
/// ```ignore
/// let geometry = Rc::new(TextGeometry::new(window));
/// geometry.bounds.save(scale, x, y, width, height);
/// assert!(geometry.painted_bounds().is_some());
/// ```
pub(crate) struct TextGeometry {
    /// Absolute logical bounds of the last painted frame.
    pub bounds: CacheBounds,
    /// Per-grapheme hit regions of the last painted frame, in absolute logical
    /// coordinates.
    pub regions: RefCell<Vec<TextHitRegion>>,
    interaction: RefCell<Option<InteractionSnapshot>>,
    window: WindowHandle,
}

struct InteractionSnapshot {
    layout: TextInteractionLayout,
    transform: Mat3,
    scale: f32,
}

impl TextGeometry {
    /// Creates empty geometry that repaints through `window`.
    #[inline]
    pub fn new(window: WindowHandle) -> Self {
        Self {
            bounds: CacheBounds::new(),
            regions: RefCell::new(Vec::new()),
            interaction: RefCell::new(None),
            window,
        }
    }

    /// Replaces the optional shared Aimer layout used by selection queries.
    pub fn set_interaction_layout(
        &self,
        layout: Option<TextInteractionLayout>,
        transform: Mat3,
        scale: f32,
    ) {
        *self.interaction.borrow_mut() = layout
            .filter(|_| transform.inverse_transform_point(0.0, 0.0).is_some())
            .map(|layout| InteractionSnapshot {
                layout,
                transform,
                scale,
            });
    }

    /// Returns a portable text accessibility snapshot of the last painted
    /// Aimer layout, if one exists.
    pub(crate) fn accessibility_snapshot(
        &self,
        selection: Option<Range<usize>>,
    ) -> Option<TextAccessibilitySnapshot> {
        let snapshot = self.interaction.borrow();
        let snapshot = snapshot.as_ref()?;
        TextAccessibilitySnapshot::from_layout(
            snapshot.layout.clone(),
            snapshot.transform,
            snapshot.scale,
        )
        .map(|snapshot| snapshot.with_selection(selection))
    }

    /// Saves the logical axis-aligned bounds of a paragraph after its canvas
    /// transform has been applied.
    pub fn save_painted_bounds(&self, scale: f32, transform: Mat3, width: f32, height: f32) {
        if transform.inverse_transform_point(0.0, 0.0).is_none() {
            self.bounds.set_bounds(Bounds::new(0.0, 0.0, 0.0, 0.0));
            return;
        }
        let bounds = transformed_rect_bounds(&transform, 0.0, 0.0, width, height);
        let scale = valid_device_scale(scale);
        self.bounds.set_bounds(Bounds::new(
            bounds.x / scale,
            bounds.y / scale,
            bounds.width / scale,
            bounds.height / scale,
        ));
    }

    /// Reports whether a pointer is inside the transformed paragraph box.
    #[inline]
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        if let Some(snapshot) = self.interaction.borrow().as_ref() {
            let scale = valid_device_scale(snapshot.scale);
            let Some((local_x, local_y)) = snapshot
                .transform
                .inverse_transform_point(x * scale, y * scale)
            else {
                return false;
            };
            let left = snapshot.layout.origin_x;
            let top = snapshot.layout.origin_y;
            return local_x >= left
                && local_x <= left + snapshot.layout.metrics.width
                && local_y >= top
                && local_y <= top + snapshot.layout.metrics.height;
        }

        self.painted_bounds()
            .is_some_and(|bounds| bounds.is_inside(x, y))
    }

    /// Reports whether `(x, y)` lands on real glyph geometry rather than
    /// merely inside the element's bounds.
    ///
    /// This is what separates the I-beam from the default cursor past the end
    /// of a short line.
    pub fn hits_glyph(&self, x: f32, y: f32) -> bool {
        if let Some(snapshot) = self.interaction.borrow().as_ref() {
            let scale = valid_device_scale(snapshot.scale);
            let Some((local_x, local_y)) = snapshot
                .transform
                .inverse_transform_point(x * scale, y * scale)
            else {
                return false;
            };
            return snapshot.layout.clusters.iter().any(|cluster| {
                if cluster.text_range.start == cluster.text_range.end
                    || snapshot
                        .layout
                        .text
                        .get(cluster.text_range.clone())
                        .is_some_and(|text| text == "\n" || text == "\r\n")
                {
                    return false;
                }
                if matches!(
                    snapshot.layout.writing_mode,
                    aimer_cupid::text_layout::TextWritingMode::VerticalRl
                ) {
                    let left = cluster.start_x - cluster.height * 0.5;
                    let right = cluster.start_x + cluster.height * 0.5;
                    let top = cluster.start_y.min(cluster.end_y);
                    let bottom = cluster.start_y.max(cluster.end_y);
                    local_x >= left
                        && local_x <= right
                        && local_y >= top
                        && local_y <= bottom
                } else {
                    let left = cluster.start_x.min(cluster.end_x);
                    let right = cluster.start_x.max(cluster.end_x);
                    let top = cluster.y;
                    let bottom = cluster.y + cluster.height;
                    local_x >= left
                        && local_x <= right
                        && local_y >= top
                        && local_y <= bottom
                }
            });
        }

        self.regions.borrow().iter().any(|region| {
            let bounds = region.bounds;
            bounds.x <= x
                && x <= bounds.x + bounds.width
                && bounds.y <= y
                && y <= bounds.y + bounds.height
        })
    }

    /// The window this participant paints into.
    #[inline]
    pub fn window(&self) -> &WindowHandle {
        &self.window
    }
}

impl Selectable for TextGeometry {
    #[inline]
    fn painted_bounds(&self) -> Option<Bounds> {
        self.bounds.get_bounds()
    }

    #[inline]
    fn offset_at(&self, x: f32, y: f32) -> Option<usize> {
        if let Some(snapshot) = self.interaction.borrow().as_ref() {
            let scale = valid_device_scale(snapshot.scale);
            let (local_x, local_y) = snapshot
                .transform
                .inverse_transform_point(x * scale, y * scale)?;
            return snapshot.layout.hit_test(local_x, local_y);
        }
        text_offset_at(&self.regions.borrow(), x, y)
    }

    fn caret_rect(&self, offset: usize) -> Option<Bounds> {
        if let Some(snapshot) = self.interaction.borrow().as_ref() {
            let caret = snapshot.layout.caret_geometry(offset)?;
            let bounds = transformed_rect_bounds(
                &snapshot.transform,
                caret.x,
                caret.y,
                caret.width,
                caret.height,
            );
            let scale = valid_device_scale(snapshot.scale);
            return Some(Bounds::new(
                bounds.x / scale,
                bounds.y / scale,
                bounds.width / scale,
                bounds.height / scale,
            ));
        }
        let regions = self.regions.borrow();
        if let Some(region) = regions
            .iter()
            .find(|region| region.source_range.start == offset)
        {
            let bounds = region.bounds;
            return Some(Bounds::new(bounds.x, bounds.y, 0.0, bounds.height));
        }
        let region = regions
            .iter()
            .rfind(|region| region.source_range.end <= offset)?;
        let bounds = region.bounds;
        Some(Bounds::new(
            bounds.x + bounds.width,
            bounds.y,
            0.0,
            bounds.height,
        ))
    }
}

#[inline]
fn valid_device_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > f32::EPSILON {
        scale
    } else {
        1.0
    }
}

fn transformed_rect_bounds(
    transform: &Mat3,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Bounds {
    let points = [
        transform.transform_point(x, y),
        transform.transform_point(x + width, y),
        transform.transform_point(x, y + height),
        transform.transform_point(x + width, y + height),
    ];
    let (min_x, max_x) = points
        .iter()
        .map(|point| point.0)
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
            (min.min(value), max.max(value))
        });
    let (min_y, max_y) = points
        .iter()
        .map(|point| point.1)
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
            (min.min(value), max.max(value))
        });
    Bounds::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// The ambient selection session installed by a selection region.
///
/// Text widgets look this state up while building; finding it is what makes
/// them join the region instead of owning a private selection.
pub(crate) struct SelectionScope(pub Rc<SelectionSession>);

/// Arbitrates between selection sessions inside one widget tree.
///
/// Only one session may hold a selection at a time, so claiming clears the
/// previous one. This is what makes clicking a standalone text drop a region's
/// selection, and the other way round.
#[derive(Default)]
pub(crate) struct SelectionCoordinator {
    current: RefCell<Weak<SelectionSession>>,
}

impl SelectionCoordinator {
    /// Grants the selection to `session`, clearing whichever session held it.
    pub fn claim(&self, session: &Rc<SelectionSession>) {
        let previous = self.current.borrow().upgrade();
        if previous
            .as_ref()
            .is_some_and(|previous| Rc::ptr_eq(previous, session))
        {
            return;
        }
        if let Some(previous) = previous {
            previous.clear();
        }
        *self.current.borrow_mut() = Rc::downgrade(session);
    }

    /// The session currently holding the selection, if it is still alive.
    #[cfg(test)]
    pub fn current(&self) -> Option<Rc<SelectionSession>> {
        self.current.borrow().upgrade()
    }
}

/// Returns the tree-wide coordinator, inserting it on first use.
pub(crate) fn selection_coordinator(ctx: &BuildContext) -> Rc<SelectionCoordinator> {
    if let Some(coordinator) = ctx.get_state::<SelectionCoordinator>() {
        return coordinator;
    }
    ctx.insert_state(SelectionCoordinator::default());
    ctx.get_state::<SelectionCoordinator>()
        .expect("selection coordinator was just inserted")
} 

/// Everything a selectable text keeps alive to take part in a selection.
///
/// It lives behind one [`std::cell::RefCell`] so a rebuilt element can adopt the
/// whole registration from the element it replaces, which is what keeps a
/// selection alive across a rebuild.
pub(crate) struct SelectionBinding {
    pub geometry: Rc<TextGeometry>,
    pub session: Rc<SelectionSession>,
    pub slot: Rc<SelectionSlot>,
    /// `true` when the element created the session itself, which is the case
    /// outside a selection region.
    pub owns_session: bool,
}

impl SelectionBinding {
    /// Registers `text` with the ambient session of `ctx`, or with a fresh
    /// private one when the element sits outside a selection region.
    pub fn new(ctx: &BuildContext, text: Rc<str>, fallback_color: Color) -> Self {
        let window = ctx.window.clone();
        let (session, owns_session) = match ctx.get_state::<SelectionScope>() {
            Some(scope) => (Rc::clone(&scope.0), false),
            None => (
                SelectionSession::new(window.clone(), selection_coordinator(ctx), fallback_color),
                true,
            ),
        };
        let geometry = Rc::new(TextGeometry::new(window));
        let slot = session.register(text, Rc::downgrade(&geometry) as _);
        Self {
            geometry,
            session,
            slot,
            owns_session,
        }
    }

    /// Clones the registration of the element being replaced and refreshes its
    /// text, which clamps any live endpoint inside it.
    pub fn adopt(&self, text: Rc<str>) -> Self {
        self.slot.set_text(text);
        Self {
            geometry: Rc::clone(&self.geometry),
            session: Rc::clone(&self.session),
            slot: Rc::clone(&self.slot),
            owns_session: self.owns_session,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Selectable, TextGeometry};
    use aimer_attribute::Bounds;
    use aimer_cupid::text_layout::{
        ParagraphMetrics, TextCluster, TextInteractionLayout, TextLine, TextWritingMode,
    };
    use aimer_cupid::utilities::Mat3;
    use aimer_widget::base::WindowHandle;
    use winit::dpi::PhysicalSize;

    fn interaction_layout() -> TextInteractionLayout {
        TextInteractionLayout {
            text: "a".to_owned(),
            lines: vec![TextLine {
                text_range: 0..1,
                glyph_range: 0..0,
                baseline: 16.0,
                width: 10.0,
                ascent: 12.0,
                descent: -4.0,
                line_gap: 0.0,
                hard_break: false,
            }],
            clusters: vec![TextCluster {
                text_range: 0..1,
                line_index: 0,
                level: unicode_bidi::Level::ltr(),
                start_x: 0.0,
                end_x: 10.0,
                start_y: 16.0,
                end_y: 16.0,
                y: 4.0,
                height: 20.0,
            }],
            metrics: ParagraphMetrics {
                width: 10.0,
                height: 20.0,
                ascent: 12.0,
                descent: -4.0,
                line_gap: 0.0,
                line_height: 20.0,
                line_count: 1,
            },
            origin_x: 0.0,
            origin_y: 0.0,
            writing_mode: TextWritingMode::HorizontalTb,
        }
    }

    #[test]
    fn aimer_interaction_maps_pointer_and_caret_through_the_canvas_transform() {
        let geometry = TextGeometry::new(WindowHandle::headless(PhysicalSize::new(200, 200), 1.0));
        let transform = Mat3::translate(20.0, 30.0).mul(&Mat3::scale(2.0, 3.0));
        geometry.save_painted_bounds(1.0, transform, 10.0, 20.0);
        geometry.set_interaction_layout(Some(interaction_layout()), transform, 1.0);

        assert_eq!(
            geometry.painted_bounds(),
            Some(Bounds::new(20.0, 30.0, 20.0, 60.0))
        );
        assert!(geometry.contains_point(24.0, 54.0));
        assert!(!geometry.contains_point(41.0, 30.0));
        assert!(geometry.hits_glyph(24.0, 54.0));
        assert_eq!(Selectable::offset_at(&geometry, 24.0, 54.0), Some(0));
        assert_eq!(
            Selectable::caret_rect(&geometry, 1),
            Some(Bounds::new(40.0, 42.0, 0.0, 60.0))
        );
        let accessibility = geometry
            .accessibility_snapshot(Some(0..1))
            .expect("the painted interaction layout is exposed to accessibility");
        assert_eq!(accessibility.text(), "a");
        assert_eq!(accessibility.selection(), Some(0..1));
        assert_eq!(
            accessibility.selected_rects(),
            vec![crate::TextAccessibilitySelectionRect {
                line_index: 0,
                bounds: Bounds::new(20.0, 42.0, 20.0, 60.0),
            }]
        );
    }
}
