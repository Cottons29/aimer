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

        // Draw combined background, border and outline if possible
        if self.border.has_visible_border(box_width, box_height, scale) || self.outline.has_visible_outline(box_width, box_height, scale) {
            let b_widths = self.border.strokes(box_width, box_height, scale);
            let o_widths = self.outline.strokes(box_width, box_height, scale);
            
            // Note: fill_rect_with_border_and_outline_per_side currently only supports uniform border/outline color in this API call.
            // If colors are different per side, it would need multiple calls or a more complex shader.
            // But usually border/outline have uniform color per BoxBorder/BoxOutline.
            
            ctx.canvas.fill_rect_with_border_and_outline_per_side(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize { width: box_width, height: box_height },
                self.background_color.unwrap_or(Color::Transparent),
                radii,
                [b_widths.1, b_widths.2, b_widths.3, b_widths.0], // stroke_rect_per_side uses [top, right, bottom, left]
                self.border.left.color, // Assuming uniform color for now
                [o_widths.1, o_widths.2, o_widths.3, o_widths.0],
                self.outline.left.color,
            );
        } else if let Some(color) = self.background_color {
            ctx.canvas.fill_color_rect_per_corner(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize { width: box_width, height: box_height },
                color,
                radii,
            );
        }
    }
}