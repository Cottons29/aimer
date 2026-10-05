use aimer::canvas::{FrameCanvas, InnerCanvas};
use aimer::cupid::draw_cmd::DrawCommand;
use aimer::cupid::draw_cmd_v2::{DrawCommand as V2DrawCommand, Rect, RenderPaintSource, RenderTree};
use aimer::widget::base::WindowHandle;
use aimer::{
    BuildContext, Color, CustomShape, FillStyle, Glass, LayoutElement, Liquid, ResolvedSize,
    Scalable, ShapeClip, ShapeColor, ShapeFit, ShapeHitTest, ShapePath, SizedBox, StrokeStyle,
    Vec2d, Widget,
};

fn context<'a>(canvas: FrameCanvas<'a>, runtime: &tokio::runtime::Runtime) -> BuildContext<'a> {
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
    let canvas = FrameCanvas::new(&inner);
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
    let canvas = FrameCanvas::new(&inner);
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

#[test]
fn custom_shape_records_svg_paint_in_its_local_v2_list() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let inner = InnerCanvas::new();
    let frame_canvas = FrameCanvas::new(&inner);
    let context = context(frame_canvas, &runtime);
    let element = CustomShape::new()
        .path(square_path())
        .fill(FillStyle::solid(ShapeColor::BLACK))
        .child(SizedBox::new().width(64.0).height(64.0))
        .to_element(&context);
    element.layout(&context);

    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 64.0, 64.0)).unwrap();
    let retained_context = tree.context(root).unwrap();
    context.with_local_v2_paint_context(retained_context, |context| {
        assert!(element.can_paint_local_v2(context));
        element.paint_local_v2(context);
    });

    let commands = tree.draw_list_snapshot(root).unwrap().commands;
    assert!(matches!(commands.as_ref(), [V2DrawCommand::Svg { .. }]));
}

#[test]
fn scalable_keeps_its_child_as_a_scaled_retained_node() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let inner = InnerCanvas::new();
    let frame_canvas = FrameCanvas::new(&inner);
    let context = context(frame_canvas, &runtime);
    let element = Scalable::new()
        .scale(1.5)
        .child(SizedBox::new().width(32.0).height(32.0))
        .to_element(&context);
    element.layout(&context);

    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 64.0, 64.0)).unwrap();
    let child = tree
        .add_child(root, Rect::new(0.0, 0.0, 32.0, 32.0))
        .unwrap();
    tree.set_paint_source(child, RenderPaintSource::LocalV2)
        .unwrap();
    tree.context(child)
        .unwrap()
        .begin_recording()
        .unwrap()
        .commit(vec![V2DrawCommand::FillRect {
            rect: Rect::new(0.0, 0.0, 32.0, 32.0),
            color: aimer::cupid::utilities::Color::white(),
            border_radius: [0.0; 4],
            border_width: [0.0; 4],
            border_color: aimer::cupid::utilities::Color::transparent(),
            outline_width: [0.0; 4],
            outline_color: aimer::cupid::utilities::Color::transparent(),
        }])
        .unwrap();
    let retained_context = tree.context(root).unwrap();
    context.with_local_v2_paint_context(retained_context, |context| {
        assert!(element.can_paint_local_v2(context));
        assert!(element.sync_local_v2_state(context));
        element.paint_local_v2(context);

        element.visit_children(&mut |child| {
            let child_context = element.retained_v2_child_context_at(context, child, 0);
            assert_eq!(child_context.as_ref().map(|ctx| ctx.scale), Some(1.5));
        });
    });

    assert_eq!(
        tree.element_bounds(child).unwrap(),
        Rect::new(0.0, 0.0, 48.0, 48.0)
    );
}

#[test]
fn glass_and_liquid_record_material_requests_in_their_local_v2_lists() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let inner = InnerCanvas::new();
    let frame_canvas = FrameCanvas::new(&inner);
    let context = context(frame_canvas, &runtime);

    let glass = Glass::new()
        .child(SizedBox::new().width(32.0).height(24.0))
        .to_element(&context);
    glass.layout(&context);
    let tree = RenderTree::new();
    let glass_node = tree.add_root(Rect::new(0.0, 0.0, 64.0, 64.0)).unwrap();
    let retained_context = tree.context(glass_node).unwrap();
    context.with_local_v2_paint_context(retained_context, |context| {
        assert!(glass.can_paint_local_v2(context));
        glass.paint_local_v2(context);
    });
    let glass_commands = tree.draw_list_snapshot(glass_node).unwrap().commands;
    assert!(glass_commands.iter().any(|command| matches!(
        command,
        V2DrawCommand::DrawCustom { pipeline_name, data }
            if pipeline_name.as_ref() == "aimer.material"
                && data.len() == aimer::canvas::MaterialDrawRequest::PACKET_LEN
    )));

    let liquid = Liquid::new()
        .child(SizedBox::new().width(32.0).height(24.0))
        .to_element(&context);
    liquid.layout(&context);
    let liquid_node = tree.add_root(Rect::new(0.0, 0.0, 64.0, 64.0)).unwrap();
    let retained_context = tree.context(liquid_node).unwrap();
    context.with_local_v2_paint_context(retained_context, |context| {
        assert!(liquid.can_paint_local_v2(context));
        liquid.paint_local_v2(context);
    });
    let liquid_commands = tree.draw_list_snapshot(liquid_node).unwrap().commands;
    assert!(liquid_commands.iter().any(|command| matches!(
        command,
        V2DrawCommand::DrawCustom { pipeline_name, data }
            if pipeline_name.as_ref() == "aimer.material"
                && data.len() == aimer::canvas::MaterialDrawRequest::PACKET_LEN
    )));
}
