//! The paint-order plan caches how element identities map to node positions.
//!
//! The cache is keyed on the tree's topology, so the properties that matter
//! are that an unchanged topology never has to be resolved again, and that
//! every edit which can change parents, children or sibling order is seen on
//! the next plan.

use super::*;

fn drawn(tree: &RenderTree) -> Vec<RenderNodeId> {
    tree.render_all()
        .into_iter()
        .filter_map(|op| match op {
            RenderOp::Draw(item) => Some(item.element),
            _ => None,
        })
        .collect()
}

fn rect(x: f32, y: f32) -> Rect {
    Rect::new(x, y, 20.0, 20.0)
}

/// root -> [a -> [a1, a2], b]
fn sample() -> (RenderTree, [RenderNodeId; 5]) {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 200.0, 200.0)).unwrap();
    let a = tree.add_child(root, rect(0.0, 0.0)).unwrap();
    let a1 = tree.add_child(a, rect(1.0, 1.0)).unwrap();
    let a2 = tree.add_child(a, rect(2.0, 2.0)).unwrap();
    let b = tree.add_child(root, rect(30.0, 0.0)).unwrap();
    (tree, [root, a, a1, a2, b])
}

#[test]
fn nodes_are_planned_parent_first_in_sibling_order() {
    let (tree, [root, a, a1, a2, b]) = sample();
    assert_eq!(drawn(&tree), vec![root, a, a1, a2, b]);
}

#[test]
fn an_unchanged_topology_is_not_resolved_again() {
    let (tree, [root, a, a1, a2, b]) = sample();
    assert_eq!(drawn(&tree), vec![root, a, a1, a2, b]);

    // Dropping the identity index proves the second plan resolves parents and
    // children from the cached positions instead of looking each one up.
    tree.draw_cmd.borrow_mut().indices.clear();

    assert_eq!(drawn(&tree), vec![root, a, a1, a2, b]);
}

#[test]
fn moving_a_node_replans_its_geometry_without_a_topology_change() {
    let (tree, [_, a, a1, _, _]) = sample();
    let before = tree.render_all();
    tree.set_bounds(a, Rect::new(50.0, 40.0, 20.0, 20.0)).unwrap();

    let origin_of = |ops: &[RenderOp], id| {
        ops.iter().find_map(|op| match op {
            RenderOp::Draw(item) if item.element == id => Some(item.origin),
            _ => None,
        })
    };
    let after = tree.render_all();
    assert_eq!(origin_of(&before, a1), Some((1.0, 1.0)));
    assert_eq!(origin_of(&after, a1), Some((51.0, 41.0)));
}

#[test]
fn a_node_added_after_a_plan_is_part_of_the_next_one() {
    let (tree, [root, a, a1, a2, b]) = sample();
    assert_eq!(drawn(&tree), vec![root, a, a1, a2, b]);

    let a3 = tree.add_child(a, rect(3.0, 3.0)).unwrap();
    let late_root = tree.add_root(Rect::new(0.0, 0.0, 10.0, 10.0)).unwrap();

    assert_eq!(drawn(&tree), vec![root, a, a1, a2, a3, b, late_root]);
}

#[test]
fn reordering_siblings_through_a_structure_sync_changes_paint_order() {
    let (tree, [root, a, a1, a2, b]) = sample();
    assert_eq!(drawn(&tree), vec![root, a, a1, a2, b]);

    tree.sync_structure(&[
        RenderNodeSpec::new(Some(root), None, Rect::new(0.0, 0.0, 200.0, 200.0)),
        RenderNodeSpec::new(Some(b), Some(0), rect(30.0, 0.0)),
        RenderNodeSpec::new(Some(a), Some(0), rect(0.0, 0.0)),
        RenderNodeSpec::new(Some(a2), Some(2), rect(2.0, 2.0)),
        RenderNodeSpec::new(Some(a1), Some(2), rect(1.0, 1.0)),
    ])
    .unwrap();

    assert_eq!(drawn(&tree), vec![root, b, a, a2, a1]);
}

#[test]
fn reparenting_and_removing_nodes_through_a_structure_sync_is_planned() {
    let (tree, [root, a, a1, a2, b]) = sample();
    assert_eq!(drawn(&tree), vec![root, a, a1, a2, b]);

    // `a2` is dropped, `a1` moves under `b`, and a new node joins `a`.
    let ids = tree
        .sync_structure(&[
            RenderNodeSpec::new(Some(root), None, Rect::new(0.0, 0.0, 200.0, 200.0)),
            RenderNodeSpec::new(Some(a), Some(0), rect(0.0, 0.0)),
            RenderNodeSpec::new(None, Some(1), rect(5.0, 5.0)),
            RenderNodeSpec::new(Some(b), Some(0), rect(30.0, 0.0)),
            RenderNodeSpec::new(Some(a1), Some(3), rect(1.0, 1.0)),
        ])
        .unwrap();

    assert_eq!(drawn(&tree), vec![root, a, ids[2], b, a1]);
}

/// The plan derives each node's world state in one top-down pass; the
/// per-query definitions in `world.rs` derive it independently. They describe
/// the same geometry, so a plan item must land where the query says it does.
#[test]
fn planned_geometry_agrees_with_the_world_queries() {
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(3.0, 5.0, 400.0, 300.0)).unwrap();
    let plain = tree.add_child(root, Rect::new(7.0, 11.0, 90.0, 60.0)).unwrap();
    let scaled = tree.add_child(root, Rect::new(120.0, 40.0, 80.0, 50.0)).unwrap();
    let below_scaled = tree.add_child(scaled, Rect::new(4.0, 6.0, 30.0, 20.0)).unwrap();
    let presented = tree.add_child(plain, Rect::new(2.0, 3.0, 40.0, 40.0)).unwrap();
    let animated = tree.add_child(presented, Rect::new(1.0, 1.0, 20.0, 20.0)).unwrap();
    {
        let mut inner = tree.draw_cmd.borrow_mut();
        inner.node_mut(scaled).unwrap().transform = Mat3::scale(1.5, 0.75);
        inner.node_mut(presented).unwrap().presentation_transform = Mat3::translate(9.0, -4.0);
        inner.node_mut(animated).unwrap().animation_clip = Some(Rect::new(0.0, 0.0, 10.0, 10.0));
        inner.node_mut(plain).unwrap().clip = Some(Rect::new(0.0, 0.0, 60.0, 40.0));
    }

    let inner = tree.draw_cmd.borrow();
    for id in [root, plain, scaled, below_scaled, presented, animated] {
        let item = tree
            .render_all()
            .into_iter()
            .find_map(|op| match op {
                RenderOp::Draw(item) if item.element == id => Some(item),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{id:?} was not planned"));
        let expected = inner.world_bounds(id).unwrap();
        let tolerance = 1e-3;
        for (planned, queried, what) in [
            (item.bounds.x, expected.x, "x"),
            (item.bounds.y, expected.y, "y"),
            (item.bounds.width, expected.width, "width"),
            (item.bounds.height, expected.height, "height"),
        ] {
            assert!(
                (planned - queried).abs() <= tolerance,
                "{id:?} {what}: planned {planned}, queried {queried}"
            );
        }
    }
}
