//! The default content of a menu: one row per verb.
//!
//! `ContextMenuRows` is what [`crate::ContextMenu`] builds when it was given
//! items rather than a child, and it is a perfectly ordinary widget — so a menu
//! that wants something else can put the rows inside its own layout, or leave
//! them out entirely.
//!
//! The rows are one element rather than one element per row. A row is a
//! rectangle, a label and a highlight; measuring the labels once for the whole
//! panel is what decides the panel's width, and a press is answered from the
//! rectangles that measurement produced.

mod geometry;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use aimer_attribute::{Bounds, ResolvedSize, Vec2d};
use aimer_events::element::ElementEvent;
use aimer_macro::Rebuildable;
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutElement, PointerKey,
    VisitorElement, Widget,
};

use crate::dismiss::ContextMenuDismiss;
use crate::item::ContextMenuItem;
use crate::shape::ContextMenuShape;
use crate::style::ContextMenuStyle;

/// The rows of a context menu, laid out in the menu's shape.
///
/// # Examples
///
/// ```
/// use aimer_ctxmenu::{ContextMenuItem, ContextMenuRows, ContextMenuShape};
///
/// let rows = ContextMenuRows::new()
///     .shape(ContextMenuShape::List)
///     .items(vec![
///         ContextMenuItem::new("Copy"),
///         ContextMenuItem::new("Select All"),
///     ])
///     .on_select(|index| println!("chose row {index}"));
///
/// assert_eq!(rows.len(), 2);
/// ```
pub struct ContextMenuRows {
    items: Vec<ContextMenuItem>,
    shape: ContextMenuShape,
    style: ContextMenuStyle,
    on_select: Option<Rc<dyn Fn(usize)>>,
    dismiss: ContextMenuDismiss,
    dismiss_on_select: bool,
}

impl Default for ContextMenuRows {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl ContextMenuRows {
    /// Creates an empty set of rows in the default shape and look.
    #[inline]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            shape: ContextMenuShape::default(),
            style: ContextMenuStyle::default(),
            on_select: None,
            dismiss: ContextMenuDismiss::new(),
            dismiss_on_select: true,
        }
    }

    /// Sets the shape, and with it the default look of that shape.
    ///
    /// Call [`ContextMenuRows::style`] *after* this to keep a custom look.
    #[inline]
    pub fn shape(mut self, shape: ContextMenuShape) -> Self {
        self.shape = shape;
        self.style = ContextMenuStyle::for_shape(shape);
        self
    }

    /// Sets the look of the rows.
    #[inline]
    pub fn style(mut self, style: ContextMenuStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the rows, in the order they are drawn.
    #[inline]
    pub fn items(mut self, items: Vec<ContextMenuItem>) -> Self {
        self.items = items;
        self
    }

    /// Appends one row.
    #[inline]
    pub fn item(mut self, item: ContextMenuItem) -> Self {
        self.items.push(item);
        self
    }

    /// Sets what happens when a row is chosen, by its index.
    ///
    /// This runs *after* the chosen item's own action, so a menu may use either
    /// or both.
    #[inline]
    pub fn on_select(mut self, on_select: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(on_select));
        self
    }

    /// Closes the menu identified by `dismiss` when a row is chosen.
    #[inline]
    pub fn dismiss_with(mut self, dismiss: ContextMenuDismiss) -> Self {
        self.dismiss = dismiss;
        self
    }

    /// Controls whether choosing a row closes the menu.
    #[inline]
    pub fn dismiss_on_select(mut self, dismiss_on_select: bool) -> Self {
        self.dismiss_on_select = dismiss_on_select;
        self
    }

    /// How many rows there are.
    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether there are no rows at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Widget for ContextMenuRows {
    fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
        RawContextMenuRows {
            items: self.items.clone(),
            shape: self.shape,
            style: self.style.clone(),
            on_select: self.on_select.clone(),
            dismiss: self.dismiss.clone(),
            dismiss_on_select: self.dismiss_on_select,
            rows: RefCell::new(Vec::new()),
            label_widths: RefCell::new(Vec::new()),
            measured_scale: Cell::new(0.0),
            pressed: Cell::new(None),
            pressed_by: Cell::new(None),
            hovered: Cell::new(None),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "ContextMenuRows"
    }
}

/// The element behind [`ContextMenuRows`].
#[derive(Rebuildable)]
pub(crate) struct RawContextMenuRows {
    items: Vec<ContextMenuItem>,
    shape: ContextMenuShape,
    style: ContextMenuStyle,
    on_select: Option<Rc<dyn Fn(usize)>>,
    dismiss: ContextMenuDismiss,
    dismiss_on_select: bool,
    /// Where each row was last painted, in absolute logical coordinates —
    /// the space a pointer event arrives in.
    rows: RefCell<Vec<Bounds>>,
    /// The measured label widths, in logical pixels, and the scale they were
    /// measured at, so a frame that changes neither measures nothing.
    label_widths: RefCell<Vec<f32>>,
    measured_scale: Cell<f32>,
    pressed: Cell<Option<usize>>,
    pressed_by: Cell<Option<PointerKey>>,
    hovered: Cell<Option<usize>>,
}

impl RawContextMenuRows {
    /// The label widths in logical pixels, measured at most once per scale.
    fn label_widths(&self, ctx: &BuildContext) -> Vec<f32> {
        let scale = scale_of(ctx);
        if self.measured_scale.get() == scale {
            let cached = self.label_widths.borrow();
            if cached.len() == self.items.len() {
                return cached.clone();
            }
        }
        let font_size = self.style.label.font_size as f32 * scale;
        let widths = self
            .items
            .iter()
            .map(|item| {
                ctx.canvas.measure_text_styled(
                    item.label(),
                    font_size,
                    self.style.label.font_family,
                    self.style.label.font_style,
                    self.style.label.font_weight.numeric(),
                ) / scale
            })
            .collect::<Vec<_>>();
        *self.label_widths.borrow_mut() = widths.clone();
        self.measured_scale.set(scale);
        widths
    }

    /// The room the rows need, in physical pixels.
    fn intrinsic_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale = scale_of(ctx);
        let (width, height) = geometry::content_size(self.shape, &self.style, &self.label_widths(ctx));
        ResolvedSize {
            width: width * scale,
            height: height * scale,
        }
    }

    /// The index of the *choosable* row under an absolute logical position.
    fn enabled_at(&self, pos: Vec2d) -> Option<usize> {
        let rows = self.rows.borrow();
        let index = geometry::row_at(self.shape, &rows, pos.x, pos.y)?;
        self.items
            .get(index)
            .filter(|item| item.is_enabled())
            .map(|_| index)
    }

    /// Whether an absolute logical position landed on the rows at all.
    fn contains(&self, pos: Vec2d) -> bool {
        geometry::union(&self.rows.borrow())
            .is_some_and(|bounds| geometry::contains(bounds, pos.x, pos.y))
    }

    /// Runs the chosen row and closes the menu unless told not to.
    fn choose(&self, index: usize) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        item.run();
        if let Some(on_select) = &self.on_select {
            on_select(index);
        }
        if self.dismiss_on_select {
            self.dismiss.dismiss();
        }
    }

    /// The corner radii of one row's highlight, in physical pixels.
    ///
    /// A tiled item at either end is as round as the panel, or the wash would
    /// square off the panel's own corner. A stacked row is square, because the
    /// panel's padding already keeps it clear of the corners.
    fn row_radii(&self, index: usize, panel: ResolvedSize, scale: f32) -> [f32; 4] {
        if !self.shape.is_horizontal() {
            return [0.0; 4];
        }
        let [tl, tr, br, bl] = self.style.panel_radius(panel.width, panel.height, scale);
        let first = index == 0;
        let last = index + 1 == self.items.len();
        [
            if first { tl } else { 0.0 },
            if last { tr } else { 0.0 },
            if last { br } else { 0.0 },
            if first { bl } else { 0.0 },
        ]
    }

    #[cfg(test)]
    pub(crate) fn place_for_test(&self, rows: Vec<Bounds>) {
        *self.rows.borrow_mut() = rows;
    }
}

impl Drawable for RawContextMenuRows {
    fn draw(&self, ctx: &BuildContext) {
        if self.items.is_empty() {
            self.rows.borrow_mut().clear();
            return;
        }

        let scale = scale_of(ctx);
        let widths = self.label_widths(ctx);
        let local = geometry::row_rects(
            self.shape,
            &self.style,
            Vec2d { x: 0.0, y: 0.0 },
            &widths,
        );

        // The rows are painted in element-local physical pixels and remembered
        // in absolute logical ones: the first is what the canvas draws in, the
        // second is what a pointer event arrives in.
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        *self.rows.borrow_mut() = local
            .iter()
            .map(|rect| {
                Bounds::new(
                    rect.x + abs_x / scale,
                    rect.y + abs_y / scale,
                    rect.width,
                    rect.height,
                )
            })
            .collect();

        let panel = self.intrinsic_size(ctx);
        let font_size = self.style.label.font_size as f32 * scale;
        let metrics = ctx.canvas.measure_text_metrics_styled(
            self.items[0].label(),
            font_size,
            0.0,
            self.style.label.font_family,
            self.style.label.font_style,
            self.style.label.font_weight.numeric(),
        );
        let text_height = metrics.ascent - metrics.descent;
        // A pressed row outranks a hovered one: a finger holding a row down is
        // hovering it too, and only one wash may be drawn.
        let lit = self.pressed.get().or(self.hovered.get());

        for (index, (item, rect)) in self.items.iter().zip(&local).enumerate() {
            let pos = Vec2d {
                x: rect.x * scale,
                y: rect.y * scale,
            };
            let size = ResolvedSize {
                width: rect.width * scale,
                height: rect.height * scale,
            };
            if lit == Some(index) {
                ctx.canvas.fill_color_rect_per_corner(
                    pos,
                    size,
                    self.style.highlight_color,
                    self.row_radii(index, panel, scale),
                );
            }
            if self.shape.is_horizontal() && index > 0 {
                ctx.canvas.fill_color_rect(
                    Vec2d {
                        x: pos.x,
                        y: pos.y + size.height * 0.25,
                    },
                    ResolvedSize {
                        width: scale,
                        height: size.height * 0.5,
                    },
                    self.style.separator_color,
                    [0.0; 4],
                );
            }
            ctx.canvas.draw_text_styled(
                item.label(),
                Vec2d {
                    x: pos.x + self.style.item_padding * scale,
                    y: pos.y + (size.height - text_height) * 0.5 + metrics.ascent,
                },
                font_size,
                if item.is_enabled() {
                    self.style.label.color
                } else {
                    self.style.disabled_label_color
                },
                self.style.label.font_family,
                self.style.label.font_style,
                self.style.label.font_weight.numeric(),
            );
        }
    }
}

impl EventElement for RawContextMenuRows {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        match event {
            ElementEvent::PointerDown(info) => {
                if !self.contains(info.pos) {
                    return EventResult::ignored();
                }
                let pointer = PointerKey::new(info.source, info.id);
                self.pressed.set(self.enabled_at(info.pos));
                self.pressed_by.set(Some(pointer));
                EventResult::consumed()
                    .with_redraw()
                    .with_pointer_capture(pointer)
            }
            ElementEvent::PointerMove(info) => {
                let over = self.enabled_at(info.pos);
                let moved = self.hovered.replace(over) != over;
                // A finger sliding off the row it pressed must un-arm it, the
                // way every button does.
                if self.pressed_by.get() == Some(PointerKey::new(info.source, info.id)) {
                    return EventResult::consumed().with_redraw();
                }
                let mut result = if self.contains(info.pos) {
                    EventResult::consumed()
                } else {
                    EventResult::ignored()
                };
                if moved {
                    result = result.with_redraw();
                }
                result
            }
            ElementEvent::PointerUp(info) => {
                let pointer = PointerKey::new(info.source, info.id);
                if self.pressed_by.get() != Some(pointer) {
                    return EventResult::ignored();
                }
                self.pressed_by.set(None);
                let pressed = self.pressed.take();
                if let Some(index) = pressed.filter(|index| self.enabled_at(info.pos) == Some(*index))
                {
                    self.choose(index);
                }
                EventResult::consumed()
                    .with_redraw()
                    .with_pointer_release(pointer)
            }
            ElementEvent::Cancel => {
                self.pressed.set(None);
                self.pressed_by.set(None);
                self.hovered.set(None);
                EventResult::ignored()
            }
            _ => EventResult::ignored(),
        }
    }
}

impl LayoutElement for RawContextMenuRows {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.intrinsic_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.intrinsic_size(ctx)
    }
}

impl VisitorElement for RawContextMenuRows {
    fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn aimer_widget::Element)) {}

    fn debug_name(&self) -> &'static str {
        "ContextMenuRows"
    }
}

/// The device pixel ratio, never zero.
#[inline]
fn scale_of(ctx: &BuildContext) -> f32 {
    if ctx.scale > 0.0 { ctx.scale } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};

    use super::*;

    fn rows(items: Vec<ContextMenuItem>, dismiss: ContextMenuDismiss) -> RawContextMenuRows {
        let element = RawContextMenuRows {
            items,
            shape: ContextMenuShape::List,
            style: ContextMenuStyle::list(),
            on_select: None,
            dismiss,
            dismiss_on_select: true,
            rows: RefCell::new(Vec::new()),
            label_widths: RefCell::new(Vec::new()),
            measured_scale: Cell::new(0.0),
            pressed: Cell::new(None),
            pressed_by: Cell::new(None),
            hovered: Cell::new(None),
        };
        element.place_for_test(vec![
            Bounds::new(10.0, 10.0, 150.0, 28.0),
            Bounds::new(10.0, 38.0, 150.0, 28.0),
        ]);
        element
    }

    fn press(x: f32, y: f32) -> ElementEvent {
        ElementEvent::PointerDown(pointer_at(x, y))
    }

    fn release(x: f32, y: f32) -> ElementEvent {
        ElementEvent::PointerUp(pointer_at(x, y))
    }

    fn pointer_at(x: f32, y: f32) -> PointerInfo {
        PointerInfo::new(
            Vec2d { x, y },
            PointerSource::Mouse,
            0,
            PointerButton::Primary,
        )
    }

    #[test]
    fn choosing_a_row_runs_its_action() {
        let ran = Rc::new(Cell::new(None));
        let first = Rc::clone(&ran);
        let second = Rc::clone(&ran);
        let element = rows(
            vec![
                ContextMenuItem::new("Copy").on_select(move || first.set(Some(0))),
                ContextMenuItem::new("Paste").on_select(move || second.set(Some(1))),
            ],
            ContextMenuDismiss::new(),
        );

        assert!(element.on_event(&press(20.0, 45.0)).is_consumed());
        assert!(element.on_event(&release(20.0, 45.0)).is_consumed());

        assert_eq!(ran.get(), Some(1));
    }

    #[test]
    fn a_release_that_slid_off_the_row_runs_nothing() {
        let ran = Rc::new(Cell::new(false));
        let flag = Rc::clone(&ran);
        let element = rows(
            vec![ContextMenuItem::new("Copy").on_select(move || flag.set(true))],
            ContextMenuDismiss::new(),
        );

        let _ = element.on_event(&press(20.0, 20.0));
        let _ = element.on_event(&release(400.0, 400.0));

        assert!(!ran.get());
    }

    #[test]
    fn a_disabled_row_takes_the_press_but_runs_nothing() {
        let ran = Rc::new(Cell::new(false));
        let flag = Rc::clone(&ran);
        let element = rows(
            vec![
                ContextMenuItem::new("Paste")
                    .enabled(false)
                    .on_select(move || flag.set(true)),
            ],
            ContextMenuDismiss::new(),
        );

        assert!(
            element.on_event(&press(20.0, 20.0)).is_consumed(),
            "the panel still swallows it"
        );
        let _ = element.on_event(&release(20.0, 20.0));

        assert!(!ran.get());
    }

    #[test]
    fn a_press_that_missed_the_rows_is_left_to_the_barrier() {
        let element = rows(vec![ContextMenuItem::new("Copy")], ContextMenuDismiss::new());

        assert!(
            !element.on_event(&press(400.0, 400.0)).is_consumed(),
            "so an outside press still dismisses the menu"
        );
    }

    #[test]
    fn only_the_pointer_that_pressed_can_choose() {
        let ran = Rc::new(Cell::new(false));
        let flag = Rc::clone(&ran);
        let element = rows(
            vec![ContextMenuItem::new("Copy").on_select(move || flag.set(true))],
            ContextMenuDismiss::new(),
        );
        let _ = element.on_event(&press(20.0, 20.0));

        let other = ElementEvent::PointerUp(PointerInfo::new(
            Vec2d { x: 20.0, y: 20.0 },
            PointerSource::Touch,
            7,
            PointerButton::Primary,
        ));

        assert!(!element.on_event(&other).is_consumed());
        assert!(!ran.get(), "a second finger cannot finish someone's press");
    }

    #[test]
    fn a_cancelled_gesture_forgets_the_press() {
        let ran = Rc::new(Cell::new(false));
        let flag = Rc::clone(&ran);
        let element = rows(
            vec![ContextMenuItem::new("Copy").on_select(move || flag.set(true))],
            ContextMenuDismiss::new(),
        );

        let _ = element.on_event(&press(20.0, 20.0));
        let _ = element.on_event(&ElementEvent::Cancel);
        let _ = element.on_event(&release(20.0, 20.0));

        assert!(!ran.get());
    }

    #[test]
    fn choosing_a_row_closes_the_menu_by_default() {
        let dismiss = ContextMenuDismiss::new();
        let element = rows(vec![ContextMenuItem::new("Copy")], dismiss.clone());

        let _ = element.on_event(&press(20.0, 20.0));
        let _ = element.on_event(&release(20.0, 20.0));

        assert!(dismiss.was_asked_to_dismiss());
    }

    #[test]
    fn a_menu_told_to_stay_open_is_not_dismissed_by_a_choice() {
        let dismiss = ContextMenuDismiss::new();
        let element = RawContextMenuRows {
            dismiss_on_select: false,
            ..rows(vec![ContextMenuItem::new("Select All")], dismiss.clone())
        };

        let _ = element.on_event(&press(20.0, 20.0));
        let _ = element.on_event(&release(20.0, 20.0));

        assert!(!dismiss.was_asked_to_dismiss());
    }

    #[test]
    fn the_index_callback_hears_which_row_was_chosen() {
        let chosen = Rc::new(Cell::new(None));
        let seen = Rc::clone(&chosen);
        let element = RawContextMenuRows {
            on_select: Some(Rc::new(move |index| seen.set(Some(index)))),
            ..rows(
                vec![
                    ContextMenuItem::new("Copy"),
                    ContextMenuItem::new("Select All"),
                ],
                ContextMenuDismiss::new(),
            )
        };

        let _ = element.on_event(&press(20.0, 45.0));
        let _ = element.on_event(&release(20.0, 45.0));

        assert_eq!(chosen.get(), Some(1));
    }
}
