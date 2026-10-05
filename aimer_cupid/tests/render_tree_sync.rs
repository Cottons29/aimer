use aimer_cupid::draw_cmd_v2::{
    DrawCommand, Rect, RenderNodeSpec, RenderOp, RenderTree, RenderTreeError,
};
use aimer_cupid::utilities::Color;

#[test]
fn sync_keeps_ids_and_local_paint_while_reordering_and_resizing_nodes() {
    let tree = RenderTree::new();
    let initial = [
        RenderNodeSpec::new(None, None, Rect::new(0.0, 0.0, 300.0, 200.0)),
        RenderNodeSpec::new(None, Some(0), Rect::new(10.0, 10.0, 40.0, 30.0)),
        RenderNodeSpec::new(None, Some(0), Rect::new(100.0, 10.0, 50.0, 30.0)),
    ];
    let ids = tree.sync_structure(&initial).unwrap();
    let root = ids[0];
    let left = ids[1];
    let right = ids[2];

    tree.context(left)
        .unwrap()
        .begin_recording()
        .unwrap()
        .commit(vec![DrawCommand::FillRect {
            rect: Rect::new(0.0, 0.0, 40.0, 30.0),
            color: Color::rgba8(20, 80, 180, 255),
            border_radius: [0.0; 4],
            border_width: [0.0; 4],
            border_color: Color::transparent(),
            outline_width: [0.0; 4],
            outline_color: Color::transparent(),
        }])
        .unwrap();
    assert_eq!(tree.draw_list_revision(left).unwrap(), 1);
    let _ = tree.take_damage();

    let reordered = [
        RenderNodeSpec::new(Some(root), None, Rect::new(0.0, 0.0, 300.0, 200.0)),
        RenderNodeSpec::new(Some(right), Some(0), Rect::new(100.0, 10.0, 50.0, 30.0)),
        RenderNodeSpec::new(Some(left), Some(0), Rect::new(20.0, 10.0, 60.0, 30.0)),
    ];
    let reordered_ids = tree.sync_structure(&reordered).unwrap();

    assert_eq!(reordered_ids, [root, right, left]);
    assert_eq!(tree.parent_of(right).unwrap(), Some(root));
    assert_eq!(tree.parent_of(left).unwrap(), Some(root));
    assert_eq!(tree.element_bounds(left).unwrap(), Rect::new(20.0, 10.0, 60.0, 30.0));
    assert_eq!(tree.draw_list_revision(left).unwrap(), 1);
    let draw_order = tree
        .render_all()
        .into_iter()
        .filter_map(|operation| match operation {
            RenderOp::Draw(item) => Some(item.element),
            RenderOp::BeginOpacityGroup { .. } | RenderOp::EndOpacityGroup { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(draw_order, [root, right, left]);
    assert!(!tree.take_damage().is_empty());
}

#[test]
fn sync_rejects_duplicate_ids_and_removes_nodes_omitted_from_the_next_snapshot() {
    let tree = RenderTree::new();
    let initial = [
        RenderNodeSpec::new(None, None, Rect::new(0.0, 0.0, 100.0, 100.0)),
        RenderNodeSpec::new(None, Some(0), Rect::new(0.0, 0.0, 10.0, 10.0)),
    ];
    let ids = tree.sync_structure(&initial).unwrap();
    let duplicate = [
        RenderNodeSpec::new(Some(ids[0]), None, Rect::new(0.0, 0.0, 100.0, 100.0)),
        RenderNodeSpec::new(Some(ids[1]), Some(0), Rect::new(0.0, 0.0, 10.0, 10.0)),
        RenderNodeSpec::new(Some(ids[1]), Some(0), Rect::new(10.0, 0.0, 10.0, 10.0)),
    ];

    assert_eq!(
        tree.sync_structure(&duplicate),
        Err(RenderTreeError::DuplicateNode(ids[1]))
    );

    let invalid_parent = [
        RenderNodeSpec::new(Some(ids[0]), None, Rect::new(0.0, 0.0, 100.0, 100.0)),
        RenderNodeSpec::new(Some(ids[1]), Some(1), Rect::new(0.0, 0.0, 10.0, 10.0)),
    ];
    assert!(matches!(
        tree.sync_structure(&invalid_parent),
        Err(RenderTreeError::InvalidParentIndex {
            node: 1,
            parent: 1
        })
    ));

    let without_child = [RenderNodeSpec::new(
        Some(ids[0]),
        None,
        Rect::new(0.0, 0.0, 100.0, 100.0),
    )];
    tree.sync_structure(&without_child).unwrap();
    assert!(matches!(
        tree.context(ids[1]),
        Err(RenderTreeError::UnknownNode(id)) if id == ids[1]
    ));
}
