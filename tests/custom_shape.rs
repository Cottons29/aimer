use aimer::canvas::{Canvas, InnerCanvas};
use aimer::cupid::draw_cmd::DrawCommand;
use aimer::widget::base::WindowHandle;
use aimer::{
    BuildContext, Color, CustomShape, FillStyle, LayoutElement, ResolvedSize, ShapeClip,
    ShapeColor, ShapeFit, ShapeHitTest, ShapePath, SizedBox, StrokeStyle, Vec2d, Widget,
};

fn context<'a>(canvas: Canvas<'a>, runtime: &tokio::runtime::Runtime) -> BuildContext<'a> {
    BuildContext::new(
        canvas,
        ResolvedSize {
            width: 64.0,
            height: 64.0,
        },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(winit::dpi::PhysicalSize::new(64, 64), 1.0),
        runtime.handle().clone(),
    )
}

fn square_path() -> ShapePath {
    ShapePath::builder()
        .move_to(8.0, 8.0)
        .line_to(40.0, 8.0)
        .line_to(40.0, 40.0)
        .line_to(8.0, 40.0)
        .close()
        .build()
        .expect("finite test shape")
}

#[test]
fn custom_shape_submits_a_fitted_background_before_its_child() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let context = context(canvas, &runtime);
    let fill = FillStyle::solid(ShapeColor::rgba8(220, 40, 40, 255));
    let stroke = StrokeStyle::new(2.0, ShapeColor::rgba8(40, 20, 20, 255))
        .expect("finite test stroke");
    let stroke_width = stroke.width;

    let element = CustomShape::new()
        .path(square_path())
        .fill(fill)
        .stroke(stroke)
        .clip(ShapeClip::Bounds)
        .fit(ShapeFit::None)
        .hit_test(ShapeHitTest::FillOrStroke)
        .opacity(0.75)
        .child(
            SizedBox::new()
                .width(64.0)
                .height(64.0)
                .color(Color::Rgba(0, 0, 0, 255)),
        )
        .to_element(&context);
    element.layout(&context);
    element.draw(&context);

    let draw_list = inner.take_draw_list();
    let svg_index = draw_list
        .commands()
        .iter()
        .position(|command| matches!(command, DrawCommand::Svg { .. }))
        .expect("custom shape should submit one SVG-backed draw");
    let DrawCommand::Svg { scene, .. } = &draw_list.commands()[svg_index] else {
        unreachable!("the position above identifies the SVG command")
    };
    let node = &scene.nodes[0];
    assert_eq!(node.transform, Default::default());
    assert_eq!(node.opacity, 0.75);
    assert_eq!(
        node.fill.as_ref().expect("fill is retained").color,
        aimer::cupid::svg::SvgColor::rgba8(220, 40, 40, 255),
    );
    assert_eq!(
        node.stroke.as_ref().expect("stroke is retained").width,
        stroke_width,
    );
    let clip = draw_list
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::PushClip { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("bounds clipping should target the transformed path bounds");
    assert_eq!(clip.x, 8.0);
    assert_eq!(clip.y, 8.0);
    assert_eq!(clip.width, 32.0);
    assert_eq!(clip.height, 32.0);
    let child_index = draw_list
        .commands()
        .iter()
        .position(|command| matches!(command, DrawCommand::FillRect { .. }))
        .expect("the retained child should still paint");
    assert!(svg_index < child_index);
}

#[test]
fn custom_shape_invalid_opacity_keeps_the_child_and_skips_shape_paint() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let context = context(canvas, &runtime);

    let element = CustomShape::new()
        .path(square_path())
        .fill(FillStyle::solid(ShapeColor::BLACK))
        .opacity(f32::NAN)
        .child(SizedBox::new().width(64.0).height(64.0))
        .to_element(&context);
    element.layout(&context);
    element.draw(&context);

    let draw_list = inner.take_draw_list();
    assert!(!draw_list
        .commands()
        .iter()
        .any(|command| matches!(command, DrawCommand::Svg { .. })));
    assert!(draw_list
        .commands()
        .iter()
        .any(|command| matches!(command, DrawCommand::FillRect { .. })));
}
