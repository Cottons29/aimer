use aimer_attribute::BoxConstraint;
use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_macro::{Constructor, WidgetConstructor};
use aimer_widget::base::BuildContext;
use aimer_widget::{ Drawable, Element, Widget};

#[allow(dead_code)]
#[derive(WidgetConstructor)]
pub struct Positioned<W: Widget + 'static> {
    pub child: W,
    #[constructor(default)]
    pub position: Position,
    #[constructor(default, into)]
    pub left: Dimension,
    #[constructor(default, into)]
    pub top: Dimension,
    #[constructor(default, into)]
    pub right: Dimension,
    #[constructor(default, into)]
    pub bottom: Dimension,
    #[constructor(default)]
    pub transform: Transform,
    #[constructor(default)]
    pub layer: u32,
}

impl<W: Widget> Widget for Positioned<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        Box::new(RawPositionedElement {
            child,
            position: self.position,
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            transform: self.transform,
            layer: self.layer,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default, Copy)]
pub enum Transform {
    Translate(f32, f32),
    TranslateX(f32),
    TranslateY(f32),
    Scale(f32, f32),
    ScaleX(f32),
    ScaleY(f32),
    // Matrix(Vec<f32>),
    Rotate(f32), // radians
    #[default]
    None,
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub enum Position {
    Absolute,
    #[default]
    Relative,
    Static,
    Sticky,
    Fixed,
}

#[allow(dead_code)]
#[derive(Constructor)]
pub struct RawPositionedElement<E: Element> {
    pub(crate) child: E,
    pub(crate) position: Position,
    pub(crate) left: Dimension,
    pub(crate) top: Dimension,
    pub(crate) right: Dimension,
    pub(crate) bottom: Dimension,
    pub(crate) transform: Transform,
    pub(crate) layer: u32,
}

impl<E: Element> Drawable for RawPositionedElement<E> {
    fn draw(&self, ctx: &BuildContext) {
        // debug!("Positioned::draw");
        let is_auto = self.top == Dimension::Auto
            && self.left == Dimension::Auto
            && self.right == Dimension::Auto
            && self.bottom == Dimension::Auto;

        if is_auto && self.transform == Transform::None {
            self.child.draw(ctx);
            return;
        }

        ctx.canvas.save();

        let mut offset_x = 0.0;
        let mut offset_y = 0.0;

        if !is_auto {
            let child_size = self.child.content_size(ctx);

            if self.left != Dimension::Auto {
                offset_x = self.left.resolve(ctx.parent_size.width, ctx.scale);
            } else if self.right != Dimension::Auto {
                offset_x =
                    ctx.parent_size.width - self.right.resolve(ctx.parent_size.width, ctx.scale) - child_size.width;
            }

            if self.top != Dimension::Auto {
                offset_y = self.top.resolve(ctx.parent_size.height, ctx.scale);
            } else if self.bottom != Dimension::Auto {
                offset_y =
                    ctx.parent_size.height - self.bottom.resolve(ctx.parent_size.height, ctx.scale) - child_size.height;
            }
        }

        ctx.canvas.translate(Vec2d { x: offset_x, y: offset_y });

        match &self.transform {
            Transform::Translate(tx, ty) => {
                ctx.canvas.translate(Vec2d { x: *tx, y: *ty });
            }
            Transform::TranslateX(tx) => {
                ctx.canvas.translate(Vec2d { x: *tx, y: 0.0 });
            }
            Transform::TranslateY(ty) => {
                ctx.canvas.translate(Vec2d { x: 0.0, y: *ty });
            }
            Transform::Scale(sx, sy) => {
                ctx.canvas.scale(*sx , *sy );
            }
            Transform::ScaleX(sx) => {
                ctx.canvas.scale(*sx , 1.0);
            }
            Transform::ScaleY(sy) => {
                ctx.canvas.scale(1.0, *sy );
            }
            Transform::Rotate(rad) => {
                ctx.canvas.rotate(*rad );
            }
            Transform::None => {}
        }

        if is_auto {
            self.child.draw(ctx);
        } else {
            let parent_pos = ctx.parent_pos;

            let parent_size = if let Some(size) = self.child.get_size_from_child() {
                size.resolve(&ctx.parent_size, ctx.scale)
            } else {
                ctx.parent_size
            };

            let child_constraint = BoxConstraint!(
                max_width: parent_size.width,
                max_height: parent_size.height,
            );


            let child_ctx = BuildContext {
                parent_size,
                canvas: ctx.canvas.clone(),
                scale: ctx.scale,
                parent_pos,
                cursor_pos: ctx.cursor_pos,
                box_constraint: child_constraint,
                visible_rect: ctx.visible_rect,
                window: ctx.window,
                #[cfg(not(target_arch = "wasm32"))]
                async_handle: ctx.async_handle.clone(),
                inherited_states: ctx.inherited_states.clone(),
            };

            self.child.draw(&child_ctx);
        }
        ctx.canvas.restore();
    }
}

impl<E: Element> Element for RawPositionedElement<E> {


    fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Positioned handles its own child rendering in draw() with proper offset,
        // so we don't expose children here to avoid double-rendering at (0,0).
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn layer(&self) -> u32 {
        self.layer
    }
}
