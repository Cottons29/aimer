use aimer_cupid::draw_cmd_v2::{
    DrawCommand, Rect, RenderNodeSpec, RenderOp, RenderPaintSource, RenderTree,
};
use aimer_cupid::utilities::{Color, Mat3};
use std::sync::Arc;

#[test]
fn writer_commits_commands_to_only_its_element_local_list() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    let child = tree
        .add_child(root, Rect::new(10.0, 12.0, 40.0, 30.0))
        .unwrap();

    let context = tree.context(child).unwrap();
    let writer = context.begin_recording().unwrap();
    writer.commit(vec![fill_command(Rect::new(0.0, 0.0, 40.0, 30.0))]).unwrap();

    let child_list = tree.draw_list_snapshot(child).unwrap();
    assert_eq!(child_list.revision, 1);
    assert!(matches!(child_list.commands[0], DrawCommand::FillRect { .. }));
    assert_eq!(tree.draw_list_revision(root).unwrap(), 0);
    assert!(tree.draw_list_snapshot(root).unwrap().commands.is_empty());
}

#[test]
fn local_command_snapshots_share_storage_until_the_list_changes() {
    let tree = RenderTree::new();
    let node = tree.add_root(Rect::new(0.0, 0.0, 40.0, 30.0)).unwrap();
    let context = tree.context(node).unwrap();
    context
        .begin_recording()
        .unwrap()
        .commit(vec![fill_command(Rect::new(0.0, 0.0, 20.0, 20.0))])
        .unwrap();

    let first = tree.draw_list_snapshot(node).unwrap();
    let unchanged = tree.draw_list_snapshot(node).unwrap();
    assert!(Arc::ptr_eq(&first.commands, &unchanged.commands));

    context
        .begin_recording()
        .unwrap()
        .commit(vec![fill_command(Rect::new(0.0, 0.0, 24.0, 20.0))])
        .unwrap();
    let changed = tree.draw_list_snapshot(node).unwrap();
    assert_eq!(changed.revision, first.revision + 1);
    assert!(!Arc::ptr_eq(&first.commands, &changed.commands));
}

#[test]
fn writer_rejects_unbalanced_scopes_without_replacing_the_retained_list() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    let context = tree.context(root).unwrap();

    context
        .begin_recording()
        .unwrap()
        .commit(vec![fill_command(Rect::new(0.0, 0.0, 20.0, 20.0))])
        .unwrap();
    let before = tree.draw_list_snapshot(root).unwrap();

    assert!(context
        .begin_recording()
        .unwrap()
        .commit(vec![DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 12.0, 12.0),
            border_radius: [0.0; 4],
        }])
        .is_err());

    let after = tree.draw_list_snapshot(root).unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.commands.len(), before.commands.len());
    assert!(matches!(after.commands.first(), Some(DrawCommand::FillRect { .. })));
    assert!(tree.dirty_elements().contains(&root));

    drop(context.begin_recording().unwrap());
    assert!(tree.dirty_elements().contains(&root));
    assert!(context.begin_recording().is_ok());
}

#[test]
fn geometry_and_opacity_changes_produce_a_damage_limited_group_plan() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    let child = tree
        .add_child(root, Rect::new(10.0, 10.0, 30.0, 30.0))
        .unwrap();
    record_fill(&tree, root);
    record_fill(&tree, child);
    tree.take_damage();

    tree.set_bounds(child, Rect::new(20.0, 10.0, 30.0, 30.0))
        .unwrap();
    tree.set_opacity(root, 0.5).unwrap();
    let frame = tree.render_pending();

    assert!(!frame.damage.is_empty());
    assert!(matches!(
        frame.operations.first(),
        Some(RenderOp::BeginOpacityGroup { element, opacity, .. })
            if *element == root && *opacity == 0.5
    ));
    assert!(matches!(frame.operations.last(), Some(RenderOp::EndOpacityGroup { element }) if *element == root));
}

#[test]
fn compositor_transform_updates_descendant_geometry_without_rerecording_lists() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    let child = tree
        .add_child(root, Rect::new(10.0, 20.0, 40.0, 20.0))
        .unwrap();
    tree.set_paint_source(root, RenderPaintSource::LocalV2)
        .unwrap();
    tree.set_paint_source(child, RenderPaintSource::LocalV2)
        .unwrap();
    record_fill(&tree, root);
    record_fill(&tree, child);
    let root_revision = tree.draw_list_revision(root).unwrap();
    let child_revision = tree.draw_list_revision(child).unwrap();
    tree.take_damage();

    let transform = Mat3::translate(50.0, 50.0)
        .mul(&Mat3::scale(0.5, 0.5))
        .mul(&Mat3::translate(-50.0, -50.0));
    tree.set_transform(root, transform).unwrap();

    let damage = tree.take_damage();
    assert!(!damage.is_empty());
    let operations = tree.render_all();
    let child_item = operations.iter().find_map(|operation| match operation {
        RenderOp::Draw(item) if item.element == child => Some(item),
        _ => None,
    });
    let child_item = child_item.expect("transformed child remains in render order");
    assert_eq!(child_item.bounds, Rect::new(30.0, 35.0, 20.0, 10.0));
    assert_eq!(child_item.transform, transform);
    assert_eq!(tree.draw_list_revision(root).unwrap(), root_revision);
    assert_eq!(tree.draw_list_revision(child).unwrap(), child_revision);
}

#[test]
fn build_context_updates_child_presentation_without_rerecording_lists() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    let child = tree
        .add_child(root, Rect::new(10.0, 10.0, 20.0, 20.0))
        .unwrap();
    tree.set_paint_source(root, RenderPaintSource::LocalV2)
        .unwrap();
    tree.set_paint_source(child, RenderPaintSource::LocalV2)
        .unwrap();
    record_fill(&tree, root);
    record_fill(&tree, child);
    let root_revision = tree.draw_list_revision(root).unwrap();
    let child_revision = tree.draw_list_revision(child).unwrap();
    tree.take_damage();

    let context = tree.context(root).unwrap();
    assert!(context.set_child_presentation_at(0, Mat3::scale(1.5, 0.5), 0.4));

    assert!(!tree.take_damage().is_empty());
    assert_eq!(
        tree.element_bounds(child).unwrap(),
        Rect::new(10.0, 10.0, 30.0, 10.0)
    );
    let child_item = tree.render_all().into_iter().find_map(|operation| match operation {
        RenderOp::Draw(item) if item.element == child => Some(item),
        _ => None,
    });
    let child_item = child_item.expect("the presented child remains in render order");
    let expected_transform = Mat3::translate(10.0, 10.0)
        .mul(&Mat3::scale(1.5, 0.5))
        .mul(&Mat3::translate(-10.0, -10.0));
    assert_eq!(child_item.transform, expected_transform);
    assert_eq!(child_item.opacity, 0.4);
    assert_eq!(tree.draw_list_revision(root).unwrap(), root_revision);
    assert_eq!(tree.draw_list_revision(child).unwrap(), child_revision);
}

#[test]
fn compositor_transform_rejects_non_finite_values_and_culls_zero_area_content() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    tree.set_paint_source(root, RenderPaintSource::LocalV2)
        .unwrap();
    record_fill(&tree, root);
    tree.take_damage();

    assert_eq!(
        tree.set_transform(root, Mat3::scale(f32::NAN, 1.0)),
        Err(aimer_cupid::draw_cmd_v2::RenderTreeError::InvalidTransform)
    );
    assert!(tree.take_damage().is_empty());

    let collapse = Mat3::translate(50.0, 50.0)
        .mul(&Mat3::scale(0.0, 0.0))
        .mul(&Mat3::translate(-50.0, -50.0));
    tree.set_transform(root, collapse).unwrap();
    assert!(!tree.take_damage().is_empty());
    assert!(tree.render_all().is_empty());
}

#[test]
fn invalid_geometry_and_opacity_are_rejected_without_adding_damage() {
    let tree = RenderTree::new();
    assert_eq!(
        tree.add_root(Rect::new(0.0, 0.0, -1.0, 10.0)),
        Err(aimer_cupid::draw_cmd_v2::RenderTreeError::InvalidBounds)
    );

    let root = tree.add_root(Rect::new(0.0, 0.0, 10.0, 10.0)).unwrap();
    tree.take_damage();
    assert_eq!(
        tree.set_opacity(root, f32::NAN),
        Err(aimer_cupid::draw_cmd_v2::RenderTreeError::InvalidOpacity)
    );
    assert_eq!(
        tree.set_opacity(root, 1.1),
        Err(aimer_cupid::draw_cmd_v2::RenderTreeError::InvalidOpacity)
    );
    assert!(tree.take_damage().is_empty());
}

#[test]
fn disjoint_nested_clips_hide_descendants_without_creating_damage() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(10.0, 10.0, 40.0, 40.0)).unwrap();
    let child = tree
        .add_child(root, Rect::new(40.0, 40.0, 20.0, 20.0))
        .unwrap();
    tree.set_clip(root, Some(Rect::new(0.0, 0.0, 20.0, 20.0)))
        .unwrap();
    tree.set_clip(child, Some(Rect::new(0.0, 0.0, 20.0, 20.0)))
        .unwrap();
    tree.set_paint_source(root, RenderPaintSource::LocalV2)
        .unwrap();
    tree.set_paint_source(child, RenderPaintSource::LocalV2)
        .unwrap();
    tree.take_damage();

    let operations = tree.render_all();
    assert!(matches!(
        operations.as_slice(),
        [RenderOp::Draw(item)] if item.element == root
    ));

    tree.invalidate_paint(child).unwrap();
    assert!(tree.take_damage().is_empty());
}

#[test]
fn structure_sync_moves_a_clipped_viewport_child_atomically() {
    let tree = RenderTree::new();
    let viewport = tree.add_root(Rect::new(0.0, 0.0, 100.0, 80.0)).unwrap();
    let content = tree
        .add_child(viewport, Rect::new(0.0, 0.0, 100.0, 1_000.0))
        .unwrap();
    tree.set_clip(content, Some(Rect::new(0.0, 0.0, 100.0, 80.0)))
        .unwrap();
    tree.take_damage();

    tree.sync_structure_with_clips(
        &[
            RenderNodeSpec::new(Some(viewport), None, Rect::new(0.0, 0.0, 100.0, 80.0)),
            RenderNodeSpec::new(
                Some(content),
                Some(0),
                Rect::new(0.0, -400.0, 100.0, 1_000.0),
            ),
        ],
        &[None, Some(Rect::new(0.0, 400.0, 100.0, 80.0))],
    )
    .unwrap();

    let damage = tree.take_damage();
    assert!(!damage.is_empty());
    assert!(damage.iter().all(|rect| {
        rect.x >= 0.0
            && rect.y >= 0.0
            && rect.x + rect.width <= 100.0
            && rect.y + rect.height <= 80.0
    }), "viewport sync damaged intermediate offscreen geometry: {damage:?}");
}

#[test]
fn subtree_local_v2_check_accepts_fully_retained_descendants() {
    let tree = RenderTree::new();
    let viewport = tree.add_root(Rect::new(0.0, 0.0, 100.0, 80.0)).unwrap();
    let content = tree
        .add_child(viewport, Rect::new(0.0, 0.0, 100.0, 1_000.0))
        .unwrap();
    tree.set_paint_source(viewport, RenderPaintSource::LocalV2)
        .unwrap();
    tree.set_paint_source(content, RenderPaintSource::LocalV2)
        .unwrap();

    assert!(tree.subtree_uses_only_local_v2(viewport).unwrap());
}

#[test]
fn subtree_local_v2_check_rejects_unresolved_descendants() {
    let tree = RenderTree::new();
    let viewport = tree.add_root(Rect::new(0.0, 0.0, 100.0, 80.0)).unwrap();
    let content = tree
        .add_child(viewport, Rect::new(0.0, 0.0, 100.0, 1_000.0))
        .unwrap();
    let leaf = tree
        .add_child(content, Rect::new(0.0, 0.0, 10.0, 10.0))
        .unwrap();
    tree.set_paint_source(viewport, RenderPaintSource::LocalV2)
        .unwrap();
    tree.set_paint_source(content, RenderPaintSource::LocalV2)
        .unwrap();

    assert!(!tree.subtree_uses_only_local_v2(viewport).unwrap());

    tree.set_paint_source(leaf, RenderPaintSource::LocalV2)
        .unwrap();
    assert!(tree
        .subtree_uses_only_local_v2(viewport)
        .unwrap());

    tree.set_paint_source(leaf, RenderPaintSource::Unresolved)
        .unwrap();
    assert!(!tree
        .subtree_uses_only_local_v2(viewport)
        .unwrap());
}

#[test]
fn structure_sync_rejects_clips_that_do_not_match_the_node_list() {
    let tree = RenderTree::new();
    let node = tree.add_root(Rect::new(0.0, 0.0, 20.0, 20.0)).unwrap();
    tree.take_damage();

    assert_eq!(
        tree.sync_structure_with_clips(
            &[RenderNodeSpec::new(Some(node), None, Rect::new(10.0, 10.0, 20.0, 20.0))],
            &[],
        ),
        Err(aimer_cupid::draw_cmd_v2::RenderTreeError::InvalidBounds)
    );
    assert_eq!(tree.element_bounds(node).unwrap(), Rect::new(0.0, 0.0, 20.0, 20.0));
    assert!(tree.take_damage().is_empty());
}

fn fill_command(rect: Rect) -> DrawCommand {
    DrawCommand::FillRect {
        rect,
        color: Color::rgba8(1, 2, 3, 255),
        border_radius: [0.0; 4],
        border_width: [0.0; 4],
        border_color: Color::transparent(),
        outline_width: [0.0; 4],
        outline_color: Color::transparent(),
    }
}

/// A local list is authored against the node's size (a background fills it), so
/// a node that grows must be recorded again or the new area stays unpainted.
#[test]
fn resizing_a_node_marks_its_recorded_list_stale_but_moving_it_does_not() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    record_fill(&tree, root);
    assert!(tree.dirty_elements().is_empty());

    tree.set_bounds(root, Rect::new(10.0, 5.0, 100.0, 100.0)).unwrap();
    assert!(tree.dirty_elements().is_empty(), "a move keeps the recorded list");

    tree.set_bounds(root, Rect::new(10.0, 5.0, 140.0, 100.0)).unwrap();
    assert_eq!(tree.dirty_elements(), vec![root]);
}

#[test]
fn structure_sync_that_resizes_a_node_marks_its_recorded_list_stale() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    let child = tree.add_child(root, Rect::new(0.0, 0.0, 40.0, 40.0)).unwrap();
    record_fill(&tree, root);
    record_fill(&tree, child);
    assert!(tree.dirty_elements().is_empty());

    // Same structure: only the root grows.
    tree.sync_structure_with_clips(
        &[
            RenderNodeSpec::new(Some(root), None, Rect::new(0.0, 0.0, 160.0, 100.0)),
            RenderNodeSpec::new(Some(child), Some(0), Rect::new(0.0, 0.0, 40.0, 40.0)),
        ],
        &[None, None],
    )
    .unwrap();
    assert_eq!(tree.dirty_elements(), vec![root]);
    record_fill(&tree, root);

    // A new sibling changes the structure while the child grows.
    tree.sync_structure_with_clips(
        &[
            RenderNodeSpec::new(Some(root), None, Rect::new(0.0, 0.0, 160.0, 100.0)),
            RenderNodeSpec::new(Some(child), Some(0), Rect::new(0.0, 0.0, 60.0, 40.0)),
            RenderNodeSpec::new(None, Some(0), Rect::new(70.0, 0.0, 20.0, 20.0)),
        ],
        &[None, None, None],
    )
    .unwrap();
    let dirty = tree.dirty_elements();
    assert!(dirty.contains(&child), "the grown child must be recorded again");
    assert!(!dirty.contains(&root), "the unchanged root keeps its list");
}

fn record_fill(tree: &RenderTree, node: aimer_cupid::draw_cmd_v2::RenderNodeId) {
    tree.context(node)
        .unwrap()
        .begin_recording()
        .unwrap()
        .commit(vec![fill_command(Rect::new(0.0, 0.0, 20.0, 20.0))])
        .unwrap();
}
