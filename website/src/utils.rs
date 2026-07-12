use aimer::{BuildContext, Dimension};
use aimer::Dimension::Percent;
use aimer::provider::media_query::MediaQuery;
use aimer::style::{LayoutSpacing, Spacing};

pub fn app_padding(_: &BuildContext) -> LayoutSpacing {
    let horizontal_padding = 20f64;
    LayoutSpacing::new()
        .left(horizontal_padding)
        .right(horizontal_padding)
        .top(Spacing::Px(20))
        .bottom(Spacing::Px(20))
}

pub fn is_mobile(ctx: &BuildContext) -> bool {
    let window_size = MediaQuery::of(ctx).size;
    window_size.width < 600f32
}

pub fn resp_position(ctx: &BuildContext, wide: f32, slim: f32) -> Dimension {
    if is_mobile(ctx) { Percent(slim) } else { Percent(wide) }
}

pub fn mobile_title(ctx: &BuildContext) -> u32 {
    if is_mobile(ctx) { 30 } else { 44 }
}