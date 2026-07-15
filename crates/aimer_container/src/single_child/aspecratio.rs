use aimer_attribute::size::ResolvedSize;
use aimer_attribute::{BoxConstraint, Size};
use aimer_macro::{EventElement, Rebuildable};
use aimer_widget::base::BuildContext;
use aimer_widget::{Drawable, Element, LayoutElement, RequiredChild, VisitorElement, Widget};

#[derive(Clone, Copy)]
pub enum RatioOption {
    Width,
    Height,
}

#[allow(dead_code)]
pub struct AspectRatio<W = RequiredChild> {
    pub aspect_ratio: f32,
    ratio_option: RatioOption,
    pub child: W,
}

impl AspectRatio {
    pub fn new() -> Self {
        Self { aspect_ratio: 1.0, child: RequiredChild, ratio_option: RatioOption::Width }
    }

    pub fn aspect_ratio(mut self, aspect_ratio: f32) -> Self {
        self.aspect_ratio = aspect_ratio;
        self
    }

    pub fn ratio_option(mut self, ratio_option: RatioOption) -> Self {
        self.ratio_option = ratio_option;
        self
    }

    pub fn child<C: Widget>(self, child: C) -> AspectRatio<C> {
        AspectRatio { aspect_ratio: self.aspect_ratio, ratio_option: self.ratio_option, child }
    }
}

impl Default for AspectRatio {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: Widget> Widget for AspectRatio<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawAspectRatio {
            child: self
                .child
                .to_element(ctx),
            aspect_ratio: self
                .aspect_ratio
                .abs(),
            ratio_option: self.ratio_option,
        })
    }
}

fn resolve_ratio_size(constraints: BoxConstraint, aspect_ratio: f32) -> (f32, f32) {
    resolve_ratio_size_with_option(constraints, aspect_ratio, RatioOption::Width)
}

fn resolve_ratio_size_with_option(
    constraints: BoxConstraint,
    aspect_ratio: f32,
    ratio_option: RatioOption,
) -> (f32, f32) {
    let ratio = if aspect_ratio.is_finite() && aspect_ratio > 0.0 { aspect_ratio } else { 1.0 };
    let width_bounded = constraints
        .max_width
        .is_finite()
        && constraints.max_width < f32::MAX;
    let height_bounded = constraints
        .max_height
        .is_finite()
        && constraints.max_height < f32::MAX;

    let (mut width, mut height) = if matches!(ratio_option, RatioOption::Width) && width_bounded {
        let width = constraints.max_width;
        let height = width / ratio;
        if height_bounded && height > constraints.max_height {
            (constraints.max_height * ratio, constraints.max_height)
        } else {
            (width, height)
        }
    } else if height_bounded {
        let height = constraints.max_height;
        let width = height * ratio;
        if width_bounded && width > constraints.max_width {
            (constraints.max_width, constraints.max_width / ratio)
        } else {
            (width, height)
        }
    } else if width_bounded {
        (constraints.max_width, constraints.max_width / ratio)
    } else if constraints.min_width > 0.0 {
        (constraints.min_width, constraints.min_width / ratio)
    } else {
        (constraints.min_height * ratio, constraints.min_height)
    };

    if width < constraints.min_width {
        width = constraints.min_width;
        height = width / ratio;
    }
    if height < constraints.min_height {
        height = constraints.min_height;
        width = height * ratio;
    }

    (width.min(constraints.max_width), height.min(constraints.max_height))
}

#[derive(EventElement, Rebuildable)]
struct RawAspectRatio {
    child: Box<dyn Element>,
    aspect_ratio: f32,
    ratio_option: RatioOption,
}

impl Drawable for RawAspectRatio {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let mut child_ctx = ctx.clone();
        child_ctx.parent_size = size;
        child_ctx.box_constraint = BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: size.width,
            max_height: size.height,
        };
        self.child
            .draw(&child_ctx);
    }
}

impl LayoutElement for RawAspectRatio {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let (width, height) = resolve_ratio_size_with_option(
            ctx.box_constraint,
            self.aspect_ratio,
            self.ratio_option,
        );
        ResolvedSize { width, height }
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.computed_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.child
            .layer()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child
            .get_size_from_child()
    }
}

impl VisitorElement for RawAspectRatio {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(
            self.child
                .as_ref(),
        );
    }

    fn debug_name(&self) -> &'static str {
        "AspectRatio"
    }
}

#[cfg(test)]
mod tests {
    use aimer_attribute::BoxConstraint;

    use super::{RatioOption, resolve_ratio_size, resolve_ratio_size_with_option};

    #[test]
    fn ratio_size_uses_largest_size_inside_constraints() {
        let constraints =
            BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: 320.0, max_height: 200.0 };

        assert_eq!(resolve_ratio_size(constraints, 16.0 / 9.0), (320.0, 180.0));
        assert_eq!(resolve_ratio_size(constraints, 0.5), (100.0, 200.0));
    }

    #[test]
    fn ratio_size_honors_minimum_constraints() {
        let constraints = BoxConstraint {
            min_width: 150.0,
            min_height: 100.0,
            max_width: 300.0,
            max_height: 300.0,
        };

        assert_eq!(resolve_ratio_size(constraints, 2.0), (300.0, 150.0));
    }

    #[test]
    fn height_driven_ratio_still_fits_the_width_constraint() {
        let constraints =
            BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: 300.0, max_height: 200.0 };

        assert_eq!(
            resolve_ratio_size_with_option(constraints, 2.0, RatioOption::Height),
            (300.0, 150.0)
        );
    }
}
