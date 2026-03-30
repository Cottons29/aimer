pub(crate) mod border_radius;
pub(crate) mod box_shadow;
pub(crate) mod shapes;

use color::prelude::Color;
use constructor::Constructor;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use crate::style::border::{BoxBorder, BoxOutline, RawBoxBorder, BorderMode};
use crate::style::box_decoration::border_radius::BorderRadius;
use crate::style::box_decoration::box_shadow::BoxShadow;


use crate::base::BuildContext;
use crate::Drawable;

#[derive(Default, Clone, PartialEq, Debug, Constructor)]
pub struct BoxDecoration {
    #[constructor(default)]
    pub border: BoxBorder,
    #[constructor(default)]
    pub outline: BoxOutline,
    #[constructor(default, into)]
    pub border_radius: BorderRadius,
    #[constructor(default,dyn_iter)]
    pub box_shadow: Vec<BoxShadow>,
    #[constructor(default, into)]
    pub background_color: Option<Color>,
}

impl Drawable for BoxDecoration {
    fn draw(&self, ctx: &BuildContext) {
        let box_width = ctx.parent_size.width;
        let box_height = ctx.parent_size.height;
        let scale = ctx.scale;

        let radii = self.border_radius.resolve(box_width, box_height, scale);

        // Draw background color
        if let Some(color) = self.background_color {
            ctx.canvas.fill_color_rect_per_corner(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize { width: box_width, height: box_height },
                color,
                radii,
            );
        }

        let border = RawBoxBorder::new(
            self.border.left,
            self.border.right,
            self.border.top,
            self.border.bottom,
            BorderMode::Inside,
            radii,
        );
        border.draw(ctx);

        let outline = RawBoxBorder::new(
            self.outline.left,
            self.outline.right,
            self.outline.top,
            self.outline.bottom,
            BorderMode::Outside,
            radii,
        );
        outline.draw(ctx);
    }
}