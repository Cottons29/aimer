//! An element with no retained paint paints nothing; it is not drawn legacy.
//!
//! The legacy canvas path is being removed. An element that only draws through
//! `update` (it does not implement `paint_local_v2`) used to be captured as a
//! legacy island and painted by replaying its recorded commands. It now paints
//! nothing and stays unresolved, so the frame asks it again next time, and
//! debug builds report the element.

use aimer::{
    AimerApp, AnyElement, BuildContext, Color, Drawable, Element, EventElement, LayoutElement,
    Rebuildable, ResolvedSize, Size, VisitorElement, Widget,
};

struct LegacyPainter;

impl Widget for LegacyPainter {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        Element::boxed(self)
    }
}

impl aimer::PortableWidget for LegacyPainter {}

impl VisitorElement for LegacyPainter {
    fn debug_name(&self) -> &'static str {
        "LegacyPainter"
    }
}

impl EventElement for LegacyPainter {}
impl Rebuildable for LegacyPainter {}

impl LayoutElement for LegacyPainter {
    fn size(&self) -> Option<Size> {
        Some(Size::new(40.0, 40.0))
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ResolvedSize {
            width: 40.0 * ctx.scale,
            height: 40.0 * ctx.scale,
        }
    }
}

impl Drawable for LegacyPainter {
    fn update(&self, ctx: &BuildContext) {
        ctx.canvas.fill_color_rect(
            (0.0, 0.0).into(),
            ResolvedSize {
                width: 40.0 * ctx.scale,
                height: 40.0 * ctx.scale,
            },
            Color::Rgb(255, 0, 0),
            [0.0; 4],
        );
    }
}

#[test]
fn an_element_without_retained_paint_paints_nothing_and_is_asked_again() {
    let mut app = AimerApp::start_headless(LegacyPainter);
    for _ in 0..3 {
        app.render_frame();
    }

    let census = app.paint_source_census().expect("a mounted page has a root");
    assert!(
        census
            .unresolved_roots
            .iter()
            .any(|element| element.debug_name == "LegacyPainter"),
        "the element has no paint to show: {:?}",
        census.unresolved_roots
    );
    assert!(census.drawn_unmapped.is_empty(), "{:?}", census.drawn_unmapped);
    assert_eq!(census.sync_error, None);
    // Debug builds name the element whose drawing was dropped.
    #[cfg(debug_assertions)]
    assert!(
        census
            .dropped_paint
            .iter()
            .any(|element| element.debug_name == "LegacyPainter"),
        "{:?}",
        census.dropped_paint
    );
}
