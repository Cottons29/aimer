use super::*;

/// Marks a node without a resolvable parent in [`Topology::parent`].
const NO_PARENT: usize = usize::MAX;

/// Node positions for every parent and child link, resolved once per topology.
///
/// A link is stored as an element identity, and turning it into a position in
/// the node array costs a hash lookup. A scrolling frame plans every node of
/// the tree while the structure stays the same, so those lookups were repeated
/// several times per node per frame for answers that had not changed. This
/// resolves them when [`DrawCommandList::topology_revision`] moves and lets
/// the per-frame passes index plain slices.
#[derive(Default)]
struct Topology {
    revision: Option<u64>,
    /// Positions of the roots, in paint order.
    roots: Vec<usize>,
    /// Position of each node's parent, or [`NO_PARENT`].
    parent: Vec<usize>,
    /// `children[child_start[i]..child_start[i + 1]]` are node `i`'s children,
    /// in paint order. One extra entry closes the last node's range.
    child_start: Vec<usize>,
    children: Vec<usize>,
}

impl Topology {
    fn refresh(&mut self, tree: &DrawCommandList) {
        if self.revision == Some(tree.topology_revision) && self.parent.len() == tree.nodes.len() {
            return;
        }
        self.roots.clear();
        self.roots.extend(
            tree.roots
                .iter()
                .filter_map(|root| tree.indices.get(root).copied()),
        );
        self.parent.clear();
        self.child_start.clear();
        self.children.clear();
        for node in &tree.nodes {
            self.parent.push(
                node.parent
                    .and_then(|parent| tree.indices.get(&parent).copied())
                    .unwrap_or(NO_PARENT),
            );
            self.child_start.push(self.children.len());
            self.children.extend(
                node.children
                    .iter()
                    .filter_map(|child| tree.indices.get(child).copied()),
            );
        }
        self.child_start.push(self.children.len());
        self.revision = Some(tree.topology_revision);
    }

    #[inline]
    fn children_of(&self, index: usize) -> &[usize] {
        &self.children[self.child_start[index]..self.child_start[index + 1]]
    }
}

#[derive(Default)]
pub(super) struct RenderWorkspace {
    topology: Topology,
    states: Vec<NodeFrameState>,
    visit_stack: Vec<usize>,
    node_order: Vec<usize>,
    render_stack: Vec<RenderTraversal>,
}

#[derive(Clone, Copy)]
struct NodeFrameState {
    origin: (f32, f32),
    transform: Mat3,
    bounds: Option<Rect>,
    clip: ClipState,
    clip_radius: [f32; 4],
    subtree_bounds: Option<Rect>,
    visible_subtree_bounds: Option<Rect>,
}

impl Default for NodeFrameState {
    fn default() -> Self {
        Self {
            origin: (0.0, 0.0),
            transform: Mat3::identity(),
            bounds: None,
            clip: ClipState::Unclipped,
            clip_radius: [0.0; 4],
            subtree_bounds: None,
            visible_subtree_bounds: None,
        }
    }
}

#[derive(Clone, Copy)]
struct ParentFrameState {
    origin: (f32, f32),
    transform: Mat3,
    clip: ClipState,
    clip_radius: [f32; 4],
}

impl Default for ParentFrameState {
    fn default() -> Self {
        Self {
            origin: (0.0, 0.0),
            transform: Mat3::identity(),
            clip: ClipState::Unclipped,
            clip_radius: [0.0; 4],
        }
    }
}

/// Maps a clip's local corner radii into world space.
///
/// Radii scale with the clip's size change under the node transform; for the
/// translation and uniform scale transforms retained nodes use, the result is
/// exact, and otherwise it matches the axis-aligned approximation already used
/// for the clip rectangle.
#[inline]
fn world_clip_radius(radius: [f32; 4], local: Rect, world: Rect) -> [f32; 4] {
    if radius == [0.0; 4] || local.width <= 0.0 || local.height <= 0.0 {
        return radius;
    }
    let scale = (world.width / local.width + world.height / local.height) * 0.5;
    if scale == 1.0 {
        return radius;
    }
    radius.map(|corner| corner * scale)
}

enum RenderTraversal {
    Node(usize),
    EndOpacityGroup(RenderNodeId),
}

impl RenderWorkspace {
    fn prepare(&mut self, tree: &DrawCommandList) {
        self.topology.refresh(tree);
        self.states
            .resize_with(tree.nodes.len(), NodeFrameState::default);
        self.visit_stack.clear();
        self.node_order.clear();

        // Plain index loops: the adapter chains these replace cost several
        // calls per element in an unoptimized build, for a body that is a push.
        let roots = &self.topology.roots;
        let mut next = roots.len();
        while next > 0 {
            next -= 1;
            self.visit_stack.push(roots[next]);
        }

        while let Some(index) = self.visit_stack.pop() {
            let Some(node) = tree.nodes.get(index) else {
                continue;
            };
            let parent = self
                .states
                .get(self.topology.parent[index])
                .map_or_else(ParentFrameState::default, |state| ParentFrameState {
                    origin: state.origin,
                    transform: state.transform,
                    clip: state.clip,
                    clip_radius: state.clip_radius,
                });
            let origin = (
                parent.origin.0 + node.bounds.x,
                parent.origin.1 + node.bounds.y,
            );
            // A node without its own transforms leaves the parent's transform
            // untouched: translating to the origin and back is the identity,
            // so the four products that would rebuild it are skipped.
            let moves_itself =
                !node.presentation_transform.is_identity() || !node.transform.is_identity();
            let transform = if moves_itself {
                parent
                    .transform
                    .mul(&Mat3::translate(origin.0, origin.1))
                    .mul(&node.presentation_transform)
                    .mul(&node.transform)
                    .mul(&Mat3::translate(-origin.0, -origin.1))
            } else {
                parent.transform
            };
            let Some(bounds) = transform_rect(
                transform,
                Rect::new(origin.0, origin.1, node.bounds.width, node.bounds.height),
            ) else {
                continue;
            };

            let mut clip = parent.clip;
            let mut clip_radius = parent.clip_radius;
            if let Some(local_clip) = node.clip {
                let world_clip =
                    transform_rect(transform, local_clip.translated(origin.0, origin.1));
                clip = clip.intersect(world_clip);
                // The renderer takes radii from the innermost clip, so a clip
                // owner with square corners resets an inherited radius.
                if let Some(world_clip) = world_clip {
                    clip_radius = world_clip_radius(node.clip_radius, local_clip, world_clip);
                }
            }
            if let Some(local_clip) = node.animation_clip {
                // An animation clip is placed before the animation's own
                // transform, and only nodes that have one need this matrix.
                let transform_before_animation = if node.presentation_transform.is_identity() {
                    parent.transform
                } else {
                    parent
                        .transform
                        .mul(&Mat3::translate(origin.0, origin.1))
                        .mul(&node.presentation_transform)
                        .mul(&Mat3::translate(-origin.0, -origin.1))
                };
                clip = clip.intersect(transform_rect(
                    transform_before_animation,
                    local_clip.translated(origin.0, origin.1),
                ));
            }

            self.states[index] = NodeFrameState {
                origin,
                transform,
                bounds: Some(bounds),
                clip,
                clip_radius,
                subtree_bounds: Some(bounds),
                visible_subtree_bounds: clip.intersect_bounds(bounds),
            };
            self.node_order.push(index);

            let children = self.topology.children_of(index);
            let mut next = children.len();
            while next > 0 {
                next -= 1;
                self.visit_stack.push(children[next]);
            }
        }

        for index in self.node_order.iter().rev().copied() {
            let child_bounds = self.states[index].subtree_bounds;
            let child_visible_bounds = self.states[index].visible_subtree_bounds;
            let Some(parent_state) = self.states.get_mut(self.topology.parent[index]) else {
                continue;
            };
            union_bounds(&mut parent_state.subtree_bounds, child_bounds);
            union_bounds(
                &mut parent_state.visible_subtree_bounds,
                child_visible_bounds,
            );
        }
    }

    fn collect(
        &mut self,
        tree: &DrawCommandList,
        damage: Option<&[Rect]>,
    ) -> Vec<RenderOp> {
        let mut output = Vec::new();
        self.render_stack.clear();
        let roots = &self.topology.roots;
        let mut next = roots.len();
        while next > 0 {
            next -= 1;
            self.render_stack.push(RenderTraversal::Node(roots[next]));
        }

        while let Some(item) = self.render_stack.pop() {
            let index = match item {
                RenderTraversal::EndOpacityGroup(element) => {
                    output.push(RenderOp::EndOpacityGroup { element });
                    continue;
                }
                RenderTraversal::Node(index) => index,
            };
            let Some(node) = tree.nodes.get(index) else {
                continue;
            };
            let state = &self.states[index];
            let Some(bounds) = state.bounds else {
                continue;
            };
            if matches!(state.clip, ClipState::Empty) {
                continue;
            }
            let Some(visible_subtree) = state.visible_subtree_bounds else {
                continue;
            };
            if visible_subtree.width == 0.0 || visible_subtree.height == 0.0 {
                continue;
            }
            if damage.is_some_and(|damage| {
                !damage
                    .iter()
                    .any(|region| visible_subtree.intersection(*region).is_some())
            }) {
                continue;
            }

            let id = node.id;
            let opacity = node.presentation_opacity * node.opacity;
            let groups_opacity = opacity < 1.0 && !node.children.is_empty();
            if groups_opacity {
                output.push(RenderOp::BeginOpacityGroup {
                    element: id,
                    bounds: visible_subtree,
                    opacity,
                    clip: state.clip.as_option(),
                });
            }

            let visible_bounds = state.clip.intersect_bounds(bounds);
            if let Some(visible_bounds) = visible_bounds
                && damage.is_none_or(|damage| {
                    damage
                        .iter()
                        .any(|region| visible_bounds.intersection(*region).is_some())
                })
            {
                output.push(RenderOp::Draw(RenderItem {
                    element: id,
                    bounds,
                    origin: state.origin,
                    transform: state.transform,
                    clip: state.clip.as_option(),
                    clip_radius: state.clip_radius,
                    opacity: if groups_opacity { 1.0 } else { opacity },
                    paint_source: node.paint_source,
                    draw_list: node.draw_list.clone(),
                }));
            }

            if groups_opacity {
                self.render_stack
                    .push(RenderTraversal::EndOpacityGroup(id));
            }
            // A node without a clip may have children that extend beyond its bounds.
            let children = self.topology.children_of(index);
            let mut next = children.len();
            while next > 0 {
                next -= 1;
                self.render_stack
                    .push(RenderTraversal::Node(children[next]));
            }
        }

        output
    }
}

fn union_bounds(bounds: &mut Option<Rect>, other: Option<Rect>) {
    if let Some(other) = other {
        *bounds = Some(bounds.map_or(other, |bounds| bounds.union(other)));
    }
}

impl RenderTree {
    /// Returns visible elements in parent-first paint order for these damage regions.
    ///
    /// An empty damage slice means no pixels are dirty. The plan contains every
    /// intersecting node, including clean nodes behind a dirty transparent node,
    /// so the renderer can reconstruct the damaged pixels in the right order.
    pub fn render_order(&self, damage: &[Rect]) -> Vec<RenderOp> {
        if damage.is_empty() {
            return Vec::new();
        }
        let tree = self.draw_cmd.borrow();
        let mut workspace = tree.render_workspace.borrow_mut();
        workspace.prepare(&tree);
        workspace.collect(&tree, Some(damage))
    }

    /// Returns every render node in paint order, without damage culling.
    pub fn render_all(&self) -> Vec<RenderOp> {
        let tree = self.draw_cmd.borrow();
        let mut workspace = tree.render_workspace.borrow_mut();
        workspace.prepare(&tree);
        workspace.collect(&tree, None)
    }

    /// Takes pending damage and creates its parent-first composition plan.
    pub fn render_pending(&self) -> RenderFrame {
        let damage = self.take_damage();
        let operations = self.render_order(&damage);
        RenderFrame {
            damage,
            operations,
        }
    }
}
