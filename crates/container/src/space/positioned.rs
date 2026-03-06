use attribute::dimension::Dimension;
use attribute::size::{ResolvedSize, Size};
use constructor::Constructor;
use utils::debug;
use widget::base::BuildContext;
use widget::style::BoxConstraint;
use widget::{BoxConstraint, Drawable, Element, Widget};

#[cfg(target_arch = "wasm32")]
type Float = f64;
#[cfg(not(target_arch = "wasm32"))]
type Float = f32;

#[allow(dead_code)]
#[derive(Constructor)]
pub struct Positioned<W: Widget> {
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
    Translate(Float, Float),
    TranslateX(Float),
    TranslateY(Float),
    Scale(Float, Float),
    ScaleX(Float),
    ScaleY(Float),
    // Matrix(Vec<Float>),
    Rotate(Float), // radians
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

        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.translate((offset_x, offset_y));
        #[cfg(target_arch = "wasm32")]
        let _ = ctx.canvas.translate(offset_x.into(), offset_y.into());

        match &self.transform {
            Transform::Translate(tx, ty) => {
                #[cfg(not(target_arch = "wasm32"))]
                ctx.canvas.translate((*tx, *ty));
                #[cfg(target_arch = "wasm32")]
                let _ = ctx.canvas.translate((*tx).into(), (*ty).into());
            }
            Transform::TranslateX(tx) => {
                #[cfg(not(target_arch = "wasm32"))]
                ctx.canvas.translate((*tx, 0.0));
                #[cfg(target_arch = "wasm32")]
                let _ = ctx.canvas.translate((*tx).into(), 0.0);
            }
            Transform::TranslateY(ty) => {
                #[cfg(not(target_arch = "wasm32"))]
                ctx.canvas.translate((0.0, *ty));
                #[cfg(target_arch = "wasm32")]
                let _ = ctx.canvas.translate(0.0, (*ty).into());
            }
            Transform::Scale(sx, sy) => {
                #[cfg(not(target_arch = "wasm32"))]
                ctx.canvas.scale((*sx, *sy));
                #[cfg(target_arch = "wasm32")]
                let _ = ctx.canvas.scale((*sx).into(), (*sy).into());
            }
            Transform::ScaleX(sx) => {
                #[cfg(not(target_arch = "wasm32"))]
                ctx.canvas.scale((*sx, 1.0));
                #[cfg(target_arch = "wasm32")]
                let _ = ctx.canvas.scale((*sx).into(), 1.0);
            }
            Transform::ScaleY(sy) => {
                #[cfg(not(target_arch = "wasm32"))]
                ctx.canvas.scale((1.0, *sy));
                #[cfg(target_arch = "wasm32")]
                let _ = ctx.canvas.scale(1.0, (*sy).into());
            }
            Transform::Rotate(rad) => {
                #[cfg(not(target_arch = "wasm32"))]
                ctx.canvas.rotate(rad * 180.0 / std::f32::consts::PI, None);
                #[cfg(target_arch = "wasm32")]
                let _ = ctx.canvas.rotate((*rad).into());
            }
            // Transform::Matrix(_) => {
            //     // TODO: Implement matrix transform
            // }
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
                canvas: ctx.canvas,
                scale: ctx.scale,
                parent_pos,
                cursor_pos: ctx.cursor_pos,
                box_constraint: child_constraint,
                visible_rect: ctx.visible_rect,
                window: ctx.window,
                #[cfg(not(target_arch = "wasm32"))]
                async_handle: ctx.async_handle.clone(),
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
