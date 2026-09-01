#[path = "../src/glass_liquid_example.rs"]
mod glass_liquid_example;

use aimer::{GlassMaterial, LiquidMaterial, Widget};

#[test]
fn glass_and_liquid_showcase_exposes_a_public_widget_constructor() {
    fn assert_widget(_: impl Widget) {}

    assert_widget(glass_liquid_example::glass_liquid_example());
}

#[test]
fn glass_and_liquid_builders_keep_finite_bounded_values() {
    let glass = GlassMaterial::new()
        .blur_radius(10_000.0)
        .corner_radius(f32::NAN)
        .quality(255);
    assert_eq!(
        glass.blur_radius_value(),
        GlassMaterial::MAX_BLUR_RADIUS
    );
    assert_eq!(glass.corner_radii_value(), [16.0; 4]);
    assert_eq!(glass.quality_value(), 4);

    let intensity_glass = GlassMaterial::new().blur_intensity(0.75);
    assert_eq!(intensity_glass.blur_intensity_value(), 0.75);

    let liquid = LiquidMaterial::new()
        .distortion_strength(-1.0)
        .animation_time(f32::NAN)
        .interaction(2.0);
    assert_eq!(liquid.distortion_strength_value(), 0.0);
    assert_eq!(liquid.animation_time_value(), 0.0);
    assert_eq!(liquid.interaction_value(), 1.0);
}
