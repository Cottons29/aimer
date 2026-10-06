//! Corner radii on a retained clip must survive lowering to the renderer's
//! `PushClip`, for local v2 lists and for legacy islands alike.

use crate::damage_region::{DamageRect, DamageSet};
use crate::draw_cmd::{DrawCommand as LegacyDrawCommand, DrawList as LegacyDrawList};
use crate::draw_cmd_v2::{
    DrawCommand, Rect, RenderFrame, RenderNodeId, RenderNodeSpec, RenderOp, RenderPaintSource,
    RenderTree,
};
use crate::frame::{
    Frame, FramePacket, FrameRenderMetadata, RetainedRenderOperationKind, RetainedV2Item,
};
use crate::utilities::Color;

/// Top-left, top-right, bottom-right, bottom-left, in logical pixels.
const RADIUS: [f32; 4] = [4.0, 5.0, 6.0, 7.0];
const SCALE: f32 = 2.0;

fn clipped_root(source: RenderPaintSource, radius: [f32; 4]) -> (RenderTree, RenderNodeId) {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 64.0, 64.0)).unwrap();
    tree.sync_structure_with_clip_radii(
        &[RenderNodeSpec::new(
            Some(root),
            None,
            Rect::new(0.0, 0.0, 64.0, 64.0),
        )],
        &[Some(Rect::new(0.0, 0.0, 40.0, 40.0))],
        &[radius],
    )
    .unwrap();
    tree.set_paint_source(root, source).unwrap();
    (tree, root)
}

fn lowered_local_v2(radius: [f32; 4]) -> Vec<LegacyDrawCommand> {
    let (tree, root) = clipped_root(RenderPaintSource::LocalV2, radius);
    tree.context(root)
        .unwrap()
        .begin_recording()
        .unwrap()
        .commit(vec![DrawCommand::FillRect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            color: Color::rgba8(20, 40, 60, 255),
            border_radius: [0.0; 4],
            border_width: [0.0; 4],
            border_color: Color::transparent(),
            outline_width: [0.0; 4],
            outline_color: Color::transparent(),
        }])
        .unwrap();
    let item = tree
        .render_all()
        .into_iter()
        .find_map(|operation| match operation {
            RenderOp::Draw(item) => Some(item),
            _ => None,
        })
        .expect("retained root item");
    let snapshot = item.snapshot();
    let retained = RetainedV2Item {
        element: item.element,
        revision: snapshot.revision,
        bounds: item.bounds,
        origin: item.origin,
        transform: item.transform,
        clip: item.clip,
        clip_radius: item.clip_radius,
        commands: snapshot.commands,
    };
    super::lower_retained_v2_commands(&retained, SCALE).unwrap()
}

fn push_clip_radii(commands: &[LegacyDrawCommand]) -> Vec<[f32; 4]> {
    commands
        .iter()
        .filter_map(|command| match command {
            LegacyDrawCommand::PushClip { border_radius, .. } => Some(*border_radius),
            _ => None,
        })
        .collect()
}

/// A legacy island that owns one fill, clipped by a rounded node clip.
fn island_scenario() -> (RenderFrame, Frame) {
    let (tree, root) = clipped_root(RenderPaintSource::LegacyIsland, RADIUS);
    tree.begin_legacy_frame();
    let mut legacy = LegacyDrawList::new();
    let start = legacy.commands().len();
    legacy.fill_rect(
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Color::rgba8(20, 220, 20, 255),
        [0.0; 4],
        [0.0; 4],
        Color::transparent(),
    );
    let end = legacy.commands().len();
    tree.set_legacy_command_range(root, Some((start, end)))
        .unwrap();
    (
        RenderFrame {
            damage: vec![Rect::new(0.0, 0.0, 64.0, 64.0)],
            operations: tree.render_all(),
        },
        Frame::new(legacy, 128, 128),
    )
}

fn metadata() -> FrameRenderMetadata {
    FrameRenderMetadata::new(SCALE, 1, 1, 1, 1, DamageSet::new(128, 128))
}

#[test]
fn a_local_v2_clip_carries_the_corner_radius() {
    // The clip is pushed under the logical-to-device scale transform, so the
    // renderer scales the radius with the rectangle.
    assert_eq!(push_clip_radii(&lowered_local_v2(RADIUS)), vec![RADIUS]);
}

#[test]
fn a_square_clip_still_lowers_with_zero_radii() {
    assert_eq!(push_clip_radii(&lowered_local_v2([0.0; 4])), vec![[0.0; 4]]);
}

#[test]
fn a_direct_island_replay_clips_with_the_radius_in_device_pixels() {
    let (render_frame, legacy_frame) = island_scenario();
    let packet =
        FramePacket::from_v2_direct_with_legacy(render_frame, metadata(), legacy_frame).unwrap();
    let plan = packet.render_plan().expect("direct retained render plan");
    let operations = plan
        .operations_for_region(DamageRect::new(0, 0, 128, 128))
        .collect::<Vec<_>>();
    let RetainedRenderOperationKind::LegacyRange { prefix, .. } = &operations[0].kind else {
        panic!("the island must replay as a legacy range");
    };
    // Island clips are pushed in device space, outside any scale transform.
    assert_eq!(push_clip_radii(prefix), vec![RADIUS.map(|radius| radius * SCALE)]);
}

#[test]
fn a_flattened_island_clips_with_the_radius_in_device_pixels() {
    let (render_frame, legacy_frame) = island_scenario();
    let packet = FramePacket::from_v2_with_legacy(render_frame, metadata(), legacy_frame).unwrap();
    assert_eq!(
        push_clip_radii(packet.frame().draw_list.commands()),
        vec![RADIUS.map(|radius| radius * SCALE)]
    );
}
