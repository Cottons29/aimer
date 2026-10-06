//! World-space queries on the retained render tree.
//!
//! A node's world origin, transform and clip all depend on every ancestor.
//! Deriving each from scratch makes a single query cost depth² or worse, and
//! the damage bookkeeping asks these questions for nearly every node of a
//! scrolling tree on every frame. Instead each query walks to the root once
//! to build the parent's [`WorldFrame`], then derives the node's frame and
//! those of its descendants top-down, so the cost is the depth plus the
//! subtree actually covered.
//!
//! The arithmetic, including the matrix multiplication order, is identical to
//! the recursive definitions this replaces: the tests compare them
//! bit-for-bit.

use smallvec::SmallVec;

use super::*;

/// The world state a node hands to its children.
#[derive(Clone, Copy)]
pub(super) struct WorldFrame {
    /// World-space origin of the node's local coordinate space.
    origin: (f32, f32),
    /// Cumulative transform from untransformed world space to display space.
    transform: Mat3,
    /// The clip inherited from ancestors, including the node's own clips.
    clip: ClipState,
}

impl WorldFrame {
    /// The state above every root: nothing offset, transformed or clipped.
    #[inline]
    fn root() -> Self {
        Self {
            origin: (0.0, 0.0),
            transform: Mat3::identity(),
            clip: ClipState::Unclipped,
        }
    }
}

/// One node's world state together with its own world-space bounds.
struct NodeWorld {
    frame: WorldFrame,
    /// `None` when the node's transform produced non-finite bounds.
    bounds: Option<Rect>,
}

impl RenderNode {
    /// Derives this node's world state from its parent's.
    #[inline]
    fn world(&self, parent: &WorldFrame) -> NodeWorld {
        let (x, y) = (
            parent.origin.0 + self.bounds.x,
            parent.origin.1 + self.bounds.y,
        );
        let to_origin = Mat3::translate(x, y);
        let from_origin = Mat3::translate(-x, -y);
        let local = to_origin
            .mul(&self.presentation_transform)
            .mul(&self.transform)
            .mul(&from_origin);
        let transform = parent.transform.mul(&local);

        let mut own_clip = ClipState::Unclipped;
        if let Some(local_clip) = self.clip {
            own_clip = own_clip.intersect(transform_rect(transform, local_clip.translated(x, y)));
        }
        if let Some(local_clip) = self.animation_clip {
            // An animation clip is placed before the animation's own transform.
            let before_animation = parent.transform.mul(
                &to_origin
                    .mul(&self.presentation_transform)
                    .mul(&from_origin),
            );
            own_clip = own_clip.intersect(transform_rect(
                before_animation,
                local_clip.translated(x, y),
            ));
        }

        NodeWorld {
            frame: WorldFrame {
                origin: (x, y),
                transform,
                clip: parent.clip.intersect_state(own_clip),
            },
            bounds: transform_rect(
                transform,
                Rect::new(x, y, self.bounds.width, self.bounds.height),
            ),
        }
    }
}

impl DrawCommandList {
    /// The world state that `parent`'s children inherit, found with one walk
    /// to the root. `None` if an ancestor is missing.
    pub(super) fn frame_below(&self, parent: Option<RenderNodeId>) -> Option<WorldFrame> {
        let mut chain: SmallVec<[&RenderNode; 32]> = SmallVec::new();
        let mut next = parent;
        while let Some(id) = next {
            let node = self.node(id)?;
            chain.push(node);
            next = node.parent;
        }
        let mut frame = WorldFrame::root();
        for node in chain.iter().rev() {
            frame = node.world(&frame).frame;
        }
        Some(frame)
    }

    /// The node's own world state and bounds.
    fn node_world(&self, id: RenderNodeId) -> Option<NodeWorld> {
        let node = self.node(id)?;
        Some(node.world(&self.frame_below(node.parent)?))
    }

    pub(super) fn world_bounds(&self, id: RenderNodeId) -> Option<Rect> {
        self.node_world(id)?.bounds
    }

    /// The node's bounds after clipping by every ancestor and by itself.
    pub(super) fn visible_node_bounds(&self, id: RenderNodeId) -> Option<Rect> {
        let world = self.node_world(id)?;
        world.frame.clip.intersect_bounds(world.bounds?)
    }

    /// The union of the node's and its descendants' world bounds. `None` if
    /// any of them has non-finite bounds or is missing.
    pub(super) fn subtree_bounds(&self, id: RenderNodeId) -> Option<Rect> {
        let node = self.node(id)?;
        self.subtree_bounds_from(node, &self.frame_below(node.parent)?)
    }

    fn subtree_bounds_from(&self, node: &RenderNode, parent: &WorldFrame) -> Option<Rect> {
        let world = node.world(parent);
        let mut bounds = world.bounds?;
        for child in node.children.iter().copied() {
            let child = self.node(child)?;
            bounds = bounds.union(self.subtree_bounds_from(child, &world.frame)?);
        }
        Some(bounds)
    }

    /// The union of the visible, clipped bounds of the node and everything
    /// below it: the footprint that actually reaches pixels.
    pub(super) fn visible_subtree_bounds(&self, id: RenderNodeId) -> Option<Rect> {
        let node = self.node(id)?;
        self.visible_subtree_bounds_from(node, &self.frame_below(node.parent)?)
    }

    fn visible_subtree_bounds_from(&self, node: &RenderNode, parent: &WorldFrame) -> Option<Rect> {
        let world = node.world(parent);
        let mut bounds = world.frame.clip.intersect_bounds(world.bounds?);
        for child in node.children.iter().copied() {
            let Some(child) = self.node(child) else {
                continue;
            };
            if let Some(child_bounds) = self.visible_subtree_bounds_from(child, &world.frame) {
                bounds = Some(bounds.map_or(child_bounds, |bounds| bounds.union(child_bounds)));
            }
        }
        bounds
    }
}
