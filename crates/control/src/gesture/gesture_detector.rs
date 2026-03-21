use crate::gesture::button::ButtonStyle;
use crate::gesture::GestureActions;
use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
use events::element::ElementEvent;
use events::pointer::{PointerEvent, PointerPosition};
use std::cell::UnsafeCell;
use widget::base::{BuildContext, Color};
use widget::style::BoxConstraint;
use widget::{Drawable, Element, LayoutCache};
use winit::window::Window;
use color::prelude::ColorMixer;
use canvas::CanvasRendering;

#[cfg(not(target_arch = "wasm32"))]
type Float = f32;
#[cfg(target_arch = "wasm32")]
type Float = f64;

#[allow(dead_code)]
pub struct GestureDetectorElement<'a, E: Element> {
    pub(crate) style: ButtonStyle,
    pub(crate) hover_style: ButtonStyle,
    pub(crate) pressed_style: ButtonStyle,
    pub(crate) disabled_style: ButtonStyle,
    pub(crate) is_disabled: bool,
    pub(crate) is_hovered: UnsafeCell<bool>,
    pub(crate) is_pressed: UnsafeCell<bool>,
    pub(crate) gesture: UnsafeCell<GestureActions>,
    pub(crate) is_mouse_down: UnsafeCell<bool>,
    pub(crate) is_dirty: UnsafeCell<bool>,
    pub(crate) child: E,
    pub(crate) cache: LayoutCache,
    /// Cached absolute bounding rect (abs_x, abs_y, width, height), updated during draw.
    pub(crate) cached_bounds: UnsafeCell<Option<(f64, f64, f64, f64)>>,
    pub(crate) window: &'a Window,
}

impl<'a,E: Element> GestureDetectorElement<'a, E> {


    /// Recursively render a child element and its descendants.
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
            Self::render_child(child, &child_ctx);
        });
        ctx.canvas.restore();
    }

    /// Feed a pointer event into the button. Returns `true` if the event was consumed.
    pub fn handle_pointer_event(&self, event: &PointerEvent) {
        if self.is_disabled {
            return;
        }

        let mut changed = false;
        match event {
            PointerEvent::Down(_) => unsafe {
                if !*self.is_pressed.get() {
                    *self.is_pressed.get() = true;
                    changed = true;
                }
            },
            PointerEvent::Up(_) => unsafe {
                if *self.is_pressed.get() {
                    *self.is_pressed.get() = false;
                    changed = true;
                }
            },

            PointerEvent::Move(_) => {}
            PointerEvent::Cancel => unsafe {
                if *self.is_pressed.get() {
                    *self.is_pressed.get() = false;
                    changed = true;
                }
            },
        }
        unsafe {
            (&mut *self.gesture.get()).handle_pointer_event(event);
        }

        if changed {
            unsafe {
                *self.is_dirty.get() = true;
            }
            self.window.request_redraw();
        }
    }

    #[inline]
    fn active_style(&self) -> &ButtonStyle {
        unsafe {
            if self.is_disabled {
                &self.disabled_style
            } else if *self.is_pressed.get() {
                &self.pressed_style
            } else if *self.is_hovered.get() {
                &self.hover_style
            } else {
                &self.style
            }
        }
    }

    fn compute_dimensions(&self, ctx: &BuildContext) -> (Float, Float) {

        let base_style = &self.style;

        let box_width = match base_style.width {
            Dimension::Px(w) => w * ctx.scale,
            Dimension::Percent(p) => ctx.box_constraint.max_width * (p / 100.0),
            Dimension::Auto => ctx.box_constraint.max_width,
        };

        let box_height = match base_style.height {
            Dimension::Px(h) => h * ctx.scale,
            Dimension::Percent(p) => ctx.box_constraint.max_height * (p / 100.0),
            Dimension::Auto => ctx.box_constraint.max_height,
        };

        (box_width.max(0.0), box_height.max(0.0))

    }
}

impl<'b, E: Element> Element for GestureDetectorElement<'b, E> {
    #[inline]
    fn size(&self) -> Option<Size> {
        let style = self.active_style();
        Some(Size { width: style.width, height: style.height })
    }

    fn on_event(&self, event: &ElementEvent) -> bool {

        if self.is_disabled {
            return false;
        }

        if matches!(event, ElementEvent::Cancel) {
            self.handle_pointer_event(&PointerEvent::Cancel);
            unsafe {
                *self.is_hovered.get() = false;
            }
            return true;
        }

        let pos = match event {
            ElementEvent::PointerDown(p) | ElementEvent::PointerUp(p) | ElementEvent::PointerMove(p) => p,
            _ => return false,
        };

        let mut is_inside = false;
        unsafe {
            if let Some((x, y, w, h)) = *self.cached_bounds.get() {
                is_inside = pos.x as f64 >= x && pos.x as f64 <= x + w && pos.y as f64 >= y && pos.y as f64 <= y + h;
            }
        }

        let is_pressed = unsafe { *self.is_pressed.get() };

        if !is_inside && !is_pressed {
            unsafe {
                if *self.is_hovered.get() {
                    *self.is_hovered.get() = false;
                    *self.is_dirty.get() = true;
                    self.window.request_redraw();
                }
            }
            return false;
        }

        if matches!(event, ElementEvent::PointerMove(_)) && is_inside == unsafe { *self.is_hovered.get() } {
            return true;
        }

        unsafe {
            let current_hovered = *self.is_hovered.get();
            if current_hovered != is_inside {
                *self.is_hovered.get() = is_inside;
                *self.is_dirty.get() = true;
                self.window.request_redraw();
            }
        }

        let pointer_event = match event {
            ElementEvent::PointerDown(pos) => PointerEvent::Down(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::PointerUp(pos) => PointerEvent::Up(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::PointerMove(pos) => PointerEvent::Move(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::Cancel => PointerEvent::Cancel,
            _ => return false,
        };

        self.handle_pointer_event(&pointer_event);

        true
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    /// Compute box dimensions using the non-hover style first (dimensions
    /// should be the same for both styles, but we need them to calculate
    /// bounds before deciding on hover).
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale = ctx.scale;
        let constraint = ctx.box_constraint;
        let style = self.active_style();

        let width = match style.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => constraint.max_width * (p / 100.0),
            Dimension::Auto => self.child.computed_size(ctx).width,
        };

        let height = match style.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => constraint.max_height * (p / 100.0),
            Dimension::Auto => self.child.computed_size(ctx).height,
        };

        let width = width.max(0.0);
        let height = height.max(0.0);
        let (ol, ot, or, ob) = style.outline.strokes(width, height, scale);

        ResolvedSize { width: width + ol + or, height: height + ot + ob }
    }

    fn debug_name(&self) -> &'static str {
        "GestureDetector"
    }
}

impl<'w, E: Element> Drawable for GestureDetectorElement<'w, E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        unsafe {
            *self.is_dirty.get() = false;
        }
        let (box_width, box_height) = self.compute_dimensions(ctx);
        let style = self.active_style();

        ctx.canvas.save();
        let (ol, ot, _or, _ob) = style.outline.strokes(box_width, box_height, ctx.scale);
        ctx.canvas.translate((ol, ot).into());

        // Cache absolute bounds for hit-testing
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        unsafe {
            *self.cached_bounds.get() = Some((abs_x, abs_y, box_width as f64, box_height as f64));
        }

        // Draw background
        let bg_color: Color = style.color.into();
        let radius = style.border.get_uniform_radius(box_width, box_height, ctx.scale).unwrap_or(0.0);

        if self.is_disabled {
            ctx.canvas.set_alpha(0.5);
        }

        ctx.canvas.fill_color_rect(
            (0.0, 0.0).into(),
            ResolvedSize { width: box_width, height: box_height },
            bg_color,
            radius as f32,
        );

        if self.is_disabled {
            ctx.canvas.restore_alpha();
        }

        // Draw border and outline
        let border_ctx = BuildContext {
            parent_size: ResolvedSize { width: box_width, height: box_height },
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: ctx.box_constraint,
            visible_rect: ctx.visible_rect,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };
        style.border.draw(&border_ctx);
        style.outline.draw(&border_ctx);

        // Draw pressed overlay for visual feedback
        if unsafe { *self.is_pressed.get() } && !self.is_disabled {
            ctx.canvas.fill_color_rect(
                (0.0, 0.0).into(),
                ResolvedSize { width: box_width, height: box_height },
                Color::Rgba(0, 0, 0, 40),
                radius as f32,
            );
        }

        // Draw child centered within the button bounds
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
        Self::render_child(&self.child, &child_ctx);

        ctx.canvas.restore();
        ctx.canvas.restore();
    }
}