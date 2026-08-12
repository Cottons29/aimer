use std::cell::RefCell;
use std::rc::{Rc, Weak};

use aimer_attribute::{Bounds, CacheBounds, Vec2d};
use aimer_widget::base::{BuildContext, Color, WindowHandle};

use crate::selection::session::{SelectionSession, SelectionSlot};
use crate::selection::{TextHitRegion, text_offset_at};

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
    window: WindowHandle,
}

impl TextGeometry {
    /// Creates empty geometry that repaints through `window`.
    #[inline]
    pub fn new(window: WindowHandle) -> Self {
        Self {
            bounds: CacheBounds::new(),
            regions: RefCell::new(Vec::new()),
            window,
        }
    }

    /// Reports whether `(x, y)` lands on real glyph geometry rather than
    /// merely inside the element's bounds.
    ///
    /// This is what separates the I-beam from the default cursor past the end
    /// of a short line.
    pub fn hits_glyph(&self, x: f32, y: f32) -> bool {
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
        text_offset_at(&self.regions.borrow(), x, y)
    }

    fn caret_rect(&self, offset: usize) -> Option<Bounds> {
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
