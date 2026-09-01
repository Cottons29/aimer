use aimer::{Glass, GlassMaterial, SizedBox, Widget};

#[test]
fn glass_exposes_bounded_shader_lighting_controls() {
    let material = GlassMaterial::new()
        .edge_lighting(2.0)
        .specular_highlight(-1.0);

    assert_eq!(material.edge_lighting_value(), 1.0);
    assert_eq!(material.specular_highlight_value(), 0.0);

    fn assert_widget(_: impl Widget) {}
    assert_widget(
        Glass::new()
            .edge_lighting(0.8)
            .specular_highlight(0.4)
            .child(SizedBox::new().width(80.0).height(40.0)),
    );
}
