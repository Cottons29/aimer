//! Stateful widget adapters for range controls.

use std::cell::Cell;
use std::rc::Rc;

use aimer_attribute::{BoxConstraint, CacheBounds};
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_events::pointer::PointerButton;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, ChildBuilder, Drawable, Element, EventElement, EventResult,
    FocusNode, LayoutElement, PointerKey, PortableWidget, Rebuildable, State, StateUpdater,
    StatefulElement, StatefulWidget, VisitorElement, Widget,
};

use super::{
    RangeSlider, RangeThumb, RangeValue, Slider, SliderKey, SliderThumb, SliderTrail,
};
use super::visuals::SliderVisualState;

struct SliderRuntime<T: RangeValue> {
    current: Cell<T>,
    active_pointer: Cell<Option<PointerKey>>,
    pressed: Cell<bool>,
    hovered: Cell<bool>,
    focused: Cell<bool>,
    focus_node: FocusNode,
    last_proposed: Cell<Option<T>>,
    visual_state: Rc<Cell<SliderVisualState>>,
    default_trail: ChildBuilder,
    default_thumb: ChildBuilder,
}

impl<T: RangeValue> SliderRuntime<T> {
    fn new(current: T) -> Self {
        let visual_state = Rc::new(Cell::new(SliderVisualState::default()));
        Self {
            current: Cell::new(current),
            active_pointer: Cell::new(None),
            pressed: Cell::new(false),
            hovered: Cell::new(false),
            focused: Cell::new(false),
            focus_node: FocusNode::new(),
            last_proposed: Cell::new(None),
            default_trail: ChildBuilder::from_widget(
                SliderTrail::new().with_visual_state(Rc::clone(&visual_state)),
            ),
            default_thumb: ChildBuilder::from_widget(
                SliderThumb::new().with_visual_state(Rc::clone(&visual_state)),
            ),
            visual_state,
        }
    }
}

struct RangeSliderRuntime<T: RangeValue> {
    lower: Cell<T>,
    upper: Cell<T>,
    active_pointer: Cell<Option<PointerKey>>,
    active_thumb: Cell<Option<RangeThumb>>,
    pressed: Cell<bool>,
    hovered: Cell<bool>,
    focused: Cell<bool>,
    focus_node: FocusNode,
    last_proposed: Cell<Option<(T, T)>>,
    visual_state: Rc<Cell<SliderVisualState>>,
    default_trail: ChildBuilder,
    default_lower_thumb: ChildBuilder,
    default_upper_thumb: ChildBuilder,
}

impl<T: RangeValue> RangeSliderRuntime<T> {
    fn new(lower: T, upper: T) -> Self {
        let visual_state = Rc::new(Cell::new(SliderVisualState::default()));
        Self {
            lower: Cell::new(lower),
            upper: Cell::new(upper),
            active_pointer: Cell::new(None),
            active_thumb: Cell::new(None),
            pressed: Cell::new(false),
            hovered: Cell::new(false),
            focused: Cell::new(false),
            focus_node: FocusNode::new(),
            last_proposed: Cell::new(None),
            default_trail: ChildBuilder::from_widget(
                SliderTrail::new().with_visual_state(Rc::clone(&visual_state)),
            ),
            default_lower_thumb: ChildBuilder::from_widget(
                SliderThumb::new().with_visual_state(Rc::clone(&visual_state)),
            ),
            default_upper_thumb: ChildBuilder::from_widget(
                SliderThumb::new().with_visual_state(Rc::clone(&visual_state)),
            ),
            visual_state,
        }
    }
}

/// Retained runtime state for a [`Slider`] widget.
pub struct SliderState<T: RangeValue = f64> {
    model: Slider<T>,
    runtime: Rc<SliderRuntime<T>>,
}

impl<T: RangeValue> SliderState<T> {
    /// Returns the current value held by the widget, including pointer and
    /// keyboard changes made since the last parent rebuild.
    #[inline]
    pub fn current_value(&self) -> T {
        self.runtime.current.get()
    }

    /// Returns whether a pointer is currently dragging this slider.
    #[inline]
    pub fn is_pressed(&self) -> bool {
        self.runtime.pressed.get()
    }

    /// Returns whether this slider currently owns keyboard focus.
    #[inline]
    pub fn is_focused(&self) -> bool {
        self.runtime.focused.get()
    }

    /// Returns whether this slider currently ignores user input.
    #[inline]
    pub fn is_disabled(&self) -> bool {
        self.model.is_disabled()
    }

    /// Returns semantic range metadata for the current widget value.
    #[inline]
    pub fn semantics(&self) -> super::RangeSemantics {
        let mut model = self.model.clone();
        let _ = model.set_value(self.current_value());
        model.semantics()
    }
}

impl<T: RangeValue> StatefulWidget for Slider<T> {
    type State = SliderState<T>;

    fn create_state(self) -> Self::State {
        let current = self.current_value();
        SliderState {
            model: self,
            runtime: Rc::new(SliderRuntime::new(current)),
        }
    }
}

impl<T: RangeValue> State<Slider<T>> for SliderState<T> {
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: Self) {
        let old_model = &self.model;
        let value_changed = old_model.current_value() != new.model.current_value();
        let domain_changed = old_model.range_bounds() != new.model.range_bounds()
            || old_model.step_value() != new.model.step_value()
            || old_model.reversed_bounds_policy_value() != new.model.reversed_bounds_policy_value();
        self.model = new.model;
        if value_changed {
            self.runtime.current.set(self.model.current_value());
        } else if domain_changed {
            let current = self.runtime.current.get();
            let current = self
                .model
                .canonical_value(current)
                .unwrap_or(self.model.current_value());
            self.runtime.current.set(current);
        }
        self.runtime.last_proposed.set(None);
        if self.model.is_disabled() {
            self.runtime.active_pointer.set(None);
            self.runtime.pressed.set(false);
            self.runtime.hovered.set(false);
            self.runtime.focused.set(false);
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        SliderSurface {
            model: self.model.clone(),
            runtime: Rc::clone(&self.runtime),
            track: self.model.track_child(),
            trail: self
                .model
                .trail_child()
                .unwrap_or_else(|| self.runtime.default_trail.clone()),
            thumb: self
                .model
                .thumb_child()
                .unwrap_or_else(|| self.runtime.default_thumb.clone()),
            visual_state: Rc::clone(&self.runtime.visual_state),
        }
    }
}

impl<T: RangeValue> Widget for Slider<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "Slider", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Slider"
    }
}

impl<T: RangeValue> PortableWidget for Slider<T> {}

struct SliderSurface<T: RangeValue> {
    model: Slider<T>,
    runtime: Rc<SliderRuntime<T>>,
    track: Option<ChildBuilder>,
    trail: ChildBuilder,
    thumb: ChildBuilder,
    visual_state: Rc<Cell<SliderVisualState>>,
}

impl<T: RangeValue> Widget for SliderSurface<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        Element::boxed(RawSlider {
            model: self.model,
            runtime: self.runtime,
            track: self.track.map(|child| child.build(ctx)),
            trail: self.trail.build(ctx),
            thumb: self.thumb.build(ctx),
            bounds: CacheBounds::new(),
            visual_inset: Cell::new(0.0),
            visual_state: self.visual_state,
        })
    }

    fn debug_name(&self) -> &'static str {
        "RawSlider"
    }
}

impl<T: RangeValue> PortableWidget for SliderSurface<T> {}

struct RawSlider<T: RangeValue> {
    model: Slider<T>,
    runtime: Rc<SliderRuntime<T>>,
    track: Option<AnyElement>,
    trail: AnyElement,
    thumb: AnyElement,
    bounds: CacheBounds,
    /// Logical inset reserved for half of the visual thumb at each endpoint.
    visual_inset: Cell<f32>,
    visual_state: Rc<Cell<SliderVisualState>>,
}

impl<T: RangeValue> RawSlider<T> {
    fn hit_test(&self, x: f32, y: f32) -> bool {
        self.bounds
            .get_bounds()
            .is_some_and(|bounds| bounds.width > 0.0 && bounds.height > 0.0)
            && self.bounds.is_inside(x, y)
    }

    fn track_position(&self, x: f32) -> Option<T> {
        let bounds = self.bounds.get_bounds()?;
        // Map pointer coordinates over the same inset track used for drawing,
        // so pressing an endpoint thumb keeps the value at the endpoint.
        let inset = self.visual_inset.get().max(0.0).min(bounds.width / 2.0);
        let track_width = (bounds.width - inset * 2.0).max(0.0);
        self.model
            .value_at_position(
                f64::from((x - bounds.x - inset).clamp(0.0, track_width)),
                f64::from(track_width),
            )
            .ok()
    }

    fn propose(&self, value: T) {
        if self.runtime.current.get() == value {
            return;
        }
        self.runtime.current.set(value);
        if self.runtime.last_proposed.replace(Some(value)) == Some(value) {
            return;
        }
        if let Some(callback) = self.model.on_change.as_ref() {
            callback(value);
        }
    }

    fn propose_at(&self, x: f32) {
        if let Some(value) = self.track_position(x)
            && let Ok(value) = self.model.canonical_value(value)
        {
            self.propose(value);
        }
    }

    fn key_action(key: &NamedKey) -> Option<SliderKey> {
        match key {
            NamedKey::ArrowLeft => Some(SliderKey::ArrowLeft),
            NamedKey::ArrowRight => Some(SliderKey::ArrowRight),
            NamedKey::ArrowUp => Some(SliderKey::ArrowUp),
            NamedKey::ArrowDown => Some(SliderKey::ArrowDown),
            NamedKey::Home => Some(SliderKey::Home),
            NamedKey::End => Some(SliderKey::End),
            NamedKey::PageUp => Some(SliderKey::PageUp),
            NamedKey::PageDown => Some(SliderKey::PageDown),
            _ => None,
        }
    }

    fn handle_key(&self, key: &NamedKey) -> EventResult {
        if self.model.is_disabled() {
            return EventResult::ignored();
        }
        let Some(key) = Self::key_action(key) else {
            return EventResult::ignored();
        };
        let mut candidate = self.model.clone();
        if candidate.set_value(self.runtime.current.get()).is_err() {
            return EventResult::ignored();
        }
        let Ok(changed) = candidate.handle_key(key) else {
            return EventResult::ignored();
        };
        if changed {
            self.propose(candidate.current_value());
        }
        EventResult::consumed().with_redraw()
    }

    fn position_px(&self, ctx: &BuildContext, size: ResolvedSize, inset: f32) -> f32 {
        let width = size.width.max(0.0);
        // Endpoint centers are inset by half the thumb width so the visual
        // thumb remains inside the slider bounds at both ends.
        let inset = inset.max(0.0).min(width / 2.0);
        let track_width = (width - inset * 2.0).max(0.0);
        let logical_width = (track_width / ctx.scale).max(0.0);
        let position = self
            .model
            .position_for_value(self.runtime.current.get(), logical_width as f64)
            .unwrap_or(0.0) as f32
            * ctx.scale;
        (inset + position).clamp(inset, width - inset)
    }

    fn thumb_inset(&self, ctx: &BuildContext, size: ResolvedSize) -> f32 {
        let child_ctx = child_context(ctx, size);
        (self.thumb.computed_size(&child_ctx).width.max(0.0) / 2.0)
            .min(size.width.max(0.0) / 2.0)
    }
}

impl<T: RangeValue> VisitorElement for RawSlider<T> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        if let Some(track) = self.track.as_ref() {
            visitor(track.as_ref());
        }
        visitor(self.trail.as_ref());
        visitor(self.thumb.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "RawSlider"
    }
}

impl<T: RangeValue> EventElement for RawSlider<T> {
    // The visual slots are composable decorations. The slider itself owns the
    // pointer gesture so an interactive child cannot steal its capture.
    fn event_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

    fn focus_node(&self) -> Option<&FocusNode> {
        (!self.model.is_disabled()).then_some(&self.runtime.focus_node)
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        match event {
            ElementEvent::PointerDown(pointer)
                if pointer.button == PointerButton::Primary
                    && !self.model.is_disabled()
                    && self.hit_test(pointer.pos.x, pointer.pos.y) =>
            {
                let key = PointerKey::new(pointer.source, pointer.id);
                self.runtime.active_pointer.set(Some(key));
                self.runtime.pressed.set(true);
                self.propose_at(pointer.pos.x);
                EventResult::consumed()
                    .with_pointer_capture(key)
                    .with_redraw()
            }
            ElementEvent::PointerMove(pointer) => {
                if self.model.is_disabled() {
                    return EventResult::ignored();
                }
                let key = PointerKey::new(pointer.source, pointer.id);
                if self.runtime.active_pointer.get() == Some(key) {
                    self.propose_at(pointer.pos.x);
                    EventResult::consumed().with_redraw()
                } else {
                    let inside = self.hit_test(pointer.pos.x, pointer.pos.y);
                    if self.runtime.hovered.replace(inside) != inside {
                        EventResult::redraw()
                    } else {
                        EventResult::ignored()
                    }
                }
            }
            ElementEvent::PointerUp(pointer)
                if self.runtime.active_pointer.get()
                    == Some(PointerKey::new(pointer.source, pointer.id)) =>
            {
                let key = PointerKey::new(pointer.source, pointer.id);
                self.propose_at(pointer.pos.x);
                self.runtime.active_pointer.set(None);
                self.runtime.pressed.set(false);
                EventResult::consumed()
                    .with_pointer_release(key)
                    .with_redraw()
            }
            ElementEvent::PointerExited(_, _) => {
                if self.runtime.hovered.replace(false) {
                    EventResult::redraw()
                } else {
                    EventResult::ignored()
                }
            }
            ElementEvent::FocusGained => {
                self.runtime.focused.set(true);
                EventResult::redraw()
            }
            ElementEvent::FocusLost => {
                self.runtime.focused.set(false);
                EventResult::redraw()
            }
            ElementEvent::KeyInput {
                key,
                action: KeyAction::Pressed | KeyAction::Repeat,
                ..
            } => self.handle_key(key),
            ElementEvent::Cancel => {
                if self.runtime.active_pointer.get().is_some() {
                    self.runtime.active_pointer.set(None);
                    self.runtime.pressed.set(false);
                    EventResult::consumed().with_redraw()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
        }
    }
}

impl<T: RangeValue> LayoutElement for RawSlider<T> {
    fn size(&self) -> Option<Size> {
        Some(Size::new(self.model.widget_width(), self.model.widget_height()))
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let requested = Size::new(self.model.widget_width(), self.model.widget_height()).resolve(
            &ResolvedSize {
                width: ctx.box_constraint.max_width,
                height: ctx.box_constraint.max_height,
            },
            ctx.scale,
        );
        ResolvedSize {
            width: requested
                .width
                .clamp(ctx.box_constraint.min_width, ctx.box_constraint.max_width),
            height: requested
                .height
                .clamp(ctx.box_constraint.min_height, ctx.box_constraint.max_height),
        }
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        let size = self.computed_size(ctx);
        let (x, y) = ctx.canvas.get_transform_translation();
        self.bounds.save(ctx.scale, x, y, size.width, size.height);
        let thumb_inset = self.thumb_inset(ctx, size);
        self.visual_inset
            .set((thumb_inset / ctx.scale.max(f32::EPSILON)).max(0.0));
        let child_ctx = child_context(ctx, size);
        if let Some(track) = self.track.as_ref() {
            let child_size = track.computed_size(&child_ctx);
            layout_child(track, &child_ctx, Vec2d {
                x: 0.0,
                y: (size.height - child_size.height) / 2.0,
            });
        }
        let trail_size = self.trail.computed_size(&child_ctx);
        layout_child(
            &self.trail,
            &child_ctx,
            Vec2d {
                x: 0.0,
                y: (size.height - trail_size.height) / 2.0,
            },
        );
        let thumb_size = self.thumb.computed_size(&child_ctx);
        let position = self.position_px(ctx, size, thumb_inset);
        layout_child(
            &self.thumb,
            &child_ctx,
            Vec2d {
                x: position - thumb_size.width / 2.0,
                y: (size.height - thumb_size.height) / 2.0,
            },
        );
        size
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.pos_start_end()
    }
}

impl<T: RangeValue> Drawable for RawSlider<T> {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let (x, y) = ctx.canvas.get_transform_translation();
        self.bounds.save(ctx.scale, x, y, size.width, size.height);
        let thumb_inset = self.thumb_inset(ctx, size);
        self.visual_inset
            .set((thumb_inset / ctx.scale.max(f32::EPSILON)).max(0.0));
        self.visual_state.set(SliderVisualState {
            disabled: self.model.is_disabled(),
            pressed: self.runtime.pressed.get(),
            focused: self.runtime.focused.get(),
        });
        let child_ctx = child_context(ctx, size);

        let track_height = (4.0 * ctx.scale).min(size.height.max(0.0));
        if size.width <= 0.0 || track_height <= 0.0 {
            return;
        }
        let center_y = (size.height - track_height) / 2.0;
        if let Some(track) = self.track.as_ref() {
            let child_size = track.computed_size(&child_ctx);
            draw_child(track, &child_ctx, Vec2d {
                x: 0.0,
                y: (size.height - child_size.height) / 2.0,
            });
        } else {
            let track_color = Color::Rgba(190, 196, 205, 255);
            ctx.canvas.fill_color_rect(
                Vec2d { x: 0.0, y: center_y },
                ResolvedSize {
                    width: size.width,
                    height: track_height,
                },
                track_color,
                [track_height / 2.0; 4],
            );
        }

        let position = self.position_px(ctx, size, thumb_inset);
        let trail_size = self.trail.computed_size(&child_ctx);
        let trail_y = (size.height - trail_size.height) / 2.0;
        ctx.canvas.save();
        ctx.canvas.set_clip(
            Vec2d {
                x: 0.0,
                y: trail_y,
            },
            ResolvedSize {
                width: position.clamp(0.0, size.width),
                height: trail_size.height.max(0.0),
            },
        );
        draw_child(
            &self.trail,
            &child_ctx,
            Vec2d {
                x: 0.0,
                y: trail_y,
            },
        );
        ctx.canvas.clear_clip();
        ctx.canvas.restore();

        let thumb_size = self.thumb.computed_size(&child_ctx);
        draw_child(
            &self.thumb,
            &child_ctx,
            Vec2d {
                x: position - thumb_size.width / 2.0,
                y: (size.height - thumb_size.height) / 2.0,
            },
        );
    }
}

impl<T: RangeValue> Rebuildable for RawSlider<T> {}
impl<T: RangeValue> PortableWidget for RawSlider<T> {}

impl<T: RangeValue> Widget for RawSlider<T> {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        Element::boxed(self)
    }

    fn debug_name(&self) -> &'static str {
        "RawSlider"
    }
}

/// Retained runtime state for a [`RangeSlider`] widget.
pub struct RangeSliderState<T: RangeValue = f64> {
    model: RangeSlider<T>,
    runtime: Rc<RangeSliderRuntime<T>>,
}

impl<T: RangeValue> RangeSliderState<T> {
    /// Returns the lower thumb value currently held by the widget.
    #[inline]
    pub fn lower(&self) -> T {
        self.runtime.lower.get()
    }

    /// Returns the upper thumb value currently held by the widget.
    #[inline]
    pub fn upper(&self) -> T {
        self.runtime.upper.get()
    }

    /// Returns both current thumb values as an inclusive range.
    #[inline]
    pub fn current_values(&self) -> std::ops::Range<T> {
        self.lower()..self.upper()
    }

    /// Returns the thumb currently selected for keyboard input.
    #[inline]
    pub fn active_thumb(&self) -> Option<RangeThumb> {
        self.runtime.active_thumb.get()
    }

    /// Returns whether a pointer is currently dragging either thumb.
    #[inline]
    pub fn is_pressed(&self) -> bool {
        self.runtime.pressed.get()
    }

    /// Returns whether this range slider currently owns keyboard focus.
    #[inline]
    pub fn is_focused(&self) -> bool {
        self.runtime.focused.get()
    }

    /// Returns whether this range slider currently ignores user input.
    #[inline]
    pub fn is_disabled(&self) -> bool {
        self.model.is_disabled()
    }

    /// Returns semantic range metadata for the current widget values.
    #[inline]
    pub fn semantics(&self) -> super::RangeSemantics {
        let mut model = self.model.clone();
        let _ = model.set_values(self.lower(), self.upper());
        model.semantics()
    }
}

impl<T: RangeValue> StatefulWidget for RangeSlider<T> {
    type State = RangeSliderState<T>;

    fn create_state(self) -> Self::State {
        let lower = self.lower();
        let upper = self.upper();
        RangeSliderState {
            model: self,
            runtime: Rc::new(RangeSliderRuntime::new(lower, upper)),
        }
    }
}

impl<T: RangeValue> State<RangeSlider<T>> for RangeSliderState<T> {
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: Self) {
        let old_model = &self.model;
        let values_changed = old_model.lower() != new.model.lower()
            || old_model.upper() != new.model.upper();
        let domain_changed = old_model.range_bounds() != new.model.range_bounds()
            || old_model.step_value() != new.model.step_value()
            || old_model.reversed_bounds_policy_value() != new.model.reversed_bounds_policy_value();
        self.model = new.model;
        if values_changed {
            self.runtime.lower.set(self.model.lower());
            self.runtime.upper.set(self.model.upper());
        } else if domain_changed {
            let lower = self
                .model
                .canonical_value(super::RangeField::LowerValue, self.runtime.lower.get())
                .unwrap_or(self.model.lower());
            let upper = self
                .model
                .canonical_value(super::RangeField::UpperValue, self.runtime.upper.get())
                .unwrap_or(self.model.upper());
            let (lower, upper) = if lower <= upper {
                (lower, upper)
            } else {
                (upper, lower)
            };
            self.runtime.lower.set(lower);
            self.runtime.upper.set(upper);
        }
        self.runtime.last_proposed.set(None);
        if self.model.is_disabled() {
            self.runtime.active_pointer.set(None);
            self.runtime.active_thumb.set(None);
            self.runtime.pressed.set(false);
            self.runtime.hovered.set(false);
            self.runtime.focused.set(false);
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        RangeSliderSurface {
            model: self.model.clone(),
            runtime: Rc::clone(&self.runtime),
            track: self.model.track_child(),
            trail: self
                .model
                .trail_child()
                .unwrap_or_else(|| self.runtime.default_trail.clone()),
            lower_thumb: self
                .model
                .lower_thumb_child()
                .unwrap_or_else(|| self.runtime.default_lower_thumb.clone()),
            upper_thumb: self
                .model
                .upper_thumb_child()
                .unwrap_or_else(|| self.runtime.default_upper_thumb.clone()),
            visual_state: Rc::clone(&self.runtime.visual_state),
        }
    }
}

impl<T: RangeValue> Widget for RangeSlider<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "RangeSlider", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "RangeSlider"
    }
}

impl<T: RangeValue> PortableWidget for RangeSlider<T> {}

struct RangeSliderSurface<T: RangeValue> {
    model: RangeSlider<T>,
    runtime: Rc<RangeSliderRuntime<T>>,
    track: Option<ChildBuilder>,
    trail: ChildBuilder,
    lower_thumb: ChildBuilder,
    upper_thumb: ChildBuilder,
    visual_state: Rc<Cell<SliderVisualState>>,
}

impl<T: RangeValue> Widget for RangeSliderSurface<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        Element::boxed(RawRangeSlider {
            model: self.model,
            runtime: self.runtime,
            track: self.track.map(|child| child.build(ctx)),
            trail: self.trail.build(ctx),
            lower_thumb: self.lower_thumb.build(ctx),
            upper_thumb: self.upper_thumb.build(ctx),
            bounds: CacheBounds::new(),
            visual_inset: Cell::new(0.0),
            visual_state: self.visual_state,
        })
    }

    fn debug_name(&self) -> &'static str {
        "RawRangeSlider"
    }
}

impl<T: RangeValue> PortableWidget for RangeSliderSurface<T> {}

struct RawRangeSlider<T: RangeValue> {
    model: RangeSlider<T>,
    runtime: Rc<RangeSliderRuntime<T>>,
    track: Option<AnyElement>,
    trail: AnyElement,
    lower_thumb: AnyElement,
    upper_thumb: AnyElement,
    bounds: CacheBounds,
    /// Logical inset shared by both visual thumbs at the range endpoints.
    visual_inset: Cell<f32>,
    visual_state: Rc<Cell<SliderVisualState>>,
}

impl<T: RangeValue> RawRangeSlider<T> {
    fn hit_test(&self, x: f32, y: f32) -> bool {
        self.bounds
            .get_bounds()
            .is_some_and(|bounds| bounds.width > 0.0 && bounds.height > 0.0)
            && self.bounds.is_inside(x, y)
    }

    fn track_value(&self, x: f32) -> Option<T> {
        let bounds = self.bounds.get_bounds()?;
        // Keep pointer-to-value conversion aligned with the inset visual
        // track; endpoint thumbs therefore do not jump on press.
        let inset = self.visual_inset.get().max(0.0).min(bounds.width / 2.0);
        let track_width = (bounds.width - inset * 2.0).max(0.0);
        self.model
            .value_at_position(
                f64::from((x - bounds.x - inset).clamp(0.0, track_width)),
                f64::from(track_width),
            )
            .ok()
    }

    fn choose_thumb(&self, x: f32) -> RangeThumb {
        let bounds = self.bounds.get_bounds();
        let inset = bounds
            .map(|bounds| self.visual_inset.get().max(0.0).min(bounds.width / 2.0))
            .unwrap_or(0.0);
        let width = bounds
            .map(|bounds| f64::from((bounds.width - inset * 2.0).max(0.0)))
            .unwrap_or(0.0);
        let position = bounds
            .map(|bounds| f64::from(x - bounds.x))
            .map(|position| (position - f64::from(inset)).clamp(0.0, width))
            .unwrap_or(0.0);
        let lower = self
            .model
            .position_for_value(self.runtime.lower.get(), width)
            .unwrap_or(0.0);
        let upper = self
            .model
            .position_for_value(self.runtime.upper.get(), width)
            .unwrap_or(0.0);
        if (position - lower).abs() <= (position - upper).abs() {
            RangeThumb::Lower
        } else {
            RangeThumb::Upper
        }
    }

    fn propose(&self, values: (T, T)) {
        if self.runtime.lower.get() == values.0 && self.runtime.upper.get() == values.1 {
            return;
        }
        self.runtime.lower.set(values.0);
        self.runtime.upper.set(values.1);
        if self.runtime.last_proposed.replace(Some(values)) == Some(values) {
            return;
        }
        if let Some(callback) = self.model.on_change.as_ref() {
            callback(values);
        }
    }

    fn propose_at(&self, x: f32, thumb: RangeThumb) {
        let Some(value) = self.track_value(x) else {
            return;
        };
        let mut candidate = self.model.clone();
        if candidate
            .set_values(self.runtime.lower.get(), self.runtime.upper.get())
            .is_err()
        {
            return;
        }
        let Ok(changed) = (match thumb {
            RangeThumb::Lower => candidate.set_lower(value),
            RangeThumb::Upper => candidate.set_upper(value),
        }) else {
            return;
        };
        if changed {
            self.propose((candidate.lower(), candidate.upper()));
        }
    }

    fn key_action(key: &NamedKey) -> Option<SliderKey> {
        RawSlider::<T>::key_action(key)
    }

    fn position_px(&self, ctx: &BuildContext, size: ResolvedSize, value: T, inset: f32) -> f32 {
        let width = size.width.max(0.0);
        // Both range thumbs share one inset, keeping their value mapping
        // aligned even when custom lower and upper thumbs have different
        // widths.
        let inset = inset.max(0.0).min(width / 2.0);
        let track_width = (width - inset * 2.0).max(0.0);
        let logical_width = (track_width / ctx.scale).max(0.0);
        let position = self
            .model
            .position_for_value(value, logical_width as f64)
            .unwrap_or(0.0) as f32
            * ctx.scale;
        (inset + position).clamp(inset, width - inset)
    }

    fn thumb_inset(&self, ctx: &BuildContext, size: ResolvedSize) -> f32 {
        let child_ctx = child_context(ctx, size);
        let lower = self.lower_thumb.computed_size(&child_ctx).width / 2.0;
        let upper = self.upper_thumb.computed_size(&child_ctx).width / 2.0;
        lower
            .max(upper)
            .max(0.0)
            .min(size.width.max(0.0) / 2.0)
    }
}

impl<T: RangeValue> VisitorElement for RawRangeSlider<T> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        if let Some(track) = self.track.as_ref() {
            visitor(track.as_ref());
        }
        visitor(self.trail.as_ref());
        visitor(self.lower_thumb.as_ref());
        visitor(self.upper_thumb.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "RawRangeSlider"
    }
}

impl<T: RangeValue> EventElement for RawRangeSlider<T> {
    fn event_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

    fn focus_node(&self) -> Option<&FocusNode> {
        (!self.model.is_disabled()).then_some(&self.runtime.focus_node)
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        match event {
            ElementEvent::PointerDown(pointer)
                if pointer.button == PointerButton::Primary
                    && !self.model.is_disabled()
                    && self.hit_test(pointer.pos.x, pointer.pos.y) =>
            {
                let key = PointerKey::new(pointer.source, pointer.id);
                let thumb = self.choose_thumb(pointer.pos.x);
                self.runtime.active_pointer.set(Some(key));
                self.runtime.active_thumb.set(Some(thumb));
                self.runtime.pressed.set(true);
                self.propose_at(pointer.pos.x, thumb);
                EventResult::consumed()
                    .with_pointer_capture(key)
                    .with_redraw()
            }
            ElementEvent::PointerMove(pointer) => {
                if self.model.is_disabled() {
                    return EventResult::ignored();
                }
                let key = PointerKey::new(pointer.source, pointer.id);
                if self.runtime.active_pointer.get() == Some(key) {
                    if let Some(thumb) = self.runtime.active_thumb.get() {
                        self.propose_at(pointer.pos.x, thumb);
                    }
                    EventResult::consumed().with_redraw()
                } else {
                    let inside = self.hit_test(pointer.pos.x, pointer.pos.y);
                    if self.runtime.hovered.replace(inside) != inside {
                        EventResult::redraw()
                    } else {
                        EventResult::ignored()
                    }
                }
            }
            ElementEvent::PointerUp(pointer)
                if self.runtime.active_pointer.get()
                    == Some(PointerKey::new(pointer.source, pointer.id)) =>
            {
                let key = PointerKey::new(pointer.source, pointer.id);
                if let Some(thumb) = self.runtime.active_thumb.get() {
                    self.propose_at(pointer.pos.x, thumb);
                }
                self.runtime.active_pointer.set(None);
                self.runtime.active_thumb.set(None);
                self.runtime.pressed.set(false);
                EventResult::consumed()
                    .with_pointer_release(key)
                    .with_redraw()
            }
            ElementEvent::PointerExited(_, _) => {
                if self.runtime.hovered.replace(false) {
                    EventResult::redraw()
                } else {
                    EventResult::ignored()
                }
            }
            ElementEvent::FocusGained => {
                self.runtime.focused.set(true);
                EventResult::redraw()
            }
            ElementEvent::FocusLost => {
                self.runtime.focused.set(false);
                EventResult::redraw()
            }
            ElementEvent::KeyInput {
                key,
                action: KeyAction::Pressed | KeyAction::Repeat,
                ..
            } => {
                if self.model.is_disabled() {
                    return EventResult::ignored();
                }
                let Some(key) = Self::key_action(key) else {
                    return EventResult::ignored();
                };
                let thumb = self.runtime.active_thumb.get().unwrap_or(RangeThumb::Lower);
                let mut candidate = self.model.clone();
                if candidate
                    .set_values(self.runtime.lower.get(), self.runtime.upper.get())
                    .is_err()
                {
                    return EventResult::ignored();
                }
                let Ok(changed) = candidate.handle_key(thumb, key) else {
                    return EventResult::ignored();
                };
                if changed {
                    self.propose((candidate.lower(), candidate.upper()));
                }
                EventResult::consumed().with_redraw()
            }
            ElementEvent::Cancel => {
                if self.runtime.active_pointer.get().is_some() {
                    self.runtime.active_pointer.set(None);
                    self.runtime.active_thumb.set(None);
                    self.runtime.pressed.set(false);
                    EventResult::consumed().with_redraw()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
        }
    }
}

impl<T: RangeValue> LayoutElement for RawRangeSlider<T> {
    fn size(&self) -> Option<Size> {
        Some(Size::new(self.model.widget_width(), self.model.widget_height()))
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let requested = Size::new(self.model.widget_width(), self.model.widget_height()).resolve(
            &ResolvedSize {
                width: ctx.box_constraint.max_width,
                height: ctx.box_constraint.max_height,
            },
            ctx.scale,
        );
        ResolvedSize {
            width: requested
                .width
                .clamp(ctx.box_constraint.min_width, ctx.box_constraint.max_width),
            height: requested
                .height
                .clamp(ctx.box_constraint.min_height, ctx.box_constraint.max_height),
        }
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        let size = self.computed_size(ctx);
        let (x, y) = ctx.canvas.get_transform_translation();
        self.bounds.save(ctx.scale, x, y, size.width, size.height);
        let thumb_inset = self.thumb_inset(ctx, size);
        self.visual_inset
            .set((thumb_inset / ctx.scale.max(f32::EPSILON)).max(0.0));
        let child_ctx = child_context(ctx, size);
        if let Some(track) = self.track.as_ref() {
            let child_size = track.computed_size(&child_ctx);
            layout_child(track, &child_ctx, Vec2d {
                x: 0.0,
                y: (size.height - child_size.height) / 2.0,
            });
        }
        let trail_size = self.trail.computed_size(&child_ctx);
        layout_child(
            &self.trail,
            &child_ctx,
            Vec2d {
                x: 0.0,
                y: (size.height - trail_size.height) / 2.0,
            },
        );
        let lower = self.position_px(ctx, size, self.runtime.lower.get(), thumb_inset);
        let upper = self.position_px(ctx, size, self.runtime.upper.get(), thumb_inset);
        for (child, value) in [
            (&self.lower_thumb, lower),
            (&self.upper_thumb, upper),
        ] {
            let child_size = child.computed_size(&child_ctx);
            layout_child(
                child,
                &child_ctx,
                Vec2d {
                    x: value - child_size.width / 2.0,
                    y: (size.height - child_size.height) / 2.0,
                },
            );
        }
        size
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.pos_start_end()
    }
}

impl<T: RangeValue> Drawable for RawRangeSlider<T> {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let (x, y) = ctx.canvas.get_transform_translation();
        self.bounds.save(ctx.scale, x, y, size.width, size.height);
        let thumb_inset = self.thumb_inset(ctx, size);
        self.visual_inset
            .set((thumb_inset / ctx.scale.max(f32::EPSILON)).max(0.0));
        self.visual_state.set(SliderVisualState {
            disabled: self.model.is_disabled(),
            pressed: self.runtime.pressed.get(),
            focused: self.runtime.focused.get(),
        });
        let child_ctx = child_context(ctx, size);
        let track_height = (4.0 * ctx.scale).min(size.height.max(0.0));
        if size.width <= 0.0 || track_height <= 0.0 {
            return;
        }
        let center_y = (size.height - track_height) / 2.0;
        if let Some(track) = self.track.as_ref() {
            let child_size = track.computed_size(&child_ctx);
            draw_child(track, &child_ctx, Vec2d {
                x: 0.0,
                y: (size.height - child_size.height) / 2.0,
            });
        } else {
            ctx.canvas.fill_color_rect(
                Vec2d { x: 0.0, y: center_y },
                ResolvedSize {
                    width: size.width,
                    height: track_height,
                },
                Color::Rgba(190, 196, 205, 255),
                [track_height / 2.0; 4],
            );
        }

        let lower = self.position_px(ctx, size, self.runtime.lower.get(), thumb_inset);
        let upper = self.position_px(ctx, size, self.runtime.upper.get(), thumb_inset);
        let trail_size = self.trail.computed_size(&child_ctx);
        let trail_y = (size.height - trail_size.height) / 2.0;
        ctx.canvas.save();
        ctx.canvas.set_clip(
            Vec2d {
                x: lower.min(upper),
                y: trail_y,
            },
            ResolvedSize {
                width: (upper - lower).abs().min(size.width.max(0.0)),
                height: trail_size.height.max(0.0),
            },
        );
        draw_child(
            &self.trail,
            &child_ctx,
            Vec2d {
                x: 0.0,
                y: trail_y,
            },
        );
        ctx.canvas.clear_clip();
        ctx.canvas.restore();

        for (child, position) in [(&self.lower_thumb, lower), (&self.upper_thumb, upper)] {
            let child_size = child.computed_size(&child_ctx);
            draw_child(
                child,
                &child_ctx,
                Vec2d {
                    x: position - child_size.width / 2.0,
                    y: (size.height - child_size.height) / 2.0,
                },
            );
        }
    }
}

impl<T: RangeValue> Rebuildable for RawRangeSlider<T> {}
impl<T: RangeValue> PortableWidget for RawRangeSlider<T> {}

impl<T: RangeValue> Widget for RawRangeSlider<T> {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        Element::boxed(self)
    }

    fn debug_name(&self) -> &'static str {
        "RawRangeSlider"
    }
}

fn layout_child(child: &AnyElement, ctx: &BuildContext, offset: Vec2d) {
    ctx.canvas.save();
    ctx.canvas.translate(offset);
    child.layout(ctx);
    ctx.canvas.restore();
}

fn child_context<'a>(ctx: &BuildContext<'a>, size: ResolvedSize) -> BuildContext<'a> {
    BuildContext {
        parent_size: size,
        box_constraint: BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: size.width.max(0.0),
            max_height: size.height.max(0.0),
        },
        ..ctx.clone()
    }
}

fn draw_child(child: &AnyElement, ctx: &BuildContext, offset: Vec2d) {
    ctx.canvas.save();
    ctx.canvas.translate(offset);
    child.draw(ctx);
    ctx.canvas.restore();
}
