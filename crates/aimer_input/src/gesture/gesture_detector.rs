use crate::gesture::GestureActions;
use aimer_attribute::{BoxConstraint, CacheBounds};
use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_events::pointer::{PointerEvent, PointerPosition};
use std::cell::{Cell, RefCell};
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{Drawable, Element, EventElement, LayoutCache, LayoutElement, VisitorElement};
use winit::window::Window;
use aimer_style::BoxDecoration;
use aimer_macro::Rebuildable;

#[allow(dead_code)]
#[derive(Rebuildable)]
pub struct GestureDetector<'a, E: Element> {
    pub(crate) width: Dimension,
    pub(crate) height: Dimension,
    pub(crate) decoration: BoxDecoration,
    pub(crate) hover_decoration: BoxDecoration,
    pub(crate) pressed_decoration: BoxDecoration,
    pub(crate) disabled_decoration: BoxDecoration,
    pub(crate) is_disabled: bool,
    pub(crate) is_hovered: Cell<bool>,
    pub(crate) is_pressed: Cell<bool>,
    pub(crate) pressed_overlay_color: Option<Color>,
    pub(crate) gesture: RefCell<GestureActions>,
    pub(crate) is_mouse_down: Cell<bool>,
    pub(crate) is_dirty: Cell<bool>,
    pub(crate) child: E,
    pub(crate) cache: LayoutCache,
    pub(crate) cached_bounds: CacheBounds,
    pub(crate) window: &'a Window,
}

impl<'a, E: Element> GestureDetector<'a, E> {
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
            box_constraint: BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: content.width, max_height: content.height },
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
            PointerEvent::Down(_) => {
                if !self.is_pressed.get() {
                    self.is_pressed.set(true);
                    changed = true;
                }
            }
            PointerEvent::Up(_) => {
                if self.is_pressed.get() {
                    self.is_pressed.set(false);
                    changed = true;
                }
            }

            PointerEvent::Move(_) => {}
            PointerEvent::Cancel => {
                if self.is_pressed.get() {
                    self.is_pressed.set(false);
                    changed = true;
                }
            }
        }
        
        self.gesture.borrow_mut().handle_pointer_event(event);

        if changed {
            self.is_dirty.set(true);
            self.window.request_redraw();
        }


    }

    #[inline]
    fn active_decoration(&self) -> &BoxDecoration {
        if self.is_disabled {
            &self.disabled_decoration
        } else if self.is_pressed.get() {
            &self.pressed_decoration
        } else if self.is_hovered.get() {
            &self.hover_decoration
        } else {
            &self.decoration
        }
    }

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

impl<'b, E: Element> VisitorElement for GestureDetector<'b, E> {
    fn debug_name(&self) -> &'static str {
        "GestureDetector"
    }
}

impl<'b, E: Element> EventElement for GestureDetector<'b, E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        // debug!("GestureDetectorElement::on_event: {:?}", event);
        // debug!("GestureDetectorElement::caches_bound: {:?}", self.cached_bounds);


        if self.is_disabled {
            return false;
        }

        if matches!(event, ElementEvent::Cancel) {
            self.handle_pointer_event(&PointerEvent::Cancel);
            self.is_hovered.set(false);
            self.window.request_redraw();
            return true;
        }

        let pos = match event {
            ElementEvent::PointerDown(p) | ElementEvent::PointerUp(p) | ElementEvent::PointerMove(p) => p,
            _ => return false,
        };
        // debug!("Step 1");

        let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);

        let is_pressed = self.is_pressed.get();

        if !is_inside && !is_pressed {
            let was_hovered = self.is_hovered.get();
            self.is_hovered.set(false);
            if was_hovered {
                self.is_dirty.set(true);
                self.window.request_redraw();
            }
            return false;
        }
        // debug!("Step 3");

        if matches!(event, ElementEvent::PointerMove(_)) && is_inside == self.is_hovered.get() {
            return true;
        }


        self.is_hovered.set(is_inside);
        self.is_dirty.set(true);
        self.window.request_redraw();

        // debug!("Step 5");

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
}


impl<'b, E: Element> LayoutElement for GestureDetector<'b, E> {
    #[inline]
    fn size(&self) -> Option<Size> {
        Some(Size { width: self.width, height: self.height })
    }



    /// Compute box dimensions using the non-hover style first (dimensions
    /// should be the same for both styles, but we need them to calculate
    /// bounds before deciding on hover).
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale = ctx.scale;
        let constraint = ctx.box_constraint;
        let decoration = self.active_decoration();

        let width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => constraint.max_width * (p / 100.0),
            Dimension::Auto => self.child.computed_size(ctx).width,
        };

        let height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => constraint.max_height * (p / 100.0),
            Dimension::Auto => self.child.computed_size(ctx).height,
        };

        let width = width.max(0.0);
        let height = height.max(0.0);
        let (ol, ot, or, ob) = decoration.outline.strokes(width, height, scale);

        ResolvedSize { width: width + ol + or, height: height + ot + ob }
    }
}

impl<'w, E: Element> Drawable for GestureDetector<'w, E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        self.is_dirty.set(false);
        let (box_width, box_height) = self.compute_dimensions(ctx);

        ctx.canvas.save();
        // Compute outline strokes using the current decoration (before hover re-evaluation)
        let decoration = self.active_decoration();
        let (ol, ot, _or, _ob) = decoration.outline.strokes(box_width, box_height, ctx.scale);
        ctx.canvas.translate((ol, ot).into());

        // Cache absolute bounds for hit-testing
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, box_width, box_height);

        // Re-evaluate hover state from the current cursor position so that
        // newly-rebuilt elements (which start with is_hovered = false) still
        // render the correct decoration when the pointer is over them.
        if !self.is_disabled {
            let hovering = self.cached_bounds.is_inside(ctx.cursor_pos.x, ctx.cursor_pos.y);
            self.is_hovered.set(hovering);
        }

        let decoration = self.active_decoration();

        // Draw background + border + outline using BoxDecoration
        if self.is_disabled {
            ctx.canvas.set_alpha(0.5);
        }

        let decoration_ctx = BuildContext { parent_size: ResolvedSize { width: box_width, height: box_height }, ..ctx.clone() };
        decoration.draw(&decoration_ctx);

        if self.is_disabled {
            ctx.canvas.restore_alpha();
        }

        let radii = decoration
            .border_radius
            .resolve(box_width, box_height, ctx.scale);

        // Draw pressed overlay for visual feedback
        if self.is_pressed.get() && !self.is_disabled {
            let overlay_color = self.pressed_overlay_color.unwrap_or(Color::Rgba(0, 0, 0, 40));
            ctx.canvas.fill_color_rect_per_corner(
                (0.0, 0.0).into(),
                ResolvedSize { width: box_width, height: box_height },
                overlay_color,
                radii,
            );
        }

        // Clip children to the rounded border so they don't leak outside
        let has_radius = radii.iter().any(|&r| r > 0.0);
        if has_radius {
            ctx.canvas
                .set_clip_rounded((0.0, 0.0).into(), ResolvedSize { width: box_width, height: box_height }, radii);
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
            box_constraint: BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: box_width, max_height: box_height },
            visible_rect: ctx.visible_rect,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };
        Self::render_child(&self.child, &child_ctx);

        ctx.canvas.restore();

        // Clear the rounded clip if it was set
        if has_radius {
            ctx.canvas.clear_clip();
        }

        ctx.canvas.restore();
    }
}
