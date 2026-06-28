use aimer_attribute::{BoxConstraint, CacheBounds};
use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_style::BoxDecoration;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{Drawable, Element, EventElement, LayoutElement, Rebuildable, Reconcilable, VisitorElement, Widget, WidgetConstructor};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use winit::window::Window;

use crate::callback::VoidCallback;
use crate::gesture::{ScrollCallback, ScaleCallback, SwipeCallback, DragCallback, DragUpdateCallback};
use crate::gesture::gesture_detector::GestureDetector;
use crate::mouse_region::MouseRegion;

/// A clickable button widget with visual feedback.
///
/// `Button` renders a decorated container (background, border, outline) and
/// provides gesture callbacks for tap, double-tap, long-press, right-click,
/// swipe, scroll, and scale. It dims when disabled.
///
/// For pure gesture detection without visual feedback, use [`GestureDetector`]
/// instead.
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

impl<W: Widget> Widget for Button<W> {
    #[inline]
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);

        // Shared hover state — MouseRegion writes, ButtonElement reads
        let is_hovered = Rc::new(Cell::new(false));

        let gesture_detector = GestureDetector {
            width: self.width,
            height: self.height,
            child,
            cached_bounds: CacheBounds::new(),
            window: ctx.window,
            on_tap: self.on_press.clone(),
            on_double_press: self.on_double_press.clone(),
            on_long_press: self.on_long_press.clone(),
            on_drag_start: DragCallback::default(),
            on_drag_update: DragUpdateCallback::default(),
            on_drag_end: VoidCallback::default(),
            on_right_tap: self.on_right_press.clone(),
            on_swipe: self.on_swipe.clone(),
            on_scroll: self.on_scroll.clone(),
            on_scale: self.on_scale.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            runtime_handle: Some(ctx.async_handle.clone()),
            state: Default::default(),
        };

        let button_element = ButtonElement {
            width: self.width,
            height: self.height,
            decoration: self.decoration.clone(),
            is_disabled: self.is_disabled,
            is_hovered: Rc::clone(&is_hovered),
            is_pressed: Cell::new(false),
            gesture_detector: RefCell::new(gesture_detector),
            cached_bounds: CacheBounds::new(),
            window: ctx.window,
        };

        // MouseRegion wraps ButtonElement — handles hover via mouse-only input
        Box::new(MouseRegion {
            on_hover_enter: self.on_hover_enter.clone(),
            on_hover_exit: self.on_hover_exit.clone(),
            cursor: None,
            is_hovered,
            cached_bounds: CacheBounds::new(),
            child: button_element,
            window: ctx.window,
        })
    }
}

/// Internal element that renders the button's visual appearance and
/// delegates gesture detection to [`GestureDetector`].
struct ButtonElement<'a, E: Element> {
    width: Dimension,
    height: Dimension,
    decoration: BoxDecoration,
    is_disabled: bool,
    is_hovered: Rc<Cell<bool>>,
    is_pressed: Cell<bool>,
    gesture_detector: RefCell<GestureDetector<'a, E>>,
    cached_bounds: CacheBounds,
    window: &'a Window,
}

impl<'a, E: Element> ButtonElement<'a, E> {
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

    /// Borrow the gesture detector immutably for the duration of `f`.
    fn with_gd<R>(&self, f: impl FnOnce(&GestureDetector<'a, E>) -> R) -> R {
        f(&self.gesture_detector.borrow())
    }
}

impl<'b, E: Element> VisitorElement for ButtonElement<'b, E> {
    fn debug_name(&self) -> &'static str {
        "Button"
    }
}

impl<'b, E: Element> EventElement for ButtonElement<'b, E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        if self.is_disabled {
            return false;
        }

        // Cancel always resets pressed state
        if matches!(event, ElementEvent::Cancel) {
            self.is_pressed.set(false);
            self.gesture_detector.borrow().on_event(event);
            self.window.request_redraw();
            return true;
        }

        let pos = match event {
            ElementEvent::PointerDown(p, _, _)
            | ElementEvent::PointerUp(p, _, _)
            | ElementEvent::PointerMove(p, _, _) => p,
            ElementEvent::Scroll { .. } => {
                let consumed = self.gesture_detector.borrow().on_event(event);
                if consumed {
                    self.window.request_redraw();
                }
                return consumed;
            }
            _ => return false,
        };

        let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);
        let is_pressed = self.is_pressed.get();

        if !is_inside && !is_pressed {
            return false;
        }

        if matches!(event, ElementEvent::PointerMove(_, _, _)) && !is_pressed {
            return false;
        }

        // Update pressed visual state
        match event {
            ElementEvent::PointerDown(_, _, _) => self.is_pressed.set(true),
            ElementEvent::PointerUp(_, _, _) => self.is_pressed.set(false),
            _ => {}
        }

        self.gesture_detector.borrow().on_event(event);
        self.window.request_redraw();
        true
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Safety: the Ref lives for the duration of this call. The visitor
        // is called synchronously and does not store the reference past the
        // call. The RefCell prevents concurrent mutable access.
        let gd = self.gesture_detector.borrow();
        let child: &'a dyn Element = unsafe { std::mem::transmute(&gd.child as &dyn Element) };
        visitor(child);
    }
}

impl<'b, E: Element> LayoutElement for ButtonElement<'b, E> {
    #[inline]
    fn size(&self) -> Option<Size> {
        Some(Size { width: self.width, height: self.height })
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale = ctx.scale;
        let constraint = ctx.box_constraint;
        let decoration = &self.decoration;

        let child_width = self.with_gd(|gd| gd.child.computed_size(ctx).width);
        let child_height = self.with_gd(|gd| gd.child.computed_size(ctx).height);

        let width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => constraint.max_width * (p / 100.0),
            Dimension::Auto => child_width,
        };

        let height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => constraint.max_height * (p / 100.0),
            Dimension::Auto => child_height,
        };

        let width = width.max(0.0);
        let height = height.max(0.0);
        let (ol, ot, or, ob) = decoration.outline.strokes(width, height, scale);

        ResolvedSize { width: width + ol + or, height: height + ot + ob }
    }
}

impl<'w, E: Element> Drawable for ButtonElement<'w, E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        let (box_width, box_height) = self.compute_dimensions(ctx);

        ctx.canvas.save();
        let decoration = &self.decoration;
        let (ol, ot, _or, _ob) = decoration.outline.strokes(box_width, box_height, ctx.scale);
        ctx.canvas.translate((ol, ot).into());

        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds.save(ctx.scale, abs_x, abs_y, box_width, box_height);
        // Also, save the GestureDetector's bounds — its draw() is never called
        // since we render the child directly, but its on_event needs bounds for
        // hit-testing.
        self.gesture_detector.borrow().cached_bounds.save(ctx.scale, abs_x, abs_y, box_width, box_height);

        if self.is_disabled {
            ctx.canvas.set_alpha(0.5);
        }

        let decoration_ctx = BuildContext {
            parent_size: ResolvedSize { width: box_width, height: box_height },
            ..ctx.clone()
        };
        decoration.draw(&decoration_ctx);

        if self.is_disabled {
            ctx.canvas.restore_alpha();
        }

        let radii = decoration.border_radius.resolve(box_width, box_height, ctx.scale);

        if self.is_pressed.get() && !self.is_disabled {
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
            ctx.canvas.set_clip_rounded(
                (0.0, 0.0).into(),
                ResolvedSize { width: box_width, height: box_height },
                radii,
            );
        }

        // Draw child via gesture detector's draw
        self.with_gd(|gd| {
            let child_size = gd.child.computed_size(ctx);
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
                box_constraint: BoxConstraint {
                    min_width: 0.0,
                    min_height: 0.0,
                    max_width: box_width,
                    max_height: box_height,
                },
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
                    box_constraint: BoxConstraint {
                        min_width: 0.0,
                        min_height: 0.0,
                        max_width: content.width,
                        max_height: content.height,
                    },
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
            render_child(&gd.child, &child_ctx);
        });

        ctx.canvas.restore();

        if has_radius {
            ctx.canvas.clear_clip();
        }

        ctx.canvas.restore();
    }
}

impl<'b, E: Element> Rebuildable for ButtonElement<'b, E> {}

impl<'b: 'static, E: Element + 'static> Reconcilable for ButtonElement<'b, E> {
    fn as_any(&self) -> &dyn std::any::Any { self }

    fn update_from_widget(&self, _new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        false
    }
}
