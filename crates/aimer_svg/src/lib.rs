mod document;
mod error;
mod selector;
mod source;
mod style;
mod widget;

pub use aimer_cupid::svg::{SvgColor, SvgFillRule, SvgNodeId, SvgTransform};
pub use document::{SvgDiagnostic, SvgDocument, SvgLimits, SvgPath};
pub use error::SvgError;
pub use selector::SvgSelector;
pub use source::{SvgLoadState, SvgLoader, SvgSource};
pub use style::SvgStyle;
pub use widget::{RawSvg, Svg, SvgAsset, SvgCallback, SvgHit, SvgNodeMetadata};

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_attribute::Bounds;
    use aimer_cupid::svg::{SvgElementKind, SvgPathCommand};

    use crate::widget;
    use aimer_widget::Widget;

    use crate::{SvgAsset, SvgDocument, SvgError, SvgLimits, SvgPath, SvgSelector, SvgStyle};

    #[test]
    fn svg_asset_exposes_the_asset_widget_contract() {
        let widget = SvgAsset::new("assets/icon.svg")
            .width(24.0)
            .height(32.0);

        assert_eq!(widget.debug_name(), "SvgAsset");
    }

    #[test]
    fn parses_viewbox_groups_transforms_and_solid_path_style() {
        let document = SvgDocument::from_svg(
            br##"<svg viewBox="0 0 24 12" xmlns="http://www.w3.org/2000/svg">
                <g id="layer" class="interactive foreground" transform="translate(2 3)" opacity="0.5">
                    <path id="mark" class="accent" d="M0 0 L10 0 L10 5 Z"
                          fill="#336699" fill-rule="evenodd"
                          stroke="#ff0000" stroke-width="2" stroke-linecap="round" stroke-linejoin="bevel"/>
                </g>
            </svg>"##,
        )
        .expect("valid SVG should parse");

        assert_eq!(
            document
                .scene()
                .viewport
                .width,
            24.0
        );
        assert_eq!(
            document
                .scene()
                .viewport
                .height,
            12.0
        );
        let mark = document
            .select("#mark")
            .expect("selector should parse");
        assert_eq!(mark.len(), 1);
        let node = document
            .scene()
            .node(mark[0])
            .expect("selected node exists");
        assert_eq!(node.element, SvgElementKind::Path);
        assert_eq!(
            node.classes
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
            ["accent"]
        );
        assert_eq!(node.opacity, 0.5);
        assert_eq!(node.transform.tx, 2.0);
        assert_eq!(node.transform.ty, 3.0);
        assert!(node.fill.is_some());
        assert!(node.stroke.is_some());
    }

    #[test]
    fn selectors_match_id_class_and_element_name_in_paint_order() {
        let document = SvgDocument::from_svg(
            br#"<svg width="20" height="10" xmlns="http://www.w3.org/2000/svg">
                <path id="first" class="hot shared" d="M0 0h2v2z"/>
                <path id="second" class="shared" d="M3 0h2v2z"/>
            </svg>"#,
        )
        .unwrap();

        assert_eq!(
            document
                .select("path")
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            document
                .select(".shared")
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            document
                .select(".hot")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            document
                .select("#missing")
                .unwrap(),
            []
        );
        assert!(matches!(
            "#".parse::<SvgSelector>(),
            Err(SvgError::InvalidSelector(_))
        ));
    }

    #[test]
    fn standalone_path_data_and_selected_svg_path_are_retained() {
        let path = SvgPath::from_path_data("M1 2 Q3 4 5 6 C7 8 9 10 11 12 Z").unwrap();
        assert!(matches!(path.commands()[0], SvgPathCommand::MoveTo { .. }));
        assert!(matches!(
            path.commands()[1],
            SvgPathCommand::QuadraticTo { .. }
        ));
        assert!(matches!(path.commands()[2], SvgPathCommand::CubicTo { .. }));
        assert!(matches!(path.commands()[3], SvgPathCommand::Close));

        let selected = SvgPath::from_svg(
            br#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><path id="p" d="M0 0h10v10z"/></svg>"#,
            "#p",
        )
        .unwrap();
        assert!(!selected.commands().is_empty());
    }

    #[test]
    fn rejects_empty_malformed_non_finite_and_oversized_input() {
        assert!(matches!(
            SvgDocument::from_svg([]),
            Err(SvgError::EmptyInput)
        ));
        assert!(matches!(
            SvgDocument::from_svg(b"<svg>"),
            Err(SvgError::Parse(_))
        ));
        assert!(matches!(
            SvgPath::from_path_data("M NaN 0"),
            Err(SvgError::InvalidPath(_))
        ));

        let limits = SvgLimits {
            max_source_bytes: 8,
            ..SvgLimits::default()
        };
        assert!(matches!(
            SvgDocument::from_svg_with_limits(b"<svg width='1' height='1'/>", limits),
            Err(SvgError::LimitExceeded {
                resource: "source bytes",
                ..
            })
        ));
    }

    #[test]
    fn rejects_external_resources_and_reports_deferred_content() {
        let external = br#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><image href="https://example.com/a.png"/></svg>"#;
        assert!(matches!(
            SvgDocument::from_svg(external),
            Err(SvgError::ExternalResource(_))
        ));

        let gradient = br#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="g"/></defs><path d="M0 0h10v10z" fill="url(#g)"/></svg>"#;
        let document = SvgDocument::from_svg(gradient).unwrap();
        assert!(
            document
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.feature == "gradient")
        );
    }

    #[test]
    fn intrinsic_size_uses_viewport_and_preserves_ratio_for_one_dimension() {
        let viewport = aimer_cupid::svg::SvgViewport {
            width: 40.0,
            height: 20.0,
        };
        assert_eq!(
            widget::resolved_svg_size(viewport, None, None),
            (40.0, 20.0)
        );
        assert_eq!(
            widget::resolved_svg_size(viewport, Some(100.0), None),
            (100.0, 50.0)
        );
        assert_eq!(
            widget::resolved_svg_size(viewport, None, Some(50.0)),
            (100.0, 50.0)
        );
    }

    #[test]
    fn style_overrides_affect_matching_nodes_only() {
        let document = SvgDocument::from_svg(
            br##"<svg width="20" height="10" xmlns="http://www.w3.org/2000/svg">
                <path id="left" class="accent" d="M0 0h8v8z" fill="#000000"/>
                <path id="right" d="M10 0h8v8z" fill="#000000"/>
            </svg>"##,
        )
        .unwrap();
        let style = SvgStyle::new().fill(aimer_cupid::svg::SvgColor::rgba8(255, 0, 0, 255));
        let overrides =
            widget::overrides_for_rules(document.scene(), &[(".accent".parse().unwrap(), style)]);

        assert_eq!(overrides.len(), 1);
        assert_eq!(
            document
                .scene()
                .node(overrides[0].node_id)
                .unwrap()
                .svg_id
                .as_deref(),
            Some("left")
        );
    }

    #[test]
    fn hit_testing_uses_reverse_paint_order_and_nested_transform() {
        let document = SvgDocument::from_svg(
            br##"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
                <path id="back" d="M0 0h10v10z" fill="#000000"/>
                <g transform="translate(2 3)"><path id="front" d="M0 0h10v10z" fill="#ff0000"/></g>
            </svg>"##,
        )
        .unwrap();

        let hit = widget::hit_test_scene(
            document.scene(),
            Bounds::new(0.0, 0.0, 20.0, 20.0),
            5.0,
            5.0,
            &[],
        )
        .unwrap();
        assert_eq!(hit.metadata.svg_id.as_deref(), Some("front"));
        let back = widget::hit_test_scene(
            document.scene(),
            Bounds::new(0.0, 0.0, 20.0, 20.0),
            1.0,
            1.0,
            &[],
        )
        .unwrap();
        assert_eq!(
            back.metadata
                .svg_id
                .as_deref(),
            Some("back")
        );
    }

    #[test]
    fn stroke_hit_testing_rejects_points_outside_stroke_width() {
        let document = SvgDocument::from_svg(
            br##"<svg width="20" height="20" xmlns="http://www.w3.org/2000/svg">
                <path id="line" d="M2 10h16" fill="none" stroke="#000000" stroke-width="2"/>
            </svg>"##,
        )
        .unwrap();

        assert!(
            widget::hit_test_scene(
                document.scene(),
                Bounds::new(0.0, 0.0, 20.0, 20.0),
                10.0,
                10.8,
                &[]
            )
            .is_some()
        );
        assert!(
            widget::hit_test_scene(
                document.scene(),
                Bounds::new(0.0, 0.0, 20.0, 20.0),
                10.0,
                13.0,
                &[]
            )
            .is_none()
        );
    }

    #[test]
    fn press_lifecycle_requires_release_on_the_same_path() {
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let mut interaction = widget::SvgInteraction::default();
        let node = aimer_cupid::svg::SvgNodeId(4);

        interaction.pointer_down(Some(node));
        assert_eq!(interaction.pointer_up(Some(node)), Some(node));
        observed.set(observed.get() + 1);
        interaction.pointer_down(Some(node));
        assert_eq!(interaction.pointer_up(None), None);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn source_groups_remain_selectable_when_normalizer_flattens_them() {
        let document = SvgDocument::from_svg(
            br#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg">
                <g class="cluster"><path id="child" d="M0 0h2v2z"/></g>
            </svg>"#,
        )
        .unwrap();

        let groups = document.select("g").unwrap();
        let clusters = document
            .select(".cluster")
            .unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups, clusters);
        let child = document
            .select("#child")
            .unwrap()[0];
        assert_eq!(
            document
                .scene()
                .node(child)
                .unwrap()
                .parent,
            Some(groups[0])
        );
    }

    #[test]
    fn enforces_node_command_and_viewport_limits_and_non_finite_values() {
        let two_paths = br#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/><path d="M2 0h1v1z"/></svg>"#;
        let node_limits = SvgLimits {
            max_nodes: 1,
            ..SvgLimits::default()
        };
        assert!(matches!(
            SvgDocument::from_svg_with_limits(two_paths, node_limits),
            Err(SvgError::LimitExceeded {
                resource: "nodes",
                ..
            })
        ));

        let command_limits = SvgLimits {
            max_path_commands: 2,
            ..SvgLimits::default()
        };
        assert!(matches!(
            SvgDocument::from_svg_with_limits(two_paths, command_limits),
            Err(SvgError::LimitExceeded {
                resource: "path commands",
                ..
            })
        ));

        let viewport_limits = SvgLimits {
            max_viewport_dimension: 5.0,
            ..SvgLimits::default()
        };
        assert!(matches!(
            SvgDocument::from_svg_with_limits(two_paths, viewport_limits),
            Err(SvgError::LimitExceeded {
                resource: "viewport dimension",
                ..
            })
        ));

        let non_finite = br#"<svg width="10" height="10" xmlns="http://www.w3.org/2000/svg"><path transform="matrix(NaN 0 0 1 0 0)" d="M0 0h1v1z"/></svg>"#;
        assert!(matches!(
            SvgDocument::from_svg(non_finite),
            Err(SvgError::NonFinite)
        ));
    }
}
