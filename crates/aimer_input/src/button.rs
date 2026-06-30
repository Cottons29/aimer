use crate::callback::VoidCallback;
use crate::gesture::gesture_detector::GestureDetector;
use crate::gesture::{DragCallback, DragUpdateCallback, ScaleCallback, ScrollCallback, SwipeCallback};
use crate::mouse_region::{MouseRegion, RegionAcceptState};
use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_attribute::{BoxConstraint, CacheBounds};
use aimer_events::element::ElementEvent;
use aimer_macro::Rebuildable;
use aimer_style::BoxDecoration;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Reconcilable, State, StateUpdater, StatefulElement, StatefulWidget, VisitorElement,
    Widget, WidgetConstructor,
};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::window::Window;

/// A clickable button widget with visual feedback.
///
/// `Button` renders a decorated container (background, border, outline) and
/// provides gesture callbacks for tap, double-tap, long-press, right-click,
/// swipe, scroll, and scale. It dims when disabled.
///
/// For pure gesture detection without visual feedback, use [`GestureDetector`]
/// instead.
///
/// Element tree:
/// ```text
/// MouseRegion       — hover
///   └─ GestureDetector — press / gestures
///        └─ ButtonVisual — decoration + child
/// ```
#[allow(dead_code)]
#[derive(WidgetConstructor)]
pub struct Button<W: Widget + 'static> {
    #[constructor(default, into)]
    pub on_press: VoidCallback,
    #[constructor(default, into)]
    pub on_long_press: VoidCallback,
    #[constructor(default, into)]
    pub on_double_press: VoidCallback,
    #[constructor(default, into)]
    pub on_right_press: VoidCallback,
    #[constructor(default, into)]
    pub on_hover_enter: VoidCallback,
    #[constructor(default, into)]
    pub on_hover_exit: VoidCallback,
    #[constructor(default, into)]
    pub on_swipe: SwipeCallback,
    #[constructor(default, into)]
    pub on_scroll: ScrollCallback,
    #[constructor(default, into)]
    pub on_scale: ScaleCallback,
    #[constructor(default, into)]
    pub width: Dimension,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default)]
    pub decoration: BoxDecoration,
    #[constructor(default)]
    pub is_disabled: bool,
    child: W,
}

// ── StatefulWidget ─────────────────────────────────────────────────────

pub struct ButtonState<W: Widget> {
    // Config (persisted from Button)
    on_press: VoidCallback,
    on_long_press: VoidCallback,
    on_double_press: VoidCallback,
    on_right_press: VoidCallback,
    on_hover_enter: VoidCallback,
    on_hover_exit: VoidCallback,
    on_swipe: SwipeCallback,
    on_scroll: ScrollCallback,
    on_scale: ScaleCallback,
    width: Dimension,
    height: Dimension,
    decoration: BoxDecoration,
    is_disabled: bool,
    // SAFETY: raw pointer to the Button's child widget.
    // Only dereferenced in `build()` / `ButtonTree::to_element()` on the
    // render thread while the Button widget is still alive.
    child: *const W,
    // Persistent state — survives across rebuilds
    is_pressed: Arc<AtomicBool>,
    accept_state: Rc<Cell<RegionAcceptState>>,
    updater: StateUpdater<Self>,
}

/// SAFETY: ButtonState is used exclusively on the render thread.
/// The raw pointer `child` is only dereferenced while Button is alive.
unsafe impl<W: Widget> Send for ButtonState<W> {}
unsafe impl<W: Widget> Sync for ButtonState<W> {}

impl<W: Widget + 'static> StatefulWidget for Button<W> {
    type State = ButtonState<W>;

    fn create_state(&self) -> Self::State {
        ButtonState {
            on_press: self.on_press.clone(),
            on_long_press: self.on_long_press.clone(),
            on_double_press: self.on_double_press.clone(),
            on_right_press: self.on_right_press.clone(),
            on_hover_enter: self.on_hover_enter.clone(),
            on_hover_exit: self.on_hover_exit.clone(),
            on_swipe: self.on_swipe.clone(),
            on_scroll: self.on_scroll.clone(),
            on_scale: self.on_scale.clone(),
            width: self.width,
            height: self.height,
            decoration: self.decoration.clone(),
            is_disabled: self.is_disabled,
            child: &self.child as *const W,
            is_pressed: Arc::new(AtomicBool::new(false)),
            accept_state: Rc::new(Cell::new(RegionAcceptState::Outside)),
            updater: StateUpdater::empty(),
        }
    }
}

impl<W: Widget + 'static> State<Button<W>> for ButtonState<W> {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        // ButtonTree holds raw pointers to self and ctx.window.
        // to_element() dereferences them — safe because the widget tree is
        // only evaluated on the render thread while both are alive.
        ButtonTree { state: self as *const Self, window: _ctx.window as *const Window }
    }
}

/// Lightweight Widget returned by `ButtonState::build`.
///
/// Holds raw pointers to the state and window; `to_element` dereferences
/// them on the render thread where both are guaranteed alive.
struct ButtonTree<W: Widget> {
    state: *const ButtonState<W>,
    window: *const Window,
}

// SAFETY: ButtonTree is only used on the render thread.
unsafe impl<W: Widget> Send for ButtonTree<W> {}
unsafe impl<W: Widget> Sync for ButtonTree<W> {}

impl<W: Widget + 'static> Widget for ButtonTree<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        // SAFETY: called on render thread; state and window are alive.
        let state = unsafe { &*self.state };
        let window = unsafe { &*self.window };

        let child = unsafe { &*state.child }.to_element(ctx);
        let is_pressed = state.is_pressed.clone();

        Box::new(MouseRegion {
            on_hover_enter: state.on_hover_enter.clone(),
            on_hover_exit: state.on_hover_exit.clone(),
            cursor: None,
            accept_state: state.accept_state.clone(),
            cached_bounds: CacheBounds::new(),
            child: GestureDetector {
                cached_bounds: CacheBounds::new(),
                window,
                is_pressed,
                on_tap: state.on_press.clone(),
                on_double_press: state.on_double_press.clone(),
                on_long_press: state.on_long_press.clone(),
                on_drag_start: DragCallback::default(),
                on_drag_update: DragUpdateCallback::default(),
                on_drag_end: VoidCallback::default(),
                on_right_tap: state.on_right_press.clone(),
                on_swipe: state.on_swipe.clone(),
                on_scroll: state.on_scroll.clone(),
                on_scale: state.on_scale.clone(),
                #[cfg(not(target_arch = "wasm32"))]
                runtime_handle: Some(ctx.async_handle.clone()),
                state: Default::default(),
                child,
            },
            window,
        })
    }
}

// ── Widget impl (creates StatefulElement) ──────────────────────────────

impl<W: Widget + 'static> Widget for Button<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let (element, _updater) = StatefulElement::new(self, ctx);
        Box::new(element)
    }
}

// ── ButtonVisual ───────────────────────────────────────────────────────
// Renders decoration + pressed overlay around the user's child.

#[derive(Rebuildable)]
struct ButtonVisual<E: Element> {
    width: Dimension,
    height: Dimension,
    decoration: BoxDecoration,
    is_disabled: bool,
    is_pressed: Arc<AtomicBool>,
    child: E,
    cached_bounds: CacheBounds,
}

impl<E: Element> ButtonVisual<E> {
    fn compute_dimensions(&self, ctx: &BuildContext) -> (f32, f32) {
        let box_width = match self.width {
            Dimension::Px(w) => w * ctx.scale,
            Dimension::Percent(p) => ctx.box_constraint.max_width * (p / 100.0),
            Dimension::Auto => ctx.box_constraint.max_width,
        };
        let box_height = match self.height {
            Dimension::Px(h) => h * ctx.scale,
            Dimension::Percent(p) => ctx.box_constraint.max_height * (p / 100.0),
            Dimension::Auto => ctx.box_constraint.max_height,
        };
        (box_width.max(0.0), box_height.max(0.0))
    }
}

impl<E: Element> VisitorElement for ButtonVisual<E> {
    fn debug_name(&self) -> &'static str {
        "ButtonVisual"
    }
}

impl<E: Element> EventElement for ButtonVisual<E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.child.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }
}

impl<E: Element> LayoutElement for ButtonVisual<E> {
    #[inline]
    fn size(&self) -> Option<Size> {
        Some(Size { width: self.width, height: self.height })
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale = ctx.scale;
        let constraint = ctx.box_constraint;
        let decoration = &self.decoration;

        let child_size = self.child.computed_size(ctx);

        let width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => constraint.max_width * (p / 100.0),
            Dimension::Auto => child_size.width,
        };

        let height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => constraint.max_height * (p / 100.0),
            Dimension::Auto => child_size.height,
        };

        let width = width.max(0.0);
        let height = height.max(0.0);
        let (ol, ot, or, ob) = decoration.outline.strokes(width, height, scale);

        ResolvedSize { width: width + ol + or, height: height + ot + ob }
    }
}

impl<E: Element> Drawable for ButtonVisual<E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        let (box_width, box_height) = self.compute_dimensions(ctx);

        ctx.canvas.save();
        let decoration = &self.decoration;
        let (ol, ot, _or, _ob) = decoration.outline.strokes(box_width, box_height, ctx.scale);
        ctx.canvas.translate((ol, ot).into());

        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds.save(ctx.scale, abs_x, abs_y, box_width, box_height);

        if self.is_disabled {
            ctx.canvas.set_alpha(0.5);
        }

        let decoration_ctx = BuildContext { parent_size: ResolvedSize { width: box_width, height: box_height }, ..ctx.clone() };
        decoration.draw(&decoration_ctx);

        if self.is_disabled {
            ctx.canvas.restore_alpha();
        }

        let radii = decoration.border_radius.resolve(box_width, box_height, ctx.scale);

        if self.is_pressed.load(Ordering::Relaxed) && !self.is_disabled {
            let overlay_color = Color::Rgba(0, 0, 0, 40);
            ctx.canvas.fill_color_rect_per_corner(
                (0.0, 0.0).into(),
                ResolvedSize { width: box_width, height: box_height },
                overlay_color,
                radii,
            );
        }

        let has_radius = radii.iter().any(|&r| r > 0.0);
        if has_radius {
            ctx.canvas
                .set_clip_rounded((0.0, 0.0).into(), ResolvedSize { width: box_width, height: box_height }, radii);
        }

        // Draw child centered within the button box
        let child_size = self.child.computed_size(ctx);
        let offset_x = (box_width - child_size.width).max(0.0) / 2.0;
        let offset_y = (box_height - child_size.height).max(0.0) / 2.0;

        ctx.canvas.save();
        ctx.canvas.translate((offset_x, offset_y).into());

        let child_ctx = BuildContext {
            parent_size: ResolvedSize { width: box_width, height: box_height },
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: Vec2d::default(),
            cursor_pos: ctx.cursor_pos,
            box_constraint: BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: box_width, max_height: box_height },
            visible_rect: ctx.visible_rect,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };

        fn render_child(widget: &dyn Element, ctx: &BuildContext) {
            ctx.canvas.save();
            widget.draw(ctx);
            let content = widget.content_size(ctx);
            let child_ctx = BuildContext {
                parent_size: content,
                canvas: ctx.canvas.clone(),
                scale: ctx.scale,
                parent_pos: Vec2d::default(),
                cursor_pos: ctx.cursor_pos,
                box_constraint: BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: content.width, max_height: content.height },
                visible_rect: ctx.visible_rect,
                window: ctx.window,
                #[cfg(not(target_arch = "wasm32"))]
                async_handle: ctx.async_handle.clone(),
                inherited_states: ctx.inherited_states.clone(),
            };
            widget.visit_children(&mut |child| {
                render_child(child, &child_ctx);
            });
            ctx.canvas.restore();
        }
        render_child(&self.child, &child_ctx);

        ctx.canvas.restore();

        if has_radius {
            ctx.canvas.clear_clip();
        }

        ctx.canvas.restore();
    }
}

impl<E: Element + 'static> Reconcilable for ButtonVisual<E> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_from_widget(&self, _new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        false
    }
}
