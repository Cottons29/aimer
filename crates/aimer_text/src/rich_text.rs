use std::cell::RefCell;
use std::rc::Rc;

use aimer_attribute::{Bounds, ResolvedSize};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_events::pointer::{PointerButton, PointerSource};
use aimer_style::{TextAlign, TextOverflow, TextStyle};
use aimer_utils::AnimInstant;
use aimer_utils::callback::{Callback, CallbackExecutor, ambient_spawner};
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, FocusNode, LayoutElement, PointerKey,
    VisitorElement, Widget,
};

use crate::paragraph::{Paragraph, display_color, geometry};
use crate::selection::SelectionPoint;
use crate::selection::cursor::HoverCursor;
use crate::selection::selectable::{Selectable, SelectionBinding, SelectionScope, TextGeometry};
use crate::selection::session::{SelectionSession, SelectionSlot};
use crate::selection::touch_hold::{TouchHold, TouchHoldGate, enter_hold};
use crate::selection::ui;
use crate::text_span::TextSpan;

/// Callback invoked with the target of an activated linked [`TextSpan`].
pub type LinkCallback = Callback<Rc<str>, ()>;

pub(crate) const DEFAULT_SELECTION_COLOR: Color = Color::Rgba(51, 153, 255, 96);

/// Displays a tree of styled [`TextSpan`] values with optional links and
/// selection.
///
/// A span's style is resolved over the widget's base [`TextStyle`]. The widget
/// defaults to the style's overflow mode, default alignment, no link callback,
/// and disabled selection. Wrapping lays text onto multiple lines; ellipsis
/// truncates the first line to the available width. Selectable text supports
/// pointer selection and the platform select-all and copy shortcuts.
///
/// # Example
///
/// ```
/// use aimer_text::RichText;
/// use aimer_text::text_span::TextSpan;
///
/// let text = RichText::new(
///     TextSpan::new("Read ").child(TextSpan::new("the guide").link("/guide")),
/// )
/// .on_link(|target| println!("open {target}"))
/// .selectable()
/// .wrapped();
/// ```
pub struct RichText {
    span: TextSpan,
    text_style: TextStyle,
    overflow: Option<TextOverflow>,
    text_align: TextAlign,
    on_link: LinkCallback,
    link_hover_color: Option<Color>,
    selectable: bool,
    selection_color: Option<Color>,
}

impl RichText {
    /// Creates rich text rooted at `span` with default base style and
    /// interaction settings.
    #[inline]
    pub fn new(span: TextSpan) -> Self {
        Self {
            span,
            text_style: TextStyle::default(),
            overflow: None,
            text_align: TextAlign::default(),
            on_link: LinkCallback::default(),
            link_hover_color: None,
            selectable: false,
            selection_color: None,
        }
    }

    /// Replaces the base style inherited by spans that do not override
    /// individual attributes.
    #[inline]
    pub fn text_style(mut self, text_style: TextStyle) -> Self {
        self.text_style = text_style;
        self
    }

    /// Sets the alignment of each laid-out line within the available width.
    #[inline]
    pub fn text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    /// Overrides overflow behavior independently of the base style.
    #[inline]
    pub fn text_overflow(mut self, text_overflow: TextOverflow) -> Self {
        self.overflow = Some(text_overflow);
        self
    }

    fn resolved_overflow(&self) -> TextOverflow {
        self.overflow.unwrap_or(self.text_style.text_overflow)
    }

    /// Configures spans to wrap onto additional lines when width is
    /// constrained.
    #[inline]
    pub fn wrapped(self) -> Self {
        self.text_overflow(TextOverflow::Wrap)
    }

    /// Configures overflowing content to truncate the first line with an
    /// ellipsis.
    #[inline]
    pub fn ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }

    /// Sets the callback invoked after a primary click completes on a linked
    /// span.
    ///
    /// The callback receives the link target stored by [`TextSpan::link`].
    /// Dragging to select text suppresses link activation.
    #[inline]
    pub fn on_link(mut self, on_link: impl Into<LinkCallback>) -> Self {
        self.on_link = on_link.into();
        self
    }

    /// Changes linked text to `color` while the mouse pointer is over it.
    pub const fn link_hover_color(mut self, color: Color) -> Self {
        self.link_hover_color = Some(color);
        self
    }

    /// Enables pointer selection plus select-all and copy keyboard shortcuts.
    pub const fn selectable(mut self) -> Self {
        self.selectable = true;
        self
    }

    /// Replaces the highlight color used for selected text.
    ///
    /// This does not by itself enable selection; call [`RichText::selectable`]
    /// as well.
    pub const fn selection_color(mut self, color: Color) -> Self {
        self.selection_color = Some(color);
        self
    }
}

impl Widget for RichText {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let spans = self.span.flatten(&self.text_style);
        let plain_text: Rc<str> = spans
            .iter()
            .map(|span| span.text.as_ref())
            .collect::<String>()
            .into();
        let scope = ctx.get_state::<SelectionScope>();
        let selectable = self.selectable || scope.is_some();
        let binding = SelectionBinding::new(
            ctx,
            Rc::clone(&plain_text),
            self.selection_color.unwrap_or(DEFAULT_SELECTION_COLOR),
        );
        let selection_color = self
            .selection_color
            .unwrap_or_else(|| binding.session.selection_color());
        RawRichText {
            paragraph: Paragraph::new(spans, self.text_align, self.resolved_overflow()),
            plain_text,
            on_link: self.on_link.clone(),
            link_hover_color: self.link_hover_color,
            selectable,
            selection_color,
            binding: RefCell::new(binding),
            link_regions: RefCell::new(Vec::new()),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
            hover_cursor: HoverCursor::new(),
            touch_hold: TouchHoldGate::new(),
            focus_node: FocusNode::new(),
        }
        .attached()
        .boxed()
    }
}

#[derive(Clone)]
struct LinkRegion {
    target: Rc<str>,
    bounds: Bounds,
}

/// The laid-out element produced by [`RichText`].
///
/// This low-level exported type participates directly in layout, drawing,
/// links, and selection. Prefer constructing [`RichText`], which resolves the
/// span tree and initializes its interaction state correctly.
pub struct RawRichText {
    paragraph: Paragraph,
    plain_text: Rc<str>,
    on_link: LinkCallback,
    link_hover_color: Option<Color>,
    selectable: bool,
    selection_color: Color,
    binding: RefCell<SelectionBinding>,
    link_regions: RefCell<Vec<LinkRegion>>,
    pressed_link: RefCell<Option<Rc<str>>>,
    hovered_link: RefCell<Option<Rc<str>>>,
    hover_cursor: HoverCursor,
    /// Keeps a finger from selecting until it has rested; a mouse never waits.
    touch_hold: TouchHoldGate,
    /// The keyboard focus of a text that owns its selection.
    ///
    /// Inside a [`SelectionArea`](crate::SelectionArea) the region is the one
    /// that holds the focus, and this node is never attached to anything.
    focus_node: FocusNode,
}

impl RawRichText {
    /// The shared geometry this element writes while drawing.
    #[inline]
    fn geometry(&self) -> Rc<TextGeometry> {
        Rc::clone(&self.binding.borrow().geometry)
    }

    /// The session this element takes part in.
    #[inline]
    fn session(&self) -> Rc<SelectionSession> {
        Rc::clone(&self.binding.borrow().session)
    }

    /// This element's registration inside the session.
    #[inline]
    fn slot(&self) -> Rc<SelectionSlot> {
        Rc::clone(&self.binding.borrow().slot)
    }

    /// Hands this element's focus to the session it owns, and returns it.
    ///
    /// Only the owner of a session has a keyboard to give: inside a region the
    /// focus is the region's, and attaching here would take it away from the
    /// element that can actually answer for the whole selection.
    fn attached(self) -> Self {
        if self.selectable && self.owns_session() {
            self.session().attach_focus_node(&self.focus_node);
        }
        self
    }

    /// Reports whether the element created its own session, which is the case
    /// outside a selection region.
    #[inline]
    fn owns_session(&self) -> bool {
        self.binding.borrow().owns_session
    }

    fn link_at(&self, x: f32, y: f32) -> Option<Rc<str>> {
        self.link_regions
            .borrow()
            .iter()
            .find(|region| {
                let b = region.bounds;
                b.x <= x && x <= b.x + b.width && b.y <= y && y <= b.y + b.height
            })
            .map(|region| region.target.clone())
    }

    fn set_hovered_link(&self, hovered_link: Option<Rc<str>>) {
        if *self.hovered_link.borrow() != hovered_link {
            *self.hovered_link.borrow_mut() = hovered_link;
            self.geometry().window().request_redraw();
        }
    }

    /// Fires the link callback for `target`.
    ///
    /// The paragraph holds no runtime handle, so an async callback goes to
    /// whichever runtime the frame is being built on.
    #[inline]
    fn execute_link(&self, target: Rc<str>) {
        self.on_link.execute(target, &ambient_spawner());
    }
}

impl VisitorElement for RawRichText {
    fn debug_name(&self) -> &'static str {
        "RawRichText"
    }
}

impl aimer_widget::Rebuildable for RawRichText {
    #[inline]
    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// Keeps the selection alive across a rebuild.
    ///
    /// A rebuilt paragraph is a brand-new element with a brand-new
    /// registration, so a selection anchored in the element being replaced would
    /// otherwise be orphaned. Adopting the registration — and pushing the new
    /// text into it, which clamps live endpoints — keeps a selection that spans
    /// this widget intact while its content changes.
    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        let Some(old) = old
            .option_any()
            .and_then(|value| value.downcast_ref::<Self>())
        else {
            return;
        };
        let adopted = {
            let old_binding = old.binding.borrow();
            if old_binding.owns_session != self.binding.borrow().owns_session {
                return;
            }
            old_binding.adopt(Rc::clone(&self.plain_text))
        };
        *self.binding.borrow_mut() = adopted;
        // The adopted session was pointed at the focus of the element being
        // replaced, which is about to be dropped; a live selection would stop
        // hearing about outside presses without this.
        if self.selectable && self.owns_session() {
            self.session().attach_focus_node(&self.focus_node);
        }
    }
}

impl EventElement for RawRichText {
    /// A text that owns its selection is focusable exactly while it holds one.
    ///
    /// Holding the keyboard focus is how such a text learns of a press it is
    /// never offered: routing hit-tests, so a press on another widget goes
    /// there and nowhere else, but it moves the focus all the same. Inside a
    /// [`SelectionArea`] the selection — and with it the keyboard — belongs to
    /// the region, which is focusable in this text's stead.
    fn focus_node(&self) -> Option<&FocusNode> {
        (self.selectable && self.owns_session() && self.binding.borrow().session.is_focused())
            .then_some(&self.focus_node)
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        let hovered_link = match event {
            ElementEvent::PointerDown(info)
            | ElementEvent::PointerUp(info)
            | ElementEvent::PointerMove(info)
                if info.source == PointerSource::Mouse =>
            {
                self.link_at(info.x(), info.y())
            }
            ElementEvent::PointerExited(PointerSource::Mouse, _) | ElementEvent::Cancel => None,
            _ => self.hovered_link.borrow().clone(),
        };
        self.set_hovered_link(hovered_link.clone());

        // The selection's knobs and callout are painted over this text, so a
        // press that grabbed one must never reach the paragraph underneath.
        if self.selectable
            && let Some(result) = ui::intercept(&self.session(), event)
        {
            return result;
        }

        let cursor_claimed = if self.selectable || !self.link_regions.borrow().is_empty() {
            let geometry = self.geometry();
            let over_glyphs = event
                .pointer()
                .is_some_and(|info| geometry.hits_glyph(info.x(), info.y()));
            self.hover_cursor.apply(
                geometry.window(),
                event,
                self.selectable,
                hovered_link.is_some(),
                over_glyphs,
            )
        } else {
            false
        };

        match event {
            ElementEvent::PointerDown(info) => {
                let pos = info.pos;
                let pointer = PointerKey::new(info.source, info.id);
                if self.selectable && info.button == PointerButton::Secondary {
                    return ui::open_context_menu(
                        &self.session(),
                        &self.slot(),
                        &self.geometry(),
                        pos,
                        pointer,
                    )
                    .into();
                }
                let target = self.link_at(pos.x, pos.y);
                *self.pressed_link.borrow_mut() = target;
                if self.selectable {
                    let geometry = self.geometry();
                    // The offset lookup snaps to the nearest glyph, so a press
                    // that never touched this text — the tree broadcasts the
                    // presses nobody took — must be told apart by its bounds
                    // first, or it would start a selection from a click far
                    // away instead of dismissing the one on screen.
                    let inside = geometry
                        .painted_bounds()
                        .is_some_and(|bounds| bounds.is_inside(pos.x, pos.y));
                    if inside && let Some(offset) = geometry.offset_at(pos.x, pos.y) {
                        if info.source == PointerSource::Touch {
                            // A finger means a scroll as often as a selection,
                            // so the press is only remembered — and left
                            // unconsumed, so an enclosing scrollable can still
                            // claim it — until the hold has been earned.
                            self.touch_hold
                                .press(pointer, offset, pos, AnimInstant::now());
                            return false.into();
                        }
                        self.session()
                            .begin(SelectionPoint::new(self.slot(), offset), pointer);
                        return EventResult::consumed().with_pointer_capture(pointer);
                    }
                    self.touch_hold.clear();
                    let session = self.session();
                    if session.active_pointer() != Some(pointer) {
                        session.clear();
                    }
                }
                self.pressed_link.borrow().is_some()
            }
            ElementEvent::PointerMove(info) if self.selectable => {
                let pos = info.pos;
                let pointer = PointerKey::new(info.source, info.id);
                match self.touch_hold.poll(pointer, pos, AnimInstant::now()) {
                    TouchHold::Entered(offset) => {
                        enter_hold(&self.session(), &self.slot(), offset, pointer);
                        // A hold that selects must not also follow the link it
                        // rested on.
                        self.pressed_link.borrow_mut().take();
                        return EventResult::consumed().with_pointer_capture(pointer);
                    }
                    // Still resting, or gone off scrolling: either way the
                    // gesture is not this element's yet.
                    TouchHold::Waiting | TouchHold::Abandoned => return false.into(),
                    TouchHold::Idle => {}
                }
                let session = self.session();
                if session.active_pointer() != Some(pointer) {
                    return cursor_claimed.into();
                }
                if session.extend_to_position(pos.x, pos.y, pointer) && session.was_dragged() {
                    self.pressed_link.borrow_mut().take();
                }
                return EventResult::consumed();
            }
            ElementEvent::PointerUp(info) => {
                let pos = info.pos;
                let pointer = PointerKey::new(info.source, info.id);
                if self.touch_hold.release_was_stationary(pointer, pos) {
                    let session = self.session();
                    session.end(pointer);
                    ui::offer_menu_after_gesture(&session, info.source);
                    self.pressed_link.borrow_mut().take();
                    return EventResult::consumed();
                }
                if let TouchHold::Entered(offset) =
                    self.touch_hold.poll(pointer, pos, AnimInstant::now())
                {
                    // Held still and let go: the word stays selected, and the
                    // link underneath is not followed.
                    let session = self.session();
                    enter_hold(&session, &self.slot(), offset, pointer);
                    session.end(pointer);
                    ui::offer_menu_after_gesture(&session, info.source);
                    self.pressed_link.borrow_mut().take();
                    return EventResult::consumed();
                }
                self.touch_hold.clear();
                let session = self.session();
                let selection_owned =
                    self.selectable && session.active_pointer() == Some(pointer);
                let dragged = if selection_owned {
                    session.extend_to_position(pos.x, pos.y, pointer);
                    let dragged = session.was_dragged();
                    session.end(pointer);
                    ui::offer_menu_after_gesture(&session, info.source);
                    dragged
                } else {
                    false
                };
                if dragged {
                    self.pressed_link.borrow_mut().take();
                    return EventResult::consumed().with_pointer_release(pointer);
                }
                let pressed = self.pressed_link.borrow_mut().take();
                let released = self.link_at(pos.x, pos.y);
                if let (Some(pressed), Some(released)) = (pressed, released)
                    && pressed == released
                {
                    self.execute_link(released);
                    let result = EventResult::consumed();
                    return if selection_owned {
                        result.with_pointer_release(pointer)
                    } else {
                        result
                    };
                }
                let result = EventResult::from(false);
                return if selection_owned {
                    result.with_pointer_release(pointer)
                } else {
                    result
                };
            }
            // Something else took the keyboard, which is what a press anywhere
            // outside this text amounts to.
            ElementEvent::FocusLost if self.selectable && self.owns_session() => {
                self.session().blur();
                false
            }
            ElementEvent::PointerExited(_, _) | ElementEvent::Cancel => {
                self.pressed_link.borrow_mut().take();
                self.touch_hold.clear();

                if matches!(event, ElementEvent::Cancel) {
                    self.session().cancel();
                }
                false
            }
            ElementEvent::KeyInput {
                key: NamedKey::Other(key),
                action,
                modifiers,
            } if self.selectable
                && self.owns_session()
                && self.session().is_focused()
                && matches!(action, KeyAction::Pressed | KeyAction::Repeat)
                && (modifiers.ctrl || modifiers.meta) =>
            {
                match key.as_str() {
                    "a" => {
                        self.session().select_all();
                        true
                    }
                    "c" => {
                        let text = self.session().selected_text();
                        if text.is_empty() {
                            return false.into();
                        }
                        let _ = aimer_native::clipboard::set_text(&text);
                        true
                    }
                    _ => false,
                }
            }
            _ => cursor_claimed,
        }
        .into()
    }
}

impl LayoutElement for RawRichText {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.paragraph.prepare(ctx).size
    }

    fn invalidate_layout(&self) {
        self.paragraph.invalidate();
    }

    fn pos_start_end(&self) -> Option<(aimer_attribute::Vec2d, aimer_attribute::Vec2d)> {
        self.geometry().bounds.pos_start_end()
    }
}

impl Drawable for RawRichText {
    fn draw(&self, ctx: &BuildContext) {
        if let Some((pointer, offset)) = self.touch_hold.poll_stationary(AnimInstant::now()) {
            enter_hold(&self.session(), &self.slot(), offset, pointer);
            self.pressed_link.borrow_mut().take();
        }
        let slot = self.slot();
        let geometry_state = self.geometry();
        slot.stamp();
        let layout = self.paragraph.prepare(ctx);
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        geometry_state.bounds.save(
            ctx.scale,
            abs_x,
            abs_y,
            layout.size.width,
            layout.size.height,
        );
        self.link_regions.borrow_mut().clear();
        geometry_state.regions.borrow_mut().clear();

        let clipped = self.paragraph.needs_clip();
        if clipped {
            ctx.canvas.save();
            ctx.canvas.set_clip(
                (0.0, 0.0).into(),
                ResolvedSize {
                    width: self.paragraph.available_width(ctx),
                    height: ctx.parent_size.height,
                },
            );
        }

        self.paragraph.draw_backgrounds(ctx, &layout);

        if self.selectable {
            geometry::hit_regions(
                &layout,
                abs_x,
                abs_y,
                ctx.scale,
                ctx.visible_rect,
                &mut geometry_state.regions.borrow_mut(),
            );
            let selection = slot.selected_range().unwrap_or(0..0);
            for run in geometry::selection_runs(&layout, selection, ctx.visible_rect) {
                ctx.canvas.fill_color_rect(
                    (run.x, run.y).into(),
                    ResolvedSize {
                        width: run.width,
                        height: run.height,
                    },
                    self.selection_color,
                    [0.0; 4],
                );
            }
        }

        {
            let hovered_link = self.hovered_link.borrow().clone();
            let mut link_regions = self.link_regions.borrow_mut();
            self.paragraph.draw_spans(
                ctx,
                &layout,
                |span| display_color(span, hovered_link.as_ref(), self.link_hover_color),
                |span, fragment| {
                    if let Some(target) = &span.link {
                        link_regions.push(LinkRegion {
                            target: target.clone(),
                            bounds: Bounds::new(
                                (abs_x + fragment.x) / ctx.scale,
                                (abs_y + fragment.baseline - fragment.ascent) / ctx.scale,
                                fragment.width / ctx.scale,
                                fragment.height / ctx.scale,
                            ),
                        });
                    }
                },
            );
        }

        self.set_hovered_link(self.link_at(ctx.cursor_pos.x, ctx.cursor_pos.y));

        if clipped {
            ctx.canvas.clear_clip();
            ctx.canvas.restore();
        }

        // Inside a region the knobs are painted once, by the region, on top of
        // every participant. A standalone text has no region to do it, so it
        // paints its own. The callout is nobody's to paint: it goes through the
        // modal host's overlay, clear of every clip.
        if self.selectable && self.owns_session() {
            let session = self.session();
            ui::track_menu(&session);
            ui::paint_handles(ctx, &session);
        }
    }
}

#[cfg(test)]
mod tests;
