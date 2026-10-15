#![cfg(feature = "icons")]

use aimer::{Icons, Svg, SvgColor, SvgStyle};

#[test]
fn icons_feature_also_exports_svg_and_its_styling_api() {
    let _ = Svg::new(Icons::HOUSE).width(32)
        .style("path", SvgStyle::new().stroke(SvgColor::rgba8(0, 128, 255, 255)));
    assert!(std::ptr::eq(Icons::HOUSE, aimer::icons::Icons::HOUSE));
}
