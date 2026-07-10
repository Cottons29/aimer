pub(crate) mod border_radius;
pub(crate) mod box_shadow;
pub(crate) mod shapes;

use crate::style::border::{BoxBorder, BoxOutline};
use crate::style::box_decoration::border_radius::BorderRadius;
use crate::style::box_decoration::box_shadow::BoxShadow;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_color::prelude::Color;
use aimer_macro::Constructor;

use aimer_widget::Drawable;
use aimer_widget::base::BuildContext;

#[derive(Default, Clone, PartialEq, Debug, Constructor)]
pub struct BoxDecoration {
    #[constructor(default)]
    pub border: BoxBorder,
    #[constructor(default)]
    pub outline: BoxOutline,
    #[constructor(default, into)]
    pub border_radius: BorderRadius,
    #[constructor(default, dyn_iter, into)]
    pub box_shadow: Vec<BoxShadow>,
    #[constructor(default = Option::None, into)]
    pub background_color: Option<Color>,
}

impl From<BoxShadow> for Vec<BoxShadow> {
    fn from(shadow: BoxShadow) -> Self {
        vec![shadow]
    }
}

impl BoxDecoration {
    pub fn update_color(&self, new_color: impl Into<Color>) {
        #[allow(unused_mut)]
        let mut bg_ptr = &self.background_color as *const Option<Color> as *mut Option<Color>;
        unsafe {
            *bg_ptr = Some(new_color.into());
        }
    }
}

impl Drawable for BoxDecoration {
    fn draw(&self, ctx: &BuildContext) {
        let box_width = ctx.parent_size.width;
        let box_height = ctx.parent_size.height;
        let scale = ctx.scale;

        let radii = self.border_radius.resolve(box_width, box_height, scale);

        // Draw box shadows (outer shadows drawn before background, inset shadows drawn after)
        for shadow in &self.box_shadow {
            if shadow.inset {
                continue; // inset shadows are drawn after the background
            }
            Self::draw_shadow(ctx, shadow, box_width, box_height, &radii);
        }

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
                self.border.effective_color(box_width, box_height, scale), // uniform color: use the first visible side
                [o_widths.1, o_widths.2, o_widths.3, o_widths.0],
                self.outline.effective_color(box_width, box_height, scale),
            );
        } else if let Some(color) = self.background_color {
            ctx.canvas.fill_color_rect_per_corner(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize { width: box_width, height: box_height },
                color,
                radii,
            );
        }

        // Draw inset shadows after background
        for shadow in &self.box_shadow {
            if !shadow.inset {
                continue;
            }
            Self::draw_shadow(ctx, shadow, box_width, box_height, &radii);
        }
    }
}

impl BoxDecoration {
    /// Draws a box shadow using a single GPU draw call with SDF-based Gaussian blur.
    fn draw_shadow(ctx: &BuildContext, shadow: &BoxShadow, box_width: f32, box_height: f32, radii: &[f32; 4]) {
        // Early-out for fully transparent or invisible shadows
        if shadow.color == Color::Transparent {
            return;
        }
        let blur = shadow.blur.max(0.0);
        let spread = shadow.spread;
        if blur == 0.0 && spread == 0.0 && shadow.offset_x == 0.0 && shadow.offset_y == 0.0 && !shadow.inset {
            return;
        }

        let side_params = shadow.side.to_shader_params();
        ctx.canvas.draw_shadow_rect(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize { width: box_width, height: box_height },
            shadow.color,
            [shadow.offset_x, shadow.offset_y, blur, spread],
            *radii,
            shadow.inset,
            [side_params.0, side_params.1, side_params.2],
        );
    }
}
