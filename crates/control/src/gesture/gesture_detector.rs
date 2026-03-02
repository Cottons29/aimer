use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
#[cfg(not(target_arch = "wasm32"))]
use skia_safe::{Paint, Rect};
use std::cell::UnsafeCell;
use winit::window::Window;
// use color::prelude::{Color, ColorMixer};
use crate::event::{PointerEvent, PointerPosition};
use crate::gesture::GestureActions;
use crate::gesture::button::ButtonStyle;
use widget::base::{BuildContext, Color, ColorMixer};
use widget::style::BoxConstraint;
use widget::{Element, ElementEvent, LayoutCache};

#[cfg(not(target_arch = "wasm32"))]
type FLOAT = f32;
#[cfg(target_arch = "wasm32")]
type FLOAT = f64;

#[allow(dead_code)]
pub struct GestureDetectorElement<'a> {
    pub(crate) style: ButtonStyle,
    pub(crate) hover_style: ButtonStyle,
    pub(crate) is_disabled: bool,
    pub(crate) is_hovered: UnsafeCell<bool>,
    pub(crate) is_pressed: UnsafeCell<bool>,
    pub(crate) gesture: UnsafeCell<GestureActions>,
    pub(crate) is_mouse_down: UnsafeCell<bool>,
    pub(crate) child: Box<dyn Element>,
    pub(crate) cache: LayoutCache,
    /// Cached absolute bounding rect, updated during draw.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) cached_bounds: UnsafeCell<Option<Rect>>,
    #[cfg(target_arch = "wasm32")]
    pub(crate) cached_bounds: UnsafeCell<Option<(f64, f64, f64, f64)>>,
    pub(crate) window: &'a Window,
}

impl<'a> GestureDetectorElement<'a> {
    /// Recursively render a child element and its descendants.
    fn render_child(widget: &dyn Element, ctx: &BuildContext) {
        ctx.canvas.save();
        widget.draw(ctx);
        let content = widget.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: Vec2d::default(),
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content.width,
                max_height: content.height,
            },
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
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

        match event {
            PointerEvent::Down(_) => unsafe {
                *self.is_pressed.get() = true;
            },
            PointerEvent::Up(_) => unsafe {
                if *self.is_pressed.get() {
                    *self.is_pressed.get() = false;
                }
            },

            PointerEvent::Move(_) => {}
            PointerEvent::Cancel => unsafe {
                *self.is_pressed.get() = false;
                // *self.is_hovered.get() = false;
            },
        }
        unsafe {
            // utils::debug!("Current event : {event:?}");
            (&mut *self.gesture.get()).handle_pointer_event(event);
        }
        self.window.request_redraw();
    }

    #[inline]
    fn active_style(&self) -> &ButtonStyle {
        unsafe { if *self.is_hovered.get() && !self.is_disabled { &self.hover_style } else { &self.style } }
    }
}

impl<'b> Element for GestureDetectorElement<'b> {
    fn draw(&self, ctx: &BuildContext) {
        let scale = ctx.scale;
        let constraint = ctx.box_constraint;
        let style = self.active_style();

        let box_width = match style.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => constraint.max_width * (p / 100.0),
            Dimension::Auto => constraint.max_width,
        };

        let box_height = match style.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => constraint.max_height * (p / 100.0),
            Dimension::Auto => constraint.max_height,
        };

        let box_width = box_width.max(0.0);
        let box_height = box_height.max(0.0);

        // Draw background
        #[cfg(not(target_arch = "wasm32"))]
        {
            use skia_safe::Color as SkColor;
            use skia_safe::paint::Style;
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(SkColor::from(Color::from(style.color)));
            paint.set_style(Style::Fill);

            if self.is_disabled {
                paint.set_alpha(128);
            }

            let rect = Rect::from_xywh(0.0, 0.0, box_width, box_height);
            ctx.canvas.draw_rect(rect, &paint);

            // Cache the absolute bounding rect using the current canvas transform
            let matrix = ctx.canvas.local_to_device_as_3x3();
            let abs_x = matrix.translate_x();
            let abs_y = matrix.translate_y();
            unsafe {
                *self.cached_bounds.get() = Some(Rect::from_xywh(abs_x, abs_y, box_width, box_height));
            }

            // Draw pressed overlay for visual feedback
            if unsafe { *self.is_pressed.get() } && !self.is_disabled {
                let mut pressed_paint = Paint::default();
                pressed_paint.set_anti_alias(true);
                pressed_paint.set_color(SkColor::from_argb(40, 0, 0, 0));
                pressed_paint.set_style(Style::Fill);
                ctx.canvas.draw_rect(rect, &pressed_paint);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let color_str = style.color.to_css_color();
            if self.is_disabled {
                // Approximate disabled state (maybe handle alpha better if needed)
                ctx.canvas.set_global_alpha(0.5);
            }

            ctx.canvas.set_fill_style_str(&color_str);
            ctx.canvas.fill_rect(0.0, 0.0, box_width, box_height);

            if self.is_disabled {
                ctx.canvas.set_global_alpha(1.0);
            }

            if let Ok(transform) = ctx.canvas.get_transform() {
                let abs_x = transform.e();
                let abs_y = transform.f();
                unsafe {
                    *self.cached_bounds.get() = Some((abs_x, abs_y, box_width, box_height));
                }
            }

            if unsafe { *self.is_pressed.get() } && !self.is_disabled {
                ctx.canvas.set_fill_style_str("rgba(0, 0, 0, 0.15)");
                ctx.canvas.fill_rect(0.0, 0.0, box_width, box_height);
            }
        }

        // Draw child centered within the button bounds
        let child_size = self.child.computed_size(ctx);
        let offset_x = (box_width - child_size.width).max(0.0) / 2.0;
        let offset_y = (box_height - child_size.height).max(0.0) / 2.0;

        ctx.canvas.save();
        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.translate((offset_x, offset_y));
        #[cfg(target_arch = "wasm32")]
        match ctx.canvas.translate(offset_x, offset_y) {
            Ok(_) => (),
            Err(e) => {
                println!("{:?}", e);
            }
        }

        let child_ctx = BuildContext {
            parent_size: ResolvedSize { width: box_width, height: box_height },
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: Vec2d::default(),
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: box_width,
                max_height: box_height,
            },
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
        };
        Self::render_child(self.child.as_ref(), &child_ctx);

        ctx.canvas.restore();
    }
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

        // Hit-test against cached bounds
        let pos = match event {
            ElementEvent::PointerDown(p) | ElementEvent::PointerUp(p) | ElementEvent::PointerMove(p) => p,
            _ => return false,
        };
        #[allow(clippy::collapsible_if)]
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            if let Some(bounds) = *self.cached_bounds.get() {
                if pos.x < bounds.left || pos.x > bounds.right || pos.y < bounds.top || pos.y > bounds.bottom {
                    *self.is_hovered.get() = false;
                    self.window.request_redraw();
                    return false;
                }
            }
        }
        #[allow(clippy::collapsible_if)]
        #[cfg(target_arch = "wasm32")]
        unsafe {
            if let Some((x, y, w, h)) = *self.cached_bounds.get() {
                let right = x + w;
                let bottom = y + h;
                let px = pos.x;
                let py = pos.y;
                if px < x || px > right || py < y || py > bottom {
                    *self.is_hovered.get() = false;
                    self.window.request_redraw();
                    return false;
                }
            }
        }

        let pointer_event = match event {
            ElementEvent::PointerDown(pos) => PointerEvent::Down(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::PointerUp(pos) => PointerEvent::Up(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::PointerMove(pos) => PointerEvent::Move(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::Cancel => PointerEvent::Cancel,
        };

        self.handle_pointer_event(&pointer_event);

        // PointerMove during a press should not trigger a redraw
        !matches!(event, ElementEvent::PointerMove(_))
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

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

        ResolvedSize { width: width.max(0.0), height: height.max(0.0) }
    }
}
