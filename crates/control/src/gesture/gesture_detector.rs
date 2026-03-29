use crate::gesture::button::ButtonStyle;
use crate::gesture::GestureActions;
use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
use attribute::CacheBounds;
use events::element::ElementEvent;
use events::pointer::{PointerEvent, PointerPosition};
use std::cell::UnsafeCell;
use widget::base::{BuildContext, Color};
use widget::style::BoxConstraint;
use widget::{Drawable, Element, LayoutCache};
use winit::window::Window;

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
    pub(crate) cached_bounds: CacheBounds,
    pub(crate) window: &'a Window,
}

impl<'a, E: Element> GestureDetectorElement<'a, E> {
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
        // debug!("GestureDetectorElement::handle_pointer_event: {:?}", event);
        if self.is_disabled {
            // debug!("GestureDetectorElement::handle_pointer_event: disabled");
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

    fn compute_dimensions(&self, ctx: &BuildContext) -> (f32, f32) {
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
        // debug!("GestureDetectorElement::on_event: {:?}", event);
        // debug!("GestureDetectorElement::caches_bound: {:?}", self.cached_bounds);

        if self.is_disabled {
            // debug!("GestureDetectorElement::on_event: disabled");
            return false;
        }

        if matches!(event, ElementEvent::Cancel) {
            // debug!("GestureDetectorElement::on_event: cancel");
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
        // debug!("Step 1");

        let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);

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
        // debug!("Step 3");

        if matches!(event, ElementEvent::PointerMove(_)) && is_inside == unsafe { *self.is_hovered.get() } {
            return true;
        }

        // debug!("Step 4");

        unsafe {
            let current_hovered = *self.is_hovered.get();
            if current_hovered != is_inside {
                *self.is_hovered.get() = is_inside;
                *self.is_dirty.get() = true;
                self.window.request_redraw();
            }
        }

        // debug!("Step 5");

        let pointer_event = match event {
            ElementEvent::PointerDown(pos) => PointerEvent::Down(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::PointerUp(pos) => PointerEvent::Up(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::PointerMove(pos) => PointerEvent::Move(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::Cancel => PointerEvent::Cancel,
            _ => return false,
        };

        // debug!("Step 6");

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

        let size = ResolvedSize { width: width + ol + or, height: height + ot + ob };
        size
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
        self.cached_bounds.save(
            ctx.scale,
            abs_x ,
            abs_y ,
            box_width ,
            box_height ,
        );

        // Draw background + border + outline in a single pass
        let bg_color: Color = style.color.into();
        let radius = style
            .border
            .get_uniform_radius(box_width, box_height, ctx.scale)
            .unwrap_or(0.0);

        let has_border = style.border.has_visible_border(box_width, box_height, ctx.scale);
        let has_outline = style.outline.has_visible_outline(box_width, box_height, ctx.scale);

        if self.is_disabled {
            ctx.canvas.set_alpha(0.5);
        }

        let size = ResolvedSize { width: box_width, height: box_height };
        let pos: Vec2d = (0.0, 0.0).into();

        if has_border || has_outline {
            // Resolve border strokes
            let (bl, bt, br, bb) = if has_border {
                style.border.strokes(box_width, box_height, ctx.scale)
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
            let border_width = [bt , br , bb , bl ];
            let border_color = if has_border { style.border.left.color } else { Color::Transparent };

            // Resolve outline strokes
            let (ol_l, ol_t, ol_r, ol_b) = if has_outline {
                style.outline.strokes(box_width, box_height, ctx.scale)
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
            let outline_width = [ol_t , ol_r , ol_b , ol_l ];
            let outline_color = if has_outline { style.outline.left.color } else { Color::Transparent };

            // Resolve per-corner radii
            let border_radius = style.border.get_per_corner_radii(box_width, box_height, ctx.scale)
                .unwrap_or([0.0; 4]);

            ctx.canvas.fill_rect_with_border_and_outline_per_side(
                pos,
                size,
                bg_color,
                border_radius,
                border_width,
                border_color,
                outline_width,
                outline_color,
            );
        } else {
            ctx.canvas.fill_color_rect(
                pos,
                size,
                bg_color,
                radius ,
            );
        }

        if self.is_disabled {
            ctx.canvas.restore_alpha();
        }

        // Draw pressed overlay for visual feedback
        if unsafe { *self.is_pressed.get() } && !self.is_disabled {
            ctx.canvas.fill_color_rect(
                (0.0, 0.0).into(),
                ResolvedSize { width: box_width, height: box_height },
                Color::Rgba(0, 0, 0, 40),
                // Colors::Green.into(),
                radius ,
            );
        }

        // Clip children to the rounded border so they don't leak outside
        if radius > 0.0 {
            ctx.canvas.set_clip_rounded(
                (0.0, 0.0).into(),
                ResolvedSize { width: box_width, height: box_height },
                radius ,
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

        // Clear the rounded clip if it was set
        if radius > 0.0 {
            ctx.canvas.clear_clip();
        }

        ctx.canvas.restore();
    }
}
