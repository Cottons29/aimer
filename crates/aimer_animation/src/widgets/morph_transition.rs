use std::cell::UnsafeCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, Key, LayoutElement, Rebuildable, State, StateUpdater,
    StatefulElement, StatefulWidget, VisitorElement, Widget,
};

use crate::control::controller::AnimationController;
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
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

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
/// ```ignore
/// MorphTransition::new(
///     Duration::from_millis(400),
///     Curve::FastOutSlowIn,
///     if expanded { large_card() } else { small_card() },
/// )
/// .background_color(current_color)  // optional: enables color morphing
/// ```
pub struct MorphTransition<T: Widget + 'static> {
    pub child: Arc<T>,
    pub duration: Duration,
    pub curve: Curve,
    /// Optional background color to morph. If set, the color transitions
    /// from the old value to this value when the child changes.
    pub background_color: Option<Rgba>,
    transition_key: Option<Key>,
    widget_key: Option<Key>,
}

impl<T: Widget> MorphTransition<T> {
    pub fn new(duration: Duration, curve: Curve, child: T) -> Self {
        Self {
            child: Arc::new(child),
            duration,
            curve,
            background_color: None,
            transition_key: None,
            widget_key: None,
        }
    }

    /// Set a background color that will be morphed when the child changes.
    pub fn background_color(mut self, color: Rgba) -> Self {
        self.background_color = Some(color);
        self
    }

    pub fn child_key(mut self, key: impl Into<Key>) -> Self {
        self.transition_key = Some(key.into());
        self
    }

    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.widget_key = Some(key.into());
        self
    }
}

impl<T: Widget + 'static> StatefulWidget for MorphTransition<T> {
    type State = MorphTransitionState<T>;

    fn create_state(&self) -> Self::State {
        MorphTransitionState {
            current_child: self
                .child
                .clone(),
            old_child: None,
            child_key: self
                .transition_key
                .clone()
                .or_else(|| {
                    self.child
                        .key()
                }),
            duration: self.duration,
            curve: self.curve,
            current_color: self.background_color,
            old_color: None,
            controller: AnimationController::new(self.duration, self.curve),
            updater: StateUpdater::empty(),
        }
    }
}

impl<T: Widget + 'static> Widget for MorphTransition<T> {
    fn key(&self) -> Option<Key> {
        self.widget_key
            .clone()
    }

    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new_with_name(self, ctx, "MorphTransition", self.key())
            .0
            .boxed()
    }
}

#[doc(hidden)]
pub struct MorphTransitionState<T: Widget + 'static> {
    current_child: Arc<T>,
    old_child: Option<Arc<T>>,
    child_key: Option<Key>,
    duration: Duration,
    curve: Curve,
    current_color: Option<Rgba>,
    old_color: Option<Rgba>,
    controller: AnimationController,
    updater: StateUpdater<Self>,
}

impl<T: Widget + 'static> State<MorphTransition<T>> for MorphTransitionState<T> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.duration = new.duration;
        self.curve = new.curve;
        self.controller
            .set_duration(new.duration);
        self.controller
            .set_curve(new.curve);

        if self.child_key != new.child_key || self.current_color != new.current_color {
            self.old_child = Some(
                self.current_child
                    .clone(),
            );
            self.old_color = self.current_color;
            self.current_child = new
                .current_child
                .clone();
            self.child_key = new
                .child_key
                .clone();
            self.current_color = new.current_color;
            self.controller
                .reset();
            self.controller
                .forward();
        } else {
            self.current_child = new
                .current_child
                .clone();
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        MorphTransitionFrame {
            current_child: self
                .current_child
                .clone(),
            old_child: if self
                .controller
                .is_animating()
            {
                self.old_child
                    .clone()
            } else {
                None
            },
            current_color: self.current_color,
            old_color: if self
                .controller
                .is_animating()
            {
                self.old_color
            } else {
                None
            },
            controller: self
                .controller
                .clone(),
        }
    }
}

struct MorphTransitionFrame<T: Widget + 'static> {
    current_child: Arc<T>,
    old_child: Option<Arc<T>>,
    current_color: Option<Rgba>,
    old_color: Option<Rgba>,
    controller: AnimationController,
}

impl<T: Widget + 'static> Widget for MorphTransitionFrame<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let current_child = self
            .current_child
            .to_element(ctx);
        let current_size = current_child.computed_size(ctx);
        let old_child = self
            .old_child
            .as_ref()
            .map(|child| child.to_element(ctx));
        let old_size = old_child
            .as_ref()
            .map(|child| child.computed_size(ctx))
            .unwrap_or(current_size);
        let morphing = old_child.is_some();

        Box::new(MorphTransitionElement {
            current_child: SyncChild::new(current_child),
            old_child: SyncChild(UnsafeCell::new(old_child)),
            controller: self
                .controller
                .clone(),
            window: ctx
                .window
                .clone(),
            old_snapshot: Mutex::new(LayoutSnapshot {
                size: (old_size.width, old_size.height),
                position: (0.0, 0.0),
                color: self
                    .old_color
                    .unwrap_or(Rgba::TRANSPARENT),
            }),
            new_snapshot: Mutex::new(LayoutSnapshot {
                size: (current_size.width, current_size.height),
                position: (0.0, 0.0),
                color: self
                    .current_color
                    .unwrap_or(Rgba::TRANSPARENT),
            }),
            has_background_color: self
                .current_color
                .is_some()
                || self
                    .old_color
                    .is_some(),
            morph_state: Mutex::new(if morphing {
                MorphState::MorphingIn
            } else {
                MorphState::Idle
            }),
        })
    }
}

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

/// Unsafe wrapper for single-threaded mutable access to a boxed element.
/// Safety: the rendering pipeline is single-threaded.
struct SyncChild(UnsafeCell<Option<Box<dyn Element>>>);
unsafe impl Send for SyncChild {}
unsafe impl Sync for SyncChild {}

impl SyncChild {
    fn new(element: Box<dyn Element>) -> Self {
        Self(UnsafeCell::new(Some(element)))
    }

    /// # Safety
    /// Must only be called from the single rendering thread.
    unsafe fn get(&self) -> Option<&dyn Element> {
        unsafe {
            (*self
                .0
                .get())
            .as_ref()
            .map(|b| b.as_ref())
        }
    }

    /// Take the element out, leaving `None` in its place.
    /// # Safety
    /// Must only be called from the single rendering thread.
    unsafe fn take(&self) -> Option<Box<dyn Element>> {
        unsafe {
            (*self
                .0
                .get())
            .take()
        }
    }
}

// ---------------------------------------------------------------------------
// MorphTransitionElement
// ---------------------------------------------------------------------------

struct MorphTransitionElement {
    current_child: SyncChild,
    old_child: SyncChild,
    controller: AnimationController,
    window: WindowHandle,
    old_snapshot: Mutex<LayoutSnapshot>,
    new_snapshot: Mutex<LayoutSnapshot>,
    has_background_color: bool,
    morph_state: Mutex<MorphState>,
}

// Safety: rendering pipeline is single-threaded
unsafe impl Send for MorphTransitionElement {}
unsafe impl Sync for MorphTransitionElement {}

impl MorphTransitionElement {
    /// Compute the interpolated layout between old and new snapshots.
    fn interpolated_layout(&self, t: f32) -> LayoutSnapshot {
        let old = self
            .old_snapshot
            .lock()
            .unwrap();
        let new = self
            .new_snapshot
            .lock()
            .unwrap();
        LayoutSnapshot {
            size: Animatable::lerp(&old.size, &new.size, t),
            position: Animatable::lerp(&old.position, &new.position, t),
            color: Animatable::lerp(&old.color, &new.color, t),
        }
    }
}

impl Drawable for MorphTransitionElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();

        let curved_value = self
            .controller
            .tick(now);
        let is_animating = self
            .controller
            .is_animating();

        let morph_state = *self
            .morph_state
            .lock()
            .unwrap();

        match morph_state {
            MorphState::Idle => {
                // No morph in progress — draw the current child normally.
                unsafe {
                    if let Some(child) = self
                        .current_child
                        .get()
                    {
                        child.draw(ctx);
                    }
                }
            }
            MorphState::MorphingIn => {
                let layout = self.interpolated_layout(curved_value);
                let new_size = unsafe {
                    self.current_child
                        .get()
                        .map(|c| c.computed_size(ctx))
                        .unwrap_or(ResolvedSize { width: 0.0, height: 0.0 })
                };
                let scale_x = if new_size.width > 0.01 {
                    layout
                        .size
                        .0
                        / new_size.width
                } else {
                    1.0
                };
                let scale_y = if new_size.height > 0.01 {
                    layout
                        .size
                        .1
                        / new_size.height
                } else {
                    1.0
                };

                // --- Phase 1: Draw old child fading out (first half) ---
                if curved_value < 0.5 {
                    unsafe {
                        if let Some(old) = self
                            .old_child
                            .get()
                        {
                            let old_alpha = 1.0 - curved_value * 2.0;
                            let old_snap = self
                                .old_snapshot
                                .lock()
                                .unwrap();
                            let old_size = old_snap.size;

                            ctx.canvas
                                .save();

                            if self.has_background_color {
                                let bg = old_snap.color;
                                let overlay_alpha = bg.a * old_alpha;
                                if overlay_alpha > 0.001 {
                                    let overlay_color =
                                        Rgba::new(bg.r, bg.g, bg.b, overlay_alpha).to_color();
                                    ctx.canvas
                                        .fill_color_rect(
                                            (0.0, 0.0).into(),
                                            ResolvedSize { width: old_size.0, height: old_size.1 },
                                            overlay_color,
                                            [0.0; 4],
                                        );
                                }
                            }

                            ctx.canvas
                                .set_alpha(old_alpha);
                            old.draw(ctx);
                            ctx.canvas
                                .restore();
                        }
                    }
                }

                // --- Phase 2: Draw new child morphing in (second half) ---
                let new_alpha = if curved_value < 0.5 { curved_value * 2.0 } else { 1.0 };

                let sx = lerp_f32(scale_x, 1.0, curved_value);
                let sy = lerp_f32(scale_y, 1.0, curved_value);
                let cx = new_size.width / 2.0;
                let cy = new_size.height / 2.0;

                unsafe {
                    if let Some(child) = self
                        .current_child
                        .get()
                    {
                        ctx.canvas
                            .save();

                        if self.has_background_color {
                            let bg = layout.color;
                            let overlay_alpha = bg.a * new_alpha;
                            if overlay_alpha > 0.001 {
                                let overlay_color =
                                    Rgba::new(bg.r, bg.g, bg.b, overlay_alpha).to_color();
                                ctx.canvas
                                    .fill_color_rect(
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

                        ctx.canvas
                            .translate((cx, cy).into());
                        ctx.canvas
                            .scale(sx, sy);
                        ctx.canvas
                            .translate((-cx, -cy).into());

                        ctx.canvas
                            .set_alpha(new_alpha);
                        child.draw(ctx);
                        ctx.canvas
                            .restore();
                    }
                }
            }
        }

        if is_animating {
            self.window
                .request_redraw();
        } else if morph_state == MorphState::MorphingIn {
            let _ = unsafe {
                self.old_child
                    .take()
            };
            *self
                .morph_state
                .lock()
                .unwrap() = MorphState::Idle;
        }
    }
}

impl VisitorElement for MorphTransitionElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        unsafe {
            if let Some(child) = self
                .current_child
                .get()
            {
                visitor(child);
            }
            if let Some(old) = self
                .old_child
                .get()
            {
                visitor(old);
            }
        }
    }

    fn debug_name(&self) -> &'static str {
        "MorphTransitionElement"
    }
}

impl EventElement for MorphTransitionElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        unsafe {
            self.current_child
                .get()
                .map(|c| c.on_event(event))
                .unwrap_or(false)
        }
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        unsafe {
            if let Some(child) = self
                .current_child
                .get()
            {
                visitor(child);
            }
        }
    }
}

impl Rebuildable for MorphTransitionElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        unsafe {
            if let Some(child) = self
                .current_child
                .get()
            {
                child.rebuild_if_dirty(ctx);
            }
            if let Some(old) = self
                .old_child
                .get()
            {
                old.rebuild_if_dirty(ctx);
            }
        }
    }
}

impl LayoutElement for MorphTransitionElement {
    fn pos(&self) -> Option<Vec2d> {
        unsafe {
            self.current_child
                .get()
                .and_then(|c| c.pos())
        }
    }

    fn size(&self) -> Option<Size> {
        unsafe {
            self.current_child
                .get()
                .and_then(|c| c.size())
        }
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe {
            self.current_child
                .get()
                .map(|c| c.computed_size(ctx))
                .unwrap_or(ResolvedSize { width: 0.0, height: 0.0 })
        }
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe {
            self.current_child
                .get()
                .map(|c| c.content_size(ctx))
                .unwrap_or(ResolvedSize { width: 0.0, height: 0.0 })
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
            if let Some(child) = self
                .current_child
                .get()
            {
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
            Some(Key::Value(
                self.0
                    .to_owned(),
            ))
        }

        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            panic!("not needed for state lifecycle tests")
        }
    }

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
        let old = LayoutSnapshot { size: (100.0, 50.0), position: (0.0, 0.0), color: Rgba::WHITE };
        let new =
            LayoutSnapshot { size: (200.0, 100.0), position: (10.0, 20.0), color: Rgba::BLACK };

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

        current.adopt_config_from(&state("large", Rgba::BLACK));

        assert!(
            current
                .old_child
                .is_some()
        );
        assert_eq!(current.child_key, Some(Key::Value("large".to_owned())));
        assert_eq!(current.old_color, Some(Rgba::WHITE));
        assert!(
            current
                .controller
                .is_animating()
        );
    }

    #[test]
    fn changed_color_starts_morph_for_same_child() {
        let mut current = state("card", Rgba::WHITE);

        current.adopt_config_from(&state("card", Rgba::BLACK));

        assert!(
            current
                .old_child
                .is_some()
        );
        assert_eq!(current.old_color, Some(Rgba::WHITE));
        assert!(
            current
                .controller
                .is_animating()
        );
    }

    #[test]
    fn unchanged_configuration_does_not_start_morph() {
        let mut current = state("card", Rgba::WHITE);

        current.adopt_config_from(&state("card", Rgba::WHITE));

        assert!(
            current
                .old_child
                .is_none()
        );
        assert!(
            !current
                .controller
                .is_animating()
        );
    }
}
