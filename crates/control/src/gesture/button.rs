use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use skia_safe::{Color as SkColor, Paint, Rect, paint::Style};
use widget::{Constructor, Element, ElementEvent, LayoutCache, Widget, base::*, style::BoxConstraint};

use crate::event::{PointerEvent, PointerPosition};
use crate::gesture::GestureDetector;



#[allow(dead_code)]
#[derive(Default, Clone, Copy, Constructor)]
pub struct ButtonStyle {
    #[constructor(default, into)]
    pub color: Colors,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default, into)]
    pub width: Dimension,
}



#[allow(dead_code)]
#[derive(Constructor)]
pub struct Button {
    #[constructor(default)]
    pub on_press: Option<Arc<dyn Fn() + Send + Sync>>,
    #[constructor(default)]
    pub on_long_press: Option<Arc<dyn Fn() + Send + Sync>>,
    #[constructor(default)]
    pub style: ButtonStyle,
    #[constructor(default)]
    pub hover_style: ButtonStyle,
    #[constructor(default)]
    pub is_disabled: bool,
    child: Box<dyn Widget>,
}

impl Widget for Button {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);

        let mut gesture = GestureDetector::new();
        if let Some(cb) = &self.on_press {
            let cb = cb.clone();
            gesture.on_tap = Some(Box::new(move || cb()));
        }
        if let Some(cb) = &self.on_long_press {
            let cb = cb.clone();
            gesture.on_long_press = Some(Box::new(move || cb()));
        }

        Box::new(ButtonElement {
            style: self.style,
            hover_style: self.hover_style,
            is_disabled: self.is_disabled,
            is_hovered: AtomicBool::new(false),
            is_pressed: AtomicBool::new(false),
            gesture: Mutex::new(gesture),
            child,
            cache: LayoutCache::new(),
            cached_bounds: Mutex::new(None),
        })
    }
}
#[allow(dead_code)]
pub struct ButtonElement {
    style: ButtonStyle,
    hover_style: ButtonStyle,
    is_disabled: bool,
    is_hovered: AtomicBool,
    is_pressed: AtomicBool,
    gesture: Mutex<GestureDetector>,
    child: Box<dyn Element>,
    cache: LayoutCache,
    /// Cached absolute bounding rect, updated during draw.
    cached_bounds: Mutex<Option<Rect>>,
}

impl ButtonElement {
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
            PointerEvent::Down(_) => {
                self.is_pressed.store(true, Ordering::Relaxed);
            }
            PointerEvent::Up(_) => {
                self.is_pressed.store(false, Ordering::Relaxed);
            }
            PointerEvent::Move(_) => {
                // Only set hovered if not currently pressed to avoid
                // style glitches during press-and-drag.
                if !self.is_pressed.load(Ordering::Relaxed) {
                    self.is_hovered.store(true, Ordering::Relaxed);
                }
            }
            PointerEvent::Cancel => {
                self.is_pressed.store(false, Ordering::Relaxed);
                self.is_hovered.store(false, Ordering::Relaxed);
            }
        }

        // Feed into gesture detector for tap/long-press recognition
        self.gesture.lock().unwrap().handle_pointer_event(event);
    }

    fn active_style(&self) -> &ButtonStyle {
        if self.is_hovered.load(Ordering::Relaxed) && !self.is_disabled {
            &self.hover_style
        } else {
            &self.style
        }
    }
}

impl Element for ButtonElement {
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
        *self.cached_bounds.lock().unwrap() = Some(Rect::from_xywh(abs_x, abs_y, box_width, box_height));

        // Draw pressed overlay for visual feedback
        if self.is_pressed.load(Ordering::Relaxed) && !self.is_disabled {
            let mut pressed_paint = Paint::default();
            pressed_paint.set_anti_alias(true);
            pressed_paint.set_color(SkColor::from_argb(40, 0, 0, 0));
            pressed_paint.set_style(Style::Fill);
            ctx.canvas.draw_rect(rect, &pressed_paint);
        }

        // Draw child centered within the button bounds
        let child_size = self.child.computed_size(ctx);
        let offset_x = (box_width - child_size.width).max(0.0) / 2.0;
        let offset_y = (box_height - child_size.height).max(0.0) / 2.0;

        ctx.canvas.save();
        ctx.canvas.translate((offset_x, offset_y));

        let child_ctx = BuildContext {
            parent_size: ResolvedSize {
                width: box_width,
                height: box_height,
            },
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
        };
        Self::render_child(self.child.as_ref(), &child_ctx);

        ctx.canvas.restore();
    }

    fn size(&self) -> Option<Size> {
        let style = self.active_style();
        Some(Size {
            width: style.width,
            height: style.height,
        })
    }

    // We don't implement visit_children here because ButtonElement handles
    // its own child rendering in draw(). Exposing children via visit_children
    // would cause the external render_widget_tree to draw them a second time.

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

        ResolvedSize {
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }

    fn on_event(&self, event: &ElementEvent) -> bool {
        if self.is_disabled {
            return false;
        }

        // Hit-test against cached bounds
        let pos = match event {
            ElementEvent::PointerDown(p) | ElementEvent::PointerUp(p) | ElementEvent::PointerMove(p) => p,
        };
        if let Some(bounds) = *self.cached_bounds.lock().unwrap() {
            if pos.x < bounds.left || pos.x > bounds.right || pos.y < bounds.top || pos.y > bounds.bottom {
                return false;
            }
        }

        let pointer_event = match event {
            ElementEvent::PointerDown(pos) => PointerEvent::Down(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::PointerUp(pos) => PointerEvent::Up(PointerPosition { x: pos.x, y: pos.y }),
            ElementEvent::PointerMove(pos) => PointerEvent::Move(PointerPosition { x: pos.x, y: pos.y }),
        };

        self.handle_pointer_event(&pointer_event);

        // PointerMove during a press should not trigger a redraw
        !matches!(event, ElementEvent::PointerMove(_))
    }
}
