use std::cell::{Cell, UnsafeCell};
use std::marker::PhantomData;
use std::panic::Location;
use std::time::Duration;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, ChildBuilder, Drawable, Element, EventElement, EventResult, Key, LayoutElement,
    PaintDamageTracker, Rebuildable, State, StateUpdater, StatefulElement, StatefulWidget,
    VisitorElement, Widget,
};

use crate::control::controller::AnimationController;
use crate::local_cell::LocalCell;
use crate::primitives::animatable::Animatable;
use crate::primitives::curve::Curve;
use crate::primitives::time::AnimInstant;

// ---------------------------------------------------------------------------
// RGBA color interpolation support
// ---------------------------------------------------------------------------

/// Normalized RGBA color (each component 0.0–1.0) for smooth interpolation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Convert from `aimer_color::Color` to normalized RGBA.
    pub fn from_color(color: &Color) -> Self {
        let argb = color.as_u32();
        let a = ((argb >> 24) & 0xFF) as f32 / 255.0;
        let r = ((argb >> 16) & 0xFF) as f32 / 255.0;
        let g = ((argb >> 8) & 0xFF) as f32 / 255.0;
        let b = (argb & 0xFF) as f32 / 255.0;
        Self { r, g, b, a }
    }

    /// Convert back to `aimer_color::Color::Rgba`.
    pub fn to_color(self) -> Color {
        Color::Rgba(
            (self.r * 255.0).clamp(0.0, 255.0) as u8,
            (self.g * 255.0).clamp(0.0, 255.0) as u8,
            (self.b * 255.0).clamp(0.0, 255.0) as u8,
            (self.a * 255.0).clamp(0.0, 255.0) as u8,
        )
    }
}

impl Animatable for Rgba {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

// ---------------------------------------------------------------------------
// MorphTransition widget
// ---------------------------------------------------------------------------

/// Automatically morphs between old and new child content.
///
/// When the child widget changes on rebuild, `MorphTransition` captures the
/// old child's layout (size, position) and generates a smooth transition:
///
/// - **Shape**: old child scales/fades out while new child scales in from old
///   size
/// - **Position**: new child slides from the old position to its new position
/// - **Color**: if `background_color` is set, it interpolates between old and
///   new colors
/// - **Text**: old text fades out while new text fades in (cross-fade)
///
/// Child widgets should provide distinct keys; use
/// [`MorphTransition::child_key`] when the child type does not expose one
/// itself.
///
/// # Example
/// ```rust
/// use std::time::Duration;
///
/// use aimer_animation::{Curve, MorphTransition, Rgba};
/// use aimer_widget::ErrorWidget;
///
/// let transition =
///     MorphTransition::new(Duration::from_millis(400),
///                          Curve::Linear,
///                          ErrorWidget::new("Profile card")).background_color(Rgba::WHITE)
///                                                           .child_key("profile-card");
/// ```
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(id = "aimer_animation::MorphTransition", schema_only)]
pub struct MorphTransition<T: Widget + 'static> {
    /// The morphing content, kept as a builder because a cross-fade places the
    /// same content again on every frame of the transition.
    #[portable_child]
    pub child: ChildBuilder,
    #[portable_skip]
    pub duration: Duration,
    #[portable_skip]
    pub curve: Curve,
    /// Optional background color to morph. If set, the color transitions
    /// from the old value to this value when the child changes.
    #[portable_skip]
    pub background_color: Option<Rgba>,
    #[portable_skip]
    transition_key: Option<Key>,
    #[portable_skip]
    widget_key: Option<Key>,
    /// Records which child type completed this transition without storing it,
    /// so one `State` impl stays paired with one child type.
    #[portable_skip]
    marker: PhantomData<T>,
}

impl<T: Widget> MorphTransition<T> {
    /// Creates a morph transition around `child`.
    ///
    /// The initial child is displayed without animation. Later rebuilds start
    /// a morph when the effective child key or optional background color
    /// changes. The child's own [`Widget::key`] is the default identity.

    pub fn new(duration: Duration, curve: Curve, child: T) -> Self {
        Self {
            child: ChildBuilder::from_widget(child),
            duration,
            curve,
            background_color: None,
            transition_key: None,
            widget_key: None,
            marker: PhantomData,
        }
    }

    /// Sets the background color included in the morph.
    ///
    /// A changed color starts a transition even when the child key is
    /// unchanged. Without this builder, no background is painted or
    /// interpolated by the transition.
    #[inline]
    pub fn background_color(mut self, color: Rgba) -> Self {
        self.background_color = Some(color);
        self
    }

    /// Sets an explicit child identity used to detect content changes.
    ///
    /// This overrides the key reported by the child widget.
    #[inline]
    #[track_caller]
    pub fn child_key(mut self, key: impl Into<Key>) -> Self {
        let caller = Location::caller();
        self.transition_key = Some(key.into().with_location(caller));
        self
    }

    /// Sets the identity of the transition widget for reconciliation.
    ///
    /// This does not determine whether a morph starts; use
    /// [`child_key`](Self::child_key) for that purpose.
    #[track_caller]
    #[inline]
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        let caller = Location::caller();
        self.widget_key = Some(key.into().with_location(caller));
        self
    }
}

impl<T: Widget + 'static> StatefulWidget for MorphTransition<T> {
    type State = MorphTransitionState<T>;

    fn create_state(self) -> Self::State {
        MorphTransitionState {
            child_key: self
                .transition_key
                .or_else(|| Widget::key(&self.child)),
            current_child: self.child,
            old_child: None,
            duration: self.duration,
            curve: self.curve,
            current_color: self.background_color,
            old_color: None,
            controller: AnimationController::new(self.duration, self.curve),
            updater: StateUpdater::empty(),
            marker: PhantomData,
        }
    }
}

impl<T: Widget + 'static> Widget for MorphTransition<T> {
    fn key(&self) -> Option<Key> {
        self.widget_key.clone()
    }

    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let __key = Widget::key(&self);
        StatefulElement::new_with_name(self, ctx, "MorphTransition", __key)
            .0
            .boxed()
    }
}

#[doc(hidden)]
pub struct MorphTransitionState<T: Widget + 'static> {
    current_child: ChildBuilder,
    old_child: Option<ChildBuilder>,
    child_key: Option<Key>,
    duration: Duration,
    curve: Curve,
    current_color: Option<Rgba>,
    old_color: Option<Rgba>,
    controller: AnimationController,
    updater: StateUpdater<Self>,
    marker: PhantomData<T>,
}

impl<T: Widget + 'static> State<MorphTransition<T>> for MorphTransitionState<T> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: Self) {
        self.duration = new.duration;
        self.curve = new.curve;
        self.controller.set_duration(self.duration);
        self.controller.set_curve(self.curve);

        if self.child_key != new.child_key || self.current_color != new.current_color {
            self.old_child = Some(self.current_child.clone());
            self.old_color = self.current_color;
            self.current_child = new.current_child;
            self.child_key = new.child_key;
            self.current_color = new.current_color;
            self.controller.reset();
            self.controller.forward();
        } else {
            self.current_child = new.current_child;
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        MorphTransitionFrame {
            current_child: self.current_child.clone(),
            old_child: if self.controller.is_animating() {
                self.old_child.clone()
            } else {
                None
            },
            current_color: self.current_color,
            old_color: if self.controller.is_animating() {
                self.old_color
            } else {
                None
            },
            controller: self.controller.clone(),
        }
    }
}

struct MorphTransitionFrame {
    current_child: ChildBuilder,
    old_child: Option<ChildBuilder>,
    current_color: Option<Rgba>,
    old_color: Option<Rgba>,
    controller: AnimationController,
}

#[derive(Clone, Copy)]
struct MorphBackgroundPaint {
    size: ResolvedSize,
    color: Rgba,
}

struct MorphBackgroundChild {
    element: AnyElement,
    paint: std::rc::Rc<Cell<MorphBackgroundPaint>>,
}

struct MorphBackgroundWidget {
    paint: std::rc::Rc<Cell<MorphBackgroundPaint>>,
}

struct MorphBackgroundElement {
    paint: std::rc::Rc<Cell<MorphBackgroundPaint>>,
    recorded_signature: Cell<Option<[u32; 6]>>,
}

impl MorphBackgroundWidget {
    fn new(ctx: &BuildContext, paint: MorphBackgroundPaint) -> MorphBackgroundChild {
        let paint = std::rc::Rc::new(Cell::new(paint));
        let element = Self {
            paint: std::rc::Rc::clone(&paint),
        }
        .to_element(ctx);
        MorphBackgroundChild { element, paint }
    }
}

impl Widget for MorphBackgroundWidget {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        MorphBackgroundElement {
            paint: self.paint,
            recorded_signature: Cell::new(None),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "MorphBackground"
    }
}

impl aimer_widget::PortableWidget for MorphBackgroundWidget {}

impl MorphBackgroundElement {
    #[inline]
    fn paint_value(&self) -> MorphBackgroundPaint {
        self.paint.get()
    }

    #[inline]
    fn signature(paint: MorphBackgroundPaint) -> [u32; 6] {
        [
            paint.size.width.to_bits(),
            paint.size.height.to_bits(),
            paint.color.r.to_bits(),
            paint.color.g.to_bits(),
            paint.color.b.to_bits(),
            paint.color.a.to_bits(),
        ]
    }
}

impl Drawable for MorphBackgroundElement {
    fn draw(&self, ctx: &BuildContext) {
        let paint = self.paint_value();
        ctx.canvas
            .fill_color_rect(Vec2d::ZERO, paint.size, paint.color.to_color(), [0.0; 4]);
    }

    fn can_paint_local_v2(&self, _ctx: &BuildContext) -> bool {
        true
    }

    fn paint_local_v2(&self, ctx: &BuildContext) {
        let paint = self.paint_value();
        let canvas = aimer_canvas::Canvas::of(ctx);
        canvas.fill_color_rect(Vec2d::ZERO, paint.size, paint.color.to_color());
        canvas.finish();
        self.recorded_signature.set(Some(Self::signature(paint)));
    }

    fn local_v2_paint_needs_recording(&self, _ctx: &BuildContext) -> bool {
        self.recorded_signature.get() != Some(Self::signature(self.paint_value()))
    }

    fn is_paint_bounded(&self) -> bool {
        true
    }
}

impl VisitorElement for MorphBackgroundElement {
    fn debug_name(&self) -> &'static str {
        "MorphBackgroundElement"
    }
}

impl EventElement for MorphBackgroundElement {
    fn on_event(&self, _event: &ElementEvent) -> EventResult {
        EventResult::ignored()
    }
}

impl Rebuildable for MorphBackgroundElement {}

impl LayoutElement for MorphBackgroundElement {
    fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
        self.paint_value().size
    }

    fn content_size(&self, _ctx: &BuildContext) -> ResolvedSize {
        self.paint_value().size
    }

    fn is_layout_stable(&self) -> bool {
        true
    }
}

impl Widget for MorphTransitionFrame {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let current_child = self.current_child.build(ctx);
        let current_size = current_child.computed_size(ctx);
        let old_child = self.old_child.map(|child| child.build(ctx));
        let old_size = old_child
            .as_ref()
            .map(|child| child.computed_size(ctx))
            .unwrap_or(current_size);
        let morphing = old_child.is_some();
        let has_background_color = self.current_color.is_some() || self.old_color.is_some();
        let old_background = (morphing && has_background_color).then(|| {
            MorphBackgroundWidget::new(
                ctx,
                MorphBackgroundPaint {
                    size: old_size,
                    color: self.old_color.unwrap_or(Rgba::TRANSPARENT),
                },
            )
        });
        let new_background = (morphing && has_background_color).then(|| {
            MorphBackgroundWidget::new(
                ctx,
                MorphBackgroundPaint {
                    size: current_size,
                    color: self.current_color.unwrap_or(Rgba::TRANSPARENT),
                },
            )
        });

        MorphTransitionElement {
            current_child: SyncChild::new(current_child),
            old_child: SyncChild(UnsafeCell::new(old_child)),
            old_background: UnsafeCell::new(old_background),
            new_background: UnsafeCell::new(new_background),
            controller: self.controller,
            window: ctx.window.clone(),
            old_snapshot: LocalCell::new(LayoutSnapshot {
                size: (old_size.width, old_size.height),
                position: (0.0, 0.0),
                color: self.old_color.unwrap_or(Rgba::TRANSPARENT),
            }),
            new_snapshot: LocalCell::new(LayoutSnapshot {
                size: (current_size.width, current_size.height),
                position: (0.0, 0.0),
                color: self.current_color.unwrap_or(Rgba::TRANSPARENT),
            }),
            has_background_color,
            morph_state: Cell::new(if morphing {
                MorphState::MorphingIn
            } else {
                MorphState::Idle
            }),
            damage: PaintDamageTracker::new(),
        }
        .boxed()
    }
}

impl aimer_widget::PortableWidget for MorphTransitionFrame {}

/// Snapshot of a child's layout properties at a point in time.
#[derive(Debug, Clone)]
struct LayoutSnapshot {
    size: (f32, f32),
    position: (f32, f32),
    color: Rgba,
}

/// The current morph animation state.
#[derive(Debug, Clone, Copy, PartialEq)]
enum MorphState {
    /// No animation running.
    Idle,
    /// Morphing from old child to new child.
    MorphingIn,
}

/// Unsafe wrapper for single-threaded mutable access to an element.
/// Safety: the rendering pipeline is single-threaded.
struct SyncChild(UnsafeCell<Option<AnyElement>>);
unsafe impl Send for SyncChild {}
unsafe impl Sync for SyncChild {}

impl SyncChild {
    fn new(element: AnyElement) -> Self {
        Self(UnsafeCell::new(Some(element)))
    }

    /// # Safety
    /// Must only be called from the single rendering thread.
    unsafe fn get(&self) -> Option<&dyn Element> {
        unsafe { (*self.0.get()).as_ref().map(|b| b.as_ref()) }
    }

    /// Take the element out, leaving `None` in its place.
    /// # Safety
    /// Must only be called from the single rendering thread.
    unsafe fn take(&self) -> Option<AnyElement> {
        unsafe { (*self.0.get()).take() }
    }
}

// ---------------------------------------------------------------------------
// MorphTransitionElement
// ---------------------------------------------------------------------------

struct MorphTransitionElement {
    current_child: SyncChild,
    old_child: SyncChild,
    old_background: UnsafeCell<Option<MorphBackgroundChild>>,
    new_background: UnsafeCell<Option<MorphBackgroundChild>>,
    controller: AnimationController,
    window: WindowHandle,
    old_snapshot: LocalCell<LayoutSnapshot>,
    new_snapshot: LocalCell<LayoutSnapshot>,
    has_background_color: bool,
    morph_state: Cell<MorphState>,
    damage: PaintDamageTracker,
}

// Safety: rendering pipeline is single-threaded
unsafe impl Send for MorphTransitionElement {}
unsafe impl Sync for MorphTransitionElement {}

impl MorphTransitionElement {
    /// Compute the interpolated layout between old and new snapshots.
    fn interpolated_layout(&self, t: f32) -> LayoutSnapshot {
        self.old_snapshot.with(|old| {
            self.new_snapshot.with(|new| LayoutSnapshot {
                size: Animatable::lerp(&old.size, &new.size, t),
                position: Animatable::lerp(&old.position, &new.position, t),
                color: Animatable::lerp(&old.color, &new.color, t),
            })
        })
    }
}

impl Drawable for MorphTransitionElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();

        let curved_value = self.controller.tick(now);
        let is_animating = self.controller.is_animating();

        let morph_state = self.morph_state.get();

        crate::widgets::damage::mark_dynamic_animation_damage(
            &self.damage,
            is_animating || morph_state != MorphState::Idle,
        );

        match morph_state {
            MorphState::Idle => {
                // No morph in progress — draw the current child normally.
                unsafe {
                    if let Some(child) = self.current_child.get() {
                        child.update(ctx);
                    }
                }
            }
            MorphState::MorphingIn => {
                let layout = self.interpolated_layout(curved_value);
                let new_size = unsafe {
                    self.current_child
                        .get()
                        .map(|c| c.computed_size(ctx))
                        .unwrap_or(ResolvedSize {
                            width: 0.0,
                            height: 0.0,
                        })
                };
                let scale_x = if new_size.width > 0.01 {
                    layout.size.0 / new_size.width
                } else {
                    1.0
                };
                let scale_y = if new_size.height > 0.01 {
                    layout.size.1 / new_size.height
                } else {
                    1.0
                };

                // --- Phase 1: Draw old child fading out (first half) ---
                if curved_value < 0.5 {
                    unsafe {
                        if let Some(old) = self.old_child.get() {
                            let old_alpha = 1.0 - curved_value * 2.0;
                            let old_snap = self.old_snapshot.with(Clone::clone);
                            let old_size = old_snap.size;

                            ctx.canvas.save();

                            if self.has_background_color {
                                let bg = old_snap.color;
                                let overlay_alpha = bg.a * old_alpha;
                                if overlay_alpha > 0.001 {
                                    let overlay_color =
                                        Rgba::new(bg.r, bg.g, bg.b, overlay_alpha).to_color();
                                    ctx.canvas.fill_color_rect(
                                        (0.0, 0.0).into(),
                                        ResolvedSize {
                                            width: old_size.0,
                                            height: old_size.1,
                                        },
                                        overlay_color,
                                        [0.0; 4],
                                    );
                                }
                            }

                            ctx.canvas.set_alpha(old_alpha);
                            old.update(ctx);
                            ctx.canvas.restore();
                        }
                    }
                }

                // --- Phase 2: Draw new child morphing in (second half) ---
                let new_alpha = if curved_value < 0.5 {
                    curved_value * 2.0
                } else {
                    1.0
                };

                let sx = lerp_f32(scale_x, 1.0, curved_value);
                let sy = lerp_f32(scale_y, 1.0, curved_value);
                let cx = new_size.width / 2.0;
                let cy = new_size.height / 2.0;

                unsafe {
                    if let Some(child) = self.current_child.get() {
                        ctx.canvas.save();

                        if self.has_background_color {
                            let bg = layout.color;
                            let overlay_alpha = bg.a * new_alpha;
                            if overlay_alpha > 0.001 {
                                let overlay_color =
                                    Rgba::new(bg.r, bg.g, bg.b, overlay_alpha).to_color();
                                ctx.canvas.fill_color_rect(
                                    (0.0, 0.0).into(),
                                    ResolvedSize {
                                        width: new_size.width,
                                        height: new_size.height,
                                    },
                                    overlay_color,
                                    [0.0; 4],
                                );
                            }
                        }

                        ctx.canvas.translate((cx, cy).into());
                        ctx.canvas.scale(sx, sy);
                        ctx.canvas.translate((-cx, -cy).into());

                        ctx.canvas.set_alpha(new_alpha);
                        child.update(ctx);
                        ctx.canvas.restore();
                    }
                }
            }
        }

        if is_animating {
            self.window.request_redraw();
        } else if morph_state == MorphState::MorphingIn {
            let _ = unsafe { self.old_child.take() };
            self.morph_state.set(MorphState::Idle);
        }
    }

    #[inline]
    fn can_paint_local_v2(&self, _ctx: &BuildContext) -> bool {
        true
    }

    #[inline]
    fn paint_local_v2(&self, _ctx: &BuildContext) {}

    fn sync_local_v2_state(&self, ctx: &BuildContext) -> bool {
        if unsafe { self.current_child.get() }.is_some_and(|child| !child.is_paint_bounded())
            || unsafe { self.old_child.get() }.is_some_and(|child| !child.is_paint_bounded())
        {
            return false;
        }
        let progress = self.controller.tick(AnimInstant::now());
        if !progress.is_finite() {
            return false;
        }

        if self.morph_state.get() == MorphState::Idle {
            return ctx.set_local_v2_child_presentation_at(
                0,
                aimer_canvas::Mat3::identity(),
                1.0,
            );
        }

        if unsafe { self.old_child.get() }.is_none() {
            return false;
        }
        let old_snapshot = self.old_snapshot.with(Clone::clone);
        let layout = self.interpolated_layout(progress);
        let new_size = unsafe {
            self.current_child
                .get()
                .map(|child| child.computed_size(ctx))
                .unwrap_or(ResolvedSize {
                    width: 0.0,
                    height: 0.0,
                })
        };
        let scale_x = if new_size.width > 0.01 {
            layout.size.0 / new_size.width
        } else {
            1.0
        };
        let scale_y = if new_size.height > 0.01 {
            layout.size.1 / new_size.height
        } else {
            1.0
        };
        let sx = lerp_f32(scale_x, 1.0, progress);
        let sy = lerp_f32(scale_y, 1.0, progress);
        let cx = new_size.width / 2.0;
        let cy = new_size.height / 2.0;
        if ![sx, sy, cx, cy].into_iter().all(f32::is_finite) {
            return false;
        }

        if let Some(background) = unsafe { (&*self.old_background.get()).as_ref() } {
            background.paint.set(MorphBackgroundPaint {
                size: ResolvedSize {
                    width: old_snapshot.size.0,
                    height: old_snapshot.size.1,
                },
                color: old_snapshot.color,
            });
        }
        if let Some(background) = unsafe { (&*self.new_background.get()).as_ref() } {
            background.paint.set(MorphBackgroundPaint {
                size: new_size,
                color: layout.color,
            });
        }

        let old_alpha = if progress < 0.5 {
            (1.0 - progress * 2.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let new_alpha = if progress < 0.5 {
            (progress * 2.0).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let identity = aimer_canvas::Mat3::identity();
        let mut child_index = 0;
        if self.has_background_color {
            if !ctx.set_local_v2_child_presentation_at(child_index, identity, 1.0) {
                return false;
            }
            child_index += 1;
        }
        if !ctx.set_local_v2_child_presentation_at(child_index, identity, old_alpha) {
            return false;
        }
        child_index += 1;
        if self.has_background_color {
            if !ctx.set_local_v2_child_presentation_at(child_index, identity, 1.0) {
                return false;
            }
            child_index += 1;
        }
        let transform = aimer_canvas::Mat3::translate(cx, cy)
            .mul(&aimer_canvas::Mat3::scale(sx, sy))
            .mul(&aimer_canvas::Mat3::translate(-cx, -cy));
        ctx.set_local_v2_child_presentation_at(child_index, transform, new_alpha)
    }

    fn draw_local_v2_compatibility(&self, ctx: &BuildContext) {
        if self.morph_state.get() == MorphState::MorphingIn {
            unsafe {
                if let Some(background) = (&*self.old_background.get()).as_ref() {
                    background.element.update(ctx);
                }
                if let Some(old) = self.old_child.get() {
                    old.update(ctx);
                }
                if let Some(background) = (&*self.new_background.get()).as_ref() {
                    background.element.update(ctx);
                }
                if let Some(child) = self.current_child.get() {
                    child.update(ctx);
                }
            }
        } else {
            unsafe {
                if let Some(child) = self.current_child.get() {
                    child.update(ctx);
                }
            }
        }

        if self.controller.is_animating() {
            self.window.request_redraw();
        } else if self.morph_state.get() == MorphState::MorphingIn {
            let _ = unsafe { self.old_child.take() };
            unsafe {
                let _ = (&mut *self.old_background.get()).take();
                let _ = (&mut *self.new_background.get()).take();
            }
            self.morph_state.set(MorphState::Idle);
        }
    }
}

impl VisitorElement for MorphTransitionElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        unsafe {
            if let Some(child) = self.current_child.get() {
                visitor(child);
            }
            if let Some(old) = self.old_child.get() {
                visitor(old);
            }
        }
    }

    fn visit_retained_v2_children<'a>(
        &'a self,
        visitor: &mut dyn FnMut(usize, &'a dyn Element),
    ) {
        let mut index = 0;
        if self.morph_state.get() == MorphState::MorphingIn {
            unsafe {
                if let Some(background) = (&*self.old_background.get()).as_ref() {
                    visitor(index, background.element.as_ref());
                    index += 1;
                }
                if let Some(old) = self.old_child.get() {
                    visitor(index, old);
                    index += 1;
                }
                if let Some(background) = (&*self.new_background.get()).as_ref() {
                    visitor(index, background.element.as_ref());
                    index += 1;
                }
            }
        }
        unsafe {
            if let Some(current) = self.current_child.get() {
                visitor(index, current);
            }
        }
    }

    fn debug_name(&self) -> &'static str {
        "MorphTransitionElement"
    }
}

impl EventElement for MorphTransitionElement {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        unsafe {
            self.current_child
                .get()
                .map(|c| c.on_event(event))
                .unwrap_or_else(EventResult::ignored)
        }
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        unsafe {
            if let Some(child) = self.current_child.get() {
                visitor(child);
            }
        }
    }
}

impl Rebuildable for MorphTransitionElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        unsafe {
            if let Some(child) = self.current_child.get() {
                child.rebuild_if_dirty(ctx);
            }
            if let Some(old) = self.old_child.get() {
                old.rebuild_if_dirty(ctx);
            }
        }
    }
}

impl LayoutElement for MorphTransitionElement {
    fn pos(&self) -> Option<Vec2d> {
        unsafe { self.current_child.get().and_then(|c| c.pos()) }
    }

    fn size(&self) -> Option<Size> {
        unsafe { self.current_child.get().and_then(|c| c.size()) }
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe {
            self.current_child
                .get()
                .map(|c| c.computed_size(ctx))
                .unwrap_or(ResolvedSize {
                    width: 0.0,
                    height: 0.0,
                })
        }
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe {
            self.current_child
                .get()
                .map(|c| c.content_size(ctx))
                .unwrap_or(ResolvedSize {
                    width: 0.0,
                    height: 0.0,
                })
        }
    }

    fn get_size_from_child(&self) -> Option<Size> {
        unsafe {
            self.current_child
                .get()
                .and_then(|c| c.get_size_from_child())
        }
    }

    fn invalidate_layout(&self) {
        unsafe {
            if let Some(child) = self.current_child.get() {
                child.invalidate_layout();
            }
        }
    }
}

/// Linear interpolation helper.
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestWidget(&'static str);

    impl Widget for TestWidget {
        fn key(&self) -> Option<Key> {
            Some(Key::Value(self.0.to_owned()))
        }

        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            panic!("not needed for state lifecycle tests")
        }
    }

    impl aimer_widget::PortableWidget for TestWidget {}

    fn state(key: &'static str, color: Rgba) -> MorphTransitionState<TestWidget> {
        MorphTransition::new(Duration::from_millis(100), Curve::Linear, TestWidget(key))
            .background_color(color)
            .create_state()
    }

    #[test]
    fn test_rgba_lerp() {
        let a = Rgba::new(1.0, 0.0, 0.0, 1.0);
        let b = Rgba::new(0.0, 0.0, 1.0, 1.0);
        let mid = a.lerp(&b, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-6);
        assert!((mid.g - 0.0).abs() < 1e-6);
        assert!((mid.b - 0.5).abs() < 1e-6);
        assert!((mid.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_rgba_from_to_color_roundtrip() {
        let original = Rgba::new(0.25, 0.5, 0.75, 1.0);
        let color = original.to_color();
        let recovered = Rgba::from_color(&color);
        assert!((recovered.r - original.r).abs() < 0.01);
        assert!((recovered.g - original.g).abs() < 0.01);
        assert!((recovered.b - original.b).abs() < 0.01);
        assert!((recovered.a - original.a).abs() < 0.01);
    }

    #[test]
    fn test_layout_snapshot_interpolation() {
        let old = LayoutSnapshot {
            size: (100.0, 50.0),
            position: (0.0, 0.0),
            color: Rgba::WHITE,
        };
        let new = LayoutSnapshot {
            size: (200.0, 100.0),
            position: (10.0, 20.0),
            color: Rgba::BLACK,
        };

        let t = 0.5f32;
        let size = Animatable::lerp(&old.size, &new.size, t);
        let pos = Animatable::lerp(&old.position, &new.position, t);
        let color = Animatable::lerp(&old.color, &new.color, t);

        assert!((size.0 - 150.0).abs() < 1e-6);
        assert!((size.1 - 75.0).abs() < 1e-6);
        assert!((pos.0 - 5.0).abs() < 1e-6);
        assert!((pos.1 - 10.0).abs() < 1e-6);
        assert!((color.r - 0.5).abs() < 0.01);
    }

    #[test]
    fn changed_key_preserves_old_child_and_starts_morph() {
        let mut current = state("small", Rgba::WHITE);

        current.adopt_config_from(state("large", Rgba::BLACK));

        assert!(current.old_child.is_some());
        assert_eq!(current.child_key, Some(Key::Value("large".to_owned())));
        assert_eq!(current.old_color, Some(Rgba::WHITE));
        assert!(current.controller.is_animating());
    }

    #[test]
    fn changed_color_starts_morph_for_same_child() {
        let mut current = state("card", Rgba::WHITE);

        current.adopt_config_from(state("card", Rgba::BLACK));

        assert!(current.old_child.is_some());
        assert_eq!(current.old_color, Some(Rgba::WHITE));
        assert!(current.controller.is_animating());
    }

    #[test]
    fn unchanged_configuration_does_not_start_morph() {
        let mut current = state("card", Rgba::WHITE);

        current.adopt_config_from(state("card", Rgba::WHITE));

        assert!(current.old_child.is_none());
        assert!(!current.controller.is_animating());
    }
}
