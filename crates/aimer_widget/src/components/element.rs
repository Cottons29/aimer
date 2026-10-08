use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_cupid::damage_region::{DamageRect, DamageSet};
use aimer_cupid::draw_cmd_v2::{RenderNodeId, RenderPaintSource, RenderTree};
use aimer_cupid::utilities::{IdBuildHasher, Rect};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
#[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
use aimer_events::window::request_animation_frame;
use aimer_focus::{FocusCandidate, FocusCandidates, FocusManager, FocusNode, FocusTrapId};
use aimer_rubick::{ErasedFrom, UiAllocator};
use hashbrown::{HashMap, HashSet};
use smallvec::SmallVec;

use crate::base::*;
use crate::components::event_element::{
    CaptureRequest, EventElement, EventResult, EventTreeRole, FollowUp, PointerKey,
};
use crate::components::layout_element::LayoutElement;
use crate::components::rebuildable::Rebuildable;
pub(crate) use crate::components::visitor_element::VisitorElement;
use crate::element_invalidation::{
    ElementChangeKind, ElementInvalidationBatch, ElementInvalidationBounds,
    ElementInvalidationRevisions,
};
use crate::components::drawable::{
    CompositorAnimationDecision, CompositorAnimationFrame,
};
#[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
use crate::components::drawable::CompositorTransform;
use crate::pointer_claim;
use crate::{AnyElement, Drawable, Key};

mod event;
mod scoped_rebuild;
#[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
mod animation_only;
#[cfg(any(target_arch = "wasm32", feature = "portable-guest"))]
#[path = "animation_only_stub.rs"]
mod animation_only;
mod compositor_applied;
mod node;
#[cfg(test)]
mod keep_reachable_tests;
#[cfg(test)]
mod paint_invalidation_tests;
#[cfg(test)]
mod render_context_tests;
mod retained_window;

pub use animation_only::{
    animation_only_targets, is_animation_only_frame, request_isolated_animation_frame,
    request_isolated_subtree_animation_frame, run_animation_only_pass, set_frame_rebuilt_roots, set_animation_only_frame, traversal_counts,
    with_v2_animation_only_context,
};
use animation_only::{
    begin_full_traversal, capture_rebuilt_root, finish_full_traversal, is_animation_only_pass, pass_skips_subtree,
    register_animating_element,
};
use compositor_applied::{
    compositor_animation_may_be_applied, forget_unmapped_compositor_animations,
    note_compositor_animation_applied, note_compositor_animation_cleared,
};
pub use scoped_rebuild::{
    ScopedCursor, has_scoped_rebuilds_since, scoped_rebuild_cursor, scoped_rebuilds_since,
    unscoped_element_tree_generation,
};
use scoped_rebuild::{note_scoped_rebuild, note_unscoped_tree_change};
use node::ElementNode;
pub use retained_window::{
    VisibleWindow, may_skip_settled_offscreen, retained_element_settled, retained_visible_window,
    retained_visible_window_of_current, set_scroll_only_frame, set_settled_offscreen_skip,
};
pub use event::{
    ElementPath, EventDispatchContext, EventDispatcher, broadcast_event, dispatch_event,
    dispatch_focused_event,
};

static NEXT_ELEMENT_ID: AtomicU64 = AtomicU64::new(1);
static ELEMENT_TREE_GENERATION: AtomicU64 = AtomicU64::new(0);
static RETAINED_RENDER_STRUCTURE_GENERATION: AtomicU64 = AtomicU64::new(0);
static REBUILD_INVALIDATION_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAYOUT_INVALIDATION_GENERATION: AtomicU64 = AtomicU64::new(0);
/// Advances of the rebuild generation that only kept an element reachable.
static REBUILD_KEEPALIVE_COUNT: AtomicU64 = AtomicU64::new(0);
/// Advances of the rebuild generation made by `set_state` marking an element
/// dirty.
static REBUILD_STATE_MARK_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static TEST_ELEMENT_TREE_GENERATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    /// Active per-window v2 recording context. It is installed only while the
    /// UI thread traverses the retained element tree for one frame.
    static V2_RENDER_CONTEXT_STACK: RefCell<Vec<V2RenderContext>> = const { RefCell::new(Vec::new()) };

    /// Elements that ran `ElementNode::draw` under retained presentation without
    /// a render node. A frame loop drains this after its final structure sync:
    /// an element still unmapped then painted into nothing, because the render
    /// tree never learned of it (typically its parent draws it without
    /// exposing it through `visit_children`).
    static UNMAPPED_DRAWS: RefCell<Vec<(ElementId, &'static str)>> = const { RefCell::new(Vec::new()) };
    #[cfg(debug_assertions)]
    static DECLINED_PAINT_ELEMENTS: RefCell<Vec<(ElementId, &'static str)>> = const { RefCell::new(Vec::new()) };

    /// Nested `mark_needs_rebuild` calls share one invalidation generation.
    /// Marking a large tree is already recursive; it must not also perform one
    /// atomic increment per descendant.
    static REBUILD_INVALIDATION_DEPTH: Cell<usize> = const { Cell::new(0) };

    /// Revisions are kept outside [`ElementNode`] so adding stable-layout
    /// tracking does not change the inline-erased element size. Only elements
    /// that opt into generation-independent sizing create entries here.
    static STABLE_SUBTREE_GENERATIONS: RefCell<HashMap<ElementId, u64, IdBuildHasher>> =
        RefCell::new(HashMap::default());

    /// Which elements had their stable generation set, in order, so a container
    /// can learn which of its children changed without asking each of them.
    static STABLE_GENERATION_LOG: RefCell<StableGenerationLog> =
        const { RefCell::new(StableGenerationLog { first: 0, ids: Vec::new() }) };

    /// Number of dirty rebuild sources whose root-relative path crosses each
    /// retained element. Counts keep shared ancestors indexed until every
    /// dirty source below them has been rebuilt.
    static DIRTY_SUBTREE_COUNTS: RefCell<HashMap<ElementId, usize, IdBuildHasher>> =
        RefCell::new(HashMap::default());

    /// For each element, the children that dirty paths run through, so a
    /// container can reach them without entering its other children.
    static DIRTY_EDGES: RefCell<HashMap<ElementId, SmallVec<[DirtyEdge; 2]>, IdBuildHasher>> =
        RefCell::new(HashMap::default());

    /// Advances whenever a dirty path is added, so a pass that rebuilt a
    /// snapshot of the edges can tell that the index grew underneath it.
    static DIRTY_EDGE_VERSION: Cell<u64> = const { Cell::new(0) };

    /// Event dispatchers use this UI-thread epoch to coalesce generation checks
    /// across all nested dispatchers until the next completed frame.
    static EVENT_FRAME_EPOCH: Cell<Option<u64>> = const { Cell::new(None) };

    /// The path index is usable only after one complete walk of the retained
    /// tree without a structural replacement or an untracked invalidation.
    static DIRTY_PATHS_READY: Cell<bool> = const { Cell::new(false) };
    static DIRTY_INDEXED_ROOTS: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    static DIRTY_PATHS_INVALIDATED_DURING_TRAVERSAL: Cell<bool> = const { Cell::new(false) };
    static REBUILD_TRAVERSAL_DEPTH: Cell<usize> = const { Cell::new(0) };
    static REBUILD_DESCENTS: Cell<u64> = const { Cell::new(0) };
    static REBUILD_VISITS: Cell<u64> = const { Cell::new(0) };
    /// Non-zero while a caller that rebases the paths of the subtree it
    /// replaces is committing that replacement.
    static PRESERVE_DIRTY_PATHS_DEPTH: Cell<usize> = const { Cell::new(0) };
    static REBUILD_PATH: RefCell<Vec<ElementId>> = const { RefCell::new(Vec::new()) };
    static REBUILD_FORCE_DESCEND_DEPTH: Cell<usize> = const { Cell::new(0) };
    static REBUILD_MARK_DEPTH: Cell<usize> = const { Cell::new(0) };
    static DRAW_DEPTH: Cell<usize> = const { Cell::new(0) };
    static DRAW_INVALIDATION_OWNER: Cell<Option<ElementId>> = const { Cell::new(None) };
    /// Paint invalidation metadata is retained for the current pair of
    /// rebuild/tree generations so a retained scroll tile can distinguish a
    /// local dirty element from an unrelated branch's rebuild.
    static PAINT_INVALIDATION_EPOCH: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
    static PAINT_INVALIDATED_LOCAL_ELEMENTS: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    /// Visual event changes invalidate one element's retained list for the
    /// next collection, then are consumed without dirtying later frames.
    static EVENT_PAINT_INVALIDATIONS: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    /// Damage accumulated by retained paint owners during the current frame.
    /// The target is thread-local because widget trees and their canvases are
    /// single-threaded; the finished [`DamageSet`] is copied into the frame
    /// packet before presentation moves to another thread.
    static FRAME_PAINT_DAMAGE: RefCell<Option<DamageSet>> = const { RefCell::new(None) };
    /// Stateful paint owners that contributed a precise footprint this frame.
    /// An owner rectangle can cover an otherwise unbounded event-tree
    /// invalidation somewhere inside its bounded paint subtree.
    static FRAME_PAINT_DAMAGE_OWNERS: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    /// A platform event can invalidate paint before the next frame opens its
    /// damage collector. Keep that request until [`begin_paint_frame`] consumes
    /// it instead of silently dropping the invalidation.
    static PENDING_PAINT_DAMAGE_FULL: Cell<bool> = const { Cell::new(false) };
    static FRAME_PAINT_TARGET: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
    #[cfg(feature = "frame-stats")]
    static DRAW_TRAVERSAL_COUNT: Cell<u64> = const { Cell::new(0) };
    #[cfg(feature = "frame-stats")]
    static ROUTED_EVENT_VISIT_COUNT: Cell<u64> = const { Cell::new(0) };
    #[cfg(test)]
    static HOVER_MEMBERSHIP_CHECK_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// Maps each element to its render node in the window's retained render tree.
///
/// It is consulted for every element on every draw, so it uses `hashbrown`'s
/// default hasher: the keys are process-local identifiers, and the standard
/// library's DoS-resistant SipHash costs far more than the lookup itself,
/// especially in unoptimized builds.
pub type ElementNodeMap = HashMap<ElementId, RenderNodeId>;

/// The retained render tree a traversal paints into, and how elements map to
/// its nodes.
///
/// Every element's `update` takes a copy of this, so a copy is one counted
/// reference to shared data, not one per field.
#[derive(Clone)]
pub(crate) struct V2RenderContext(Rc<V2RenderScope>);

pub(crate) struct V2RenderScope {
    pub(crate) tree: RenderTree,
    element_nodes: Rc<ElementNodeMap>,
    prepared_root: Cell<Option<ElementId>>,
}

impl std::ops::Deref for V2RenderContext {
    type Target = V2RenderScope;

    #[inline]
    fn deref(&self) -> &V2RenderScope {
        &self.0
    }
}

impl V2RenderScope {
    #[inline]
    pub(crate) fn node_for_element(&self, element: ElementId) -> Option<RenderNodeId> {
        self.element_nodes.get(&element).copied()
    }

    #[inline]
    pub(crate) fn take_prepared_root(&self, element: ElementId) -> bool {
        self.prepared_root.get() == Some(element) && self.prepared_root.take().is_some()
    }
}

struct V2RenderContextGuard;

impl Drop for V2RenderContextGuard {
    fn drop(&mut self) {
        V2_RENDER_CONTEXT_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// Runs the element traversal with the retained tree as the paint source.
///
/// Elements that have a render node record their paint into it, and the
/// traversal drops whatever they also draw through the canvas while it still
/// visits children and updates state.
#[doc(hidden)]
pub fn with_v2_render_tree_context<R>(
    tree: RenderTree,
    element_nodes: Rc<ElementNodeMap>,
    prepared_root: Option<ElementId>,
    callback: impl FnOnce() -> R,
) -> R {
    forget_unmapped_compositor_animations(&element_nodes);
    begin_full_traversal();
    V2_RENDER_CONTEXT_STACK.with(|stack| {
        stack.borrow_mut().push(V2RenderContext(Rc::new(V2RenderScope {
            tree,
            element_nodes,
            prepared_root: Cell::new(prepared_root),
        })));
    });
    let _guard = V2RenderContextGuard;
    let result = callback();
    finish_full_traversal();
    result
}

pub(crate) fn current_v2_render_context() -> Option<V2RenderContext> {
    V2_RENDER_CONTEXT_STACK.with(|stack| stack.borrow().last().cloned())
}

/// Returns whether the current draw is using the window's retained v2 tree.
#[doc(hidden)]
pub fn has_active_v2_render_tree() -> bool {
    // Asked once per element per frame: looking is enough, copying is not.
    V2_RENDER_CONTEXT_STACK.with(|stack| !stack.borrow().is_empty())
}

/// Returns whether retained v2 paint is the current presentation source, which
/// it is whenever a render tree is active.
#[doc(hidden)]
pub fn has_active_v2_render_presentation() -> bool {
    has_active_v2_render_tree()
}

/// Updates one mapped element's local bounds and clip during retained drawing.
///
/// Viewports use this for scroll offsets that change independently of layout.
/// The tree damages the old and new clipped footprints as one geometry change.
#[doc(hidden)]
pub fn update_v2_render_node_geometry(
    element: ElementId,
    bounds: Rect,
    clip: Option<Rect>,
) -> bool {
    let Some(context) = current_v2_render_context() else {
        return false;
    };
    let Some(node) = context.node_for_element(element) else {
        return false;
    };
    context.tree.set_geometry(node, bounds, clip).is_ok()
}

/// Starts a new event-dispatch frame on the current UI thread.
///
/// [`EventDispatcher`] instances use the shared epoch to check their root's
/// subtree generation once and defer a path-index rebuild until the first
/// dispatch that needs it. The application frame loop should call this after a
/// frame finishes so platform and nested widget events share the next epoch.
pub fn begin_event_frame() {
    EVENT_FRAME_EPOCH.with(|epoch| {
        let next = epoch.get().unwrap_or(0).wrapping_add(1);
        epoch.set(Some(next));
    });
}

/// Starts collecting damage for one device-sized frame target.
///
/// The first frame for a target is conservatively full. Subsequent frames can
/// remain empty when every retained paint owner replays unchanged content;
/// invalidation requested between frames is carried into the next collection
/// interval.
#[doc(hidden)]
pub fn begin_paint_frame(width: u32, height: u32) {
    FRAME_PAINT_DAMAGE_OWNERS.with(|owners| owners.borrow_mut().clear());
    let target_changed = FRAME_PAINT_TARGET.with(|target| {
        let changed = target.get() != Some((width, height));
        target.set(Some((width, height)));
        changed
    });
    let pending_full = PENDING_PAINT_DAMAGE_FULL.with(|pending| pending.replace(false));
    FRAME_PAINT_DAMAGE.with(|damage| {
        let mut next = DamageSet::new(width, height);
        if target_changed || pending_full {
            next.mark_full();
        }
        *damage.borrow_mut() = Some(next);
    });
}

/// Takes the damage collected since [`begin_paint_frame`].
#[doc(hidden)]
pub fn take_paint_frame_damage(width: u32, height: u32) -> DamageSet {
    let damage = FRAME_PAINT_DAMAGE
        .with(|damage| damage.borrow_mut().take())
        .unwrap_or_else(|| DamageSet::full(width, height));
    EVENT_PAINT_INVALIDATIONS.with(|invalidations| invalidations.borrow_mut().clear());
    damage
}

/// Adds one conservative device-pixel footprint to the current frame.
#[doc(hidden)]
pub fn mark_paint_damage(rectangle: DamageRect) {
    if let Some(owner) = DRAW_INVALIDATION_OWNER.with(Cell::get) {
        FRAME_PAINT_DAMAGE_OWNERS.with(|owners| {
            owners.borrow_mut().insert(owner);
        });
        queue_element_invalidation_for(
            Some(owner),
            None,
            ElementChangeKind::Paint,
            Some(rectangle),
        );
    }
    FRAME_PAINT_DAMAGE.with(|damage| {
        if let Some(damage) = damage.borrow_mut().as_mut() {
            damage.add(rectangle);
        }
    });
}

/// Forces the current frame to repaint its complete target.
///
/// When called between frame collections, the request is retained for the next
/// call to [`begin_paint_frame`].
#[doc(hidden)]
pub fn mark_paint_damage_full() {
    if let Some(owner) = DRAW_INVALIDATION_OWNER.with(Cell::get) {
        queue_element_invalidation_for(
            Some(owner),
            None,
            ElementChangeKind::Unknown,
            None,
        );
    } else if FRAME_PAINT_DAMAGE.with(|damage| damage.borrow().is_none()) {
        // External readiness and platform changes can request a full repaint
        // without an element owner. Preserve that fact for the next frame's
        // invalidation batch as an explicit unknown record.
        queue_element_invalidation(None, ElementChangeKind::Unknown);
    }
    FRAME_PAINT_DAMAGE.with(|damage| {
        if let Some(damage) = damage.borrow_mut().as_mut() {
            damage.mark_full();
        } else {
            PENDING_PAINT_DAMAGE_FULL.with(|pending| pending.set(true));
        }
    });
}

fn current_event_frame() -> Option<u64> {
    EVENT_FRAME_EPOCH.with(Cell::get)
}

fn sync_paint_invalidation_epoch() {
    let epoch = (
        REBUILD_INVALIDATION_GENERATION.load(Ordering::Acquire),
        ELEMENT_TREE_GENERATION.load(Ordering::Acquire),
    );
    let changed = PAINT_INVALIDATION_EPOCH.with(|current| {
        let previous = current.get();
        if previous.map(|previous| previous.0) == Some(epoch.0) {
            current.set(Some(epoch));
            false
        } else {
            current.set(Some(epoch));
            true
        }
    });
    if changed {
        PAINT_INVALIDATED_LOCAL_ELEMENTS.with(|elements| elements.borrow_mut().clear());
    }
}

fn record_local_paint_invalidation_path(path: &[ElementId]) {
    sync_paint_invalidation_epoch();
    if let Some(element) = path.last().copied() {
        PAINT_INVALIDATED_LOCAL_ELEMENTS.with(|elements| {
            elements.borrow_mut().insert(element);
        });
    }
}

fn queue_element_invalidation(
    path: Option<Rc<[ElementId]>>,
    change: ElementChangeKind,
) {
    let element_id = path.as_deref().and_then(|path| path.last().copied());
    queue_element_invalidation_for(element_id, path, change, None);
}

fn queue_element_invalidation_for(
    element_id: Option<ElementId>,
    path: Option<Rc<[ElementId]>>,
    change: ElementChangeKind,
    damage_hint: Option<DamageRect>,
) {
    let revisions = current_element_invalidation_revisions();
    let queued_during_frame = FRAME_PAINT_DAMAGE.with(|damage| damage.borrow().is_some());
    crate::element_invalidation::enqueue(
        element_id,
        path,
        change,
        revisions,
        queued_during_frame,
        damage_hint,
    );
}

fn record_current_paint_invalidation(element: ElementId, owns_paint: bool) {
    // Only an element that owns its paint has a local list to refresh; a
    // descendant's rebuild reaches its own paint owner through the dirty path.
    if !owns_paint {
        return;
    }
    REBUILD_PATH.with(|path| {
        let path = path.borrow();
        if path.is_empty() {
            record_local_paint_invalidation_path(&[element]);
        } else {
            record_local_paint_invalidation_path(&path);
        }
    });
}

fn current_element_invalidation_revisions() -> ElementInvalidationRevisions {
    ElementInvalidationRevisions {
        tree: element_tree_generation(),
        rebuild: rebuild_invalidation_generation(),
        layout: layout_invalidation_generation(),
        resource: None,
    }
}

fn element_invalidation_bounds(element: &dyn Element) -> Option<ElementInvalidationBounds> {
    let (start, end) = element.event_tree_bounds()?;
    if !start.x.is_finite()
        || !start.y.is_finite()
        || !end.x.is_finite()
        || !end.y.is_finite()
        || end.x < start.x
        || end.y < start.y
    {
        return None;
    }
    Some(ElementInvalidationBounds { start, end })
}

fn mark_paint_invalidations_unknown() {
    sync_paint_invalidation_epoch();
    queue_element_invalidation(None, ElementChangeKind::Unknown);
}

/// Returns whether this element's own local retained paint was invalidated.
#[doc(hidden)]
pub fn local_paint_element_was_invalidated(element: ElementId) -> bool {
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATED_LOCAL_ELEMENTS.with(|elements| {
        let elements = elements.borrow();
        !elements.is_empty() && elements.contains(&element)
    })
}

/// Marks one reconciled element's retained paint as stale in this invalidation epoch.
#[doc(hidden)]
pub(crate) fn mark_paint_element_invalidated(element: ElementId) {
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATED_LOCAL_ELEMENTS.with(|elements| {
        elements.borrow_mut().insert(element);
    });
}

#[inline]
pub(crate) fn mark_event_paint_invalidated(element: ElementId) {
    EVENT_PAINT_INVALIDATIONS.with(|invalidations| {
        invalidations.borrow_mut().insert(element);
    });
}

#[inline]
pub(crate) fn take_event_paint_invalidated(element: ElementId) -> bool {
    EVENT_PAINT_INVALIDATIONS.with(|invalidations| {
        let mut invalidations = invalidations.borrow_mut();
        // Asked of every element every frame, and almost always empty: hashing
        // the element only to find that out is the whole cost.
        !invalidations.is_empty() && invalidations.remove(&element)
    })
}

/// Resets the draw traversal counter for the next measured frame.
#[cfg(feature = "frame-stats")]
pub fn reset_draw_traversal_count() {
    DRAW_TRAVERSAL_COUNT.with(|count| count.set(0));
}

/// Takes the number of retained element draw calls observed since the last
/// reset. A draw call is counted for every element reached by the drawable
/// traversal, including a scroll container whose children may be culled.
#[cfg(feature = "frame-stats")]
pub fn take_draw_traversal_count() -> u64 {
    DRAW_TRAVERSAL_COUNT.with(Cell::get)
}

/// Resets the routed-event visit counter for the next measured input sample.
#[cfg(feature = "frame-stats")]
#[doc(hidden)]
pub fn reset_routed_event_visit_count() {
    ROUTED_EVENT_VISIT_COUNT.with(|count| count.set(0));
}

/// Takes the number of elements reached by routed pointer dispatch since the
/// last reset. Cached hit-chain replay contributes one visit per delivered
/// element, so this counter measures the work visible to the event path rather
/// than only calls into the uncached recursive walker.
#[cfg(feature = "frame-stats")]
#[doc(hidden)]
pub fn take_routed_event_visit_count() -> u64 {
    ROUTED_EVENT_VISIT_COUNT.with(Cell::get)
}

#[inline]
fn record_routed_event_visit() {
    crate::frame_work_stats::record_hit_test_visit();
    #[cfg(feature = "frame-stats")]
    ROUTED_EVENT_VISIT_COUNT.with(|count| count.set(count.get().saturating_add(1)));
}

#[inline]
fn record_hover_membership_check() {
    #[cfg(test)]
    HOVER_MEMBERSHIP_CHECK_COUNT.with(|count| count.set(count.get().saturating_add(1)));
}

#[cfg(test)]
fn reset_hover_membership_check_count() {
    HOVER_MEMBERSHIP_CHECK_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
fn take_hover_membership_check_count() -> u64 {
    HOVER_MEMBERSHIP_CHECK_COUNT.with(Cell::get)
}

#[cfg(feature = "frame-stats")]
#[inline]
fn record_draw_traversal() {
    DRAW_TRAVERSAL_COUNT.with(|count| count.set(count.get().saturating_add(1)));
}

struct RebuildInvalidationGuard;

impl Drop for RebuildInvalidationGuard {
    fn drop(&mut self) {
        REBUILD_INVALIDATION_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub(crate) struct RebuildMarkGuard;

impl Drop for RebuildMarkGuard {
    fn drop(&mut self) {
        REBUILD_MARK_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub(crate) fn begin_rebuild_mark() -> (RebuildMarkGuard, bool) {
    let outermost = REBUILD_MARK_DEPTH.with(|depth| {
        let outermost = depth.get() == 0;
        depth.set(depth.get() + 1);
        outermost
    });
    (RebuildMarkGuard, outermost)
}

struct DrawGuard;

impl Drop for DrawGuard {
    fn drop(&mut self) {
        DRAW_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

struct DrawInvalidationOwnerGuard {
    previous: Option<ElementId>,
}

impl DrawInvalidationOwnerGuard {
    #[inline]
    fn enter(owner: ElementId) -> Self {
        Self {
            previous: DRAW_INVALIDATION_OWNER.with(|current| current.replace(Some(owner))),
        }
    }
}

impl Drop for DrawInvalidationOwnerGuard {
    #[inline]
    fn drop(&mut self) {
        DRAW_INVALIDATION_OWNER.with(|current| current.set(self.previous));
    }
}

fn begin_draw() -> (DrawGuard, bool) {
    let outermost = DRAW_DEPTH.with(|depth| {
        let outermost = depth.get() == 0;
        depth.set(depth.get() + 1);
        outermost
    });
    (DrawGuard, outermost)
}

/// Identifies one logical element for as long as it remains in the element tree.
///
/// IDs are assigned monotonically and are never reused. Reconciliation transfers
/// an ID to a compatible newly generated element, while a genuine replacement
/// keeps its newly assigned ID.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ElementId(NonZeroU64);

impl ElementId {
    fn next() -> Self {
        let value = NEXT_ELEMENT_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("exhausted all element identities");
        Self(NonZeroU64::new(value).expect("element identity counter starts at one"))
    }

    /// Returns the non-zero integer representation of this identity.
    #[inline]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Tracks one built-in self-rebuilding element's dirty flag and current path.
/// The source is shared with the build consumer or state updater that can mark
/// the element outside the retained-tree walk.
pub(crate) struct DirtySource {
    dirty: Rc<Cell<bool>>,
    path: RefCell<Option<Rc<[ElementId]>>>,
    indexed: Cell<bool>,
}

impl DirtySource {
    pub(crate) fn new(dirty: Rc<Cell<bool>>) -> Rc<Self> {
        Rc::new(Self {
            dirty,
            path: RefCell::new(None),
            indexed: Cell::new(false),
        })
    }

    /// Marks the source dirty and returns whether it was clean before this
    /// call.
    #[inline]
    pub(crate) fn mark(&self) -> bool {
        if self.dirty.get() {
            if let Some(path) = self.path.borrow().clone() {
                queue_element_invalidation(Some(path), ElementChangeKind::Unknown);
            } else {
                mark_paint_invalidations_unknown();
            }
            if !self.indexed.get() {
                invalidate_dirty_paths();
            }
            return false;
        }
        let mut first = false;
        with_rebuild_invalidation(|| {
            first = !self.dirty.replace(true);
            if first {
                let path = self.path.borrow().clone();
                if let Some(path) = path {
                    queue_element_invalidation(Some(path.clone()), ElementChangeKind::Unknown);
                    record_local_paint_invalidation_path(&path);
                    add_dirty_path(&path);
                    self.indexed.set(true);
                } else {
                    mark_paint_invalidations_unknown();
                    invalidate_dirty_paths();
                }
            }
        });
        first
    }

    #[inline]
    pub(crate) fn clear(&self) {
        self.dirty.set(false);
        let path = self.path.borrow().clone();
        if self.indexed.replace(false)
            && let Some(path) = path.as_deref()
        {
            remove_dirty_path(path);
        }
    }

    /// Associates the source with the current root-relative path. Stable paths
    /// are left in place so clean frames do not allocate.
    pub(crate) fn set_path(&self, path: &[ElementId]) {
        let old_path = self.path.borrow().clone();
        if old_path.as_deref() == Some(path) {
            return;
        }

        let old_root = old_path.as_deref().and_then(|path| path.first()).copied();
        let new_root = path.first().copied();
        if old_root.is_some() && old_root != new_root {
            // A source path is relative to the traversal root. If a subtree
            // was walked independently, its metadata cannot safely answer a
            // later walk from the retained application root.
            invalidate_dirty_paths();
        }

        if self.indexed.replace(false)
            && let Some(old_path) = old_path.as_deref()
        {
            remove_dirty_path(old_path);
        }

        let new_path: Rc<[ElementId]> = Rc::from(path.to_vec());
        *self.path.borrow_mut() = Some(new_path.clone());

        if self.dirty.get() {
            record_local_paint_invalidation_path(&new_path);
            add_dirty_path(&new_path);
            self.indexed.set(true);
        }
    }

    pub(crate) fn set_path_if_missing(&self, path: &[ElementId]) {
        if self.path.borrow().is_none() {
            self.set_path(path);
        }
    }
}

/// Associates a newly materialized rebuilding element with its structural path.
///
/// Dynamic child sources can use this when a stateful child appears after the
/// ordinary rebuild walk has already indexed its parent. Existing source paths
/// are left intact.
#[doc(hidden)]
pub fn set_rebuild_source_path(element: &dyn Element, path: &[ElementId]) {
    let Some(value) = element.option_any() else {
        return;
    };
    if let Some(stateless) = value.downcast_ref::<crate::widget::stateless::StatelessElement>() {
        stateless.dirty_source.set_path_if_missing(path);
    } else if let Some(stateful) = value.downcast_ref::<crate::widget::stateful::StatefulElement>() {
        stateful.dirty_source.borrow().set_path_if_missing(path);
    }
}

impl Drop for DirtySource {
    fn drop(&mut self) {
        if self.indexed.get()
            && let Some(path) = self.path.get_mut().as_deref()
        {
            remove_dirty_path(path);
        }
    }
}

/// One child a dirty path runs through, and how many paths do.
struct DirtyEdge {
    child: ElementId,
    paths: usize,
    /// Where the parent last found the child, so the next lookup is one probe.
    hint: Cell<usize>,
}

fn add_dirty_path(path: &[ElementId]) {
    DIRTY_SUBTREE_COUNTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        for id in path {
            *counts.entry(*id).or_default() += 1;
        }
    });
    DIRTY_EDGES.with(|edges| {
        let mut edges = edges.borrow_mut();
        for pair in path.windows(2) {
            let siblings = edges.entry(pair[0]).or_default();
            match siblings.iter_mut().find(|edge| edge.child == pair[1]) {
                Some(edge) => edge.paths += 1,
                None => siblings.push(DirtyEdge { child: pair[1], paths: 1, hint: Cell::new(0) }),
            }
        }
    });
    DIRTY_EDGE_VERSION.with(|version| version.set(version.get() + 1));
}

fn remove_dirty_path(path: &[ElementId]) {
    DIRTY_SUBTREE_COUNTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        for id in path {
            let Some(count) = counts.get_mut(id) else {
                continue;
            };
            *count -= 1;
            if *count == 0 {
                counts.remove(id);
            }
        }
    });
    DIRTY_EDGES.with(|edges| {
        let mut edges = edges.borrow_mut();
        for pair in path.windows(2) {
            let Some(siblings) = edges.get_mut(&pair[0]) else {
                continue;
            };
            if let Some(position) = siblings.iter().position(|edge| edge.child == pair[1]) {
                siblings[position].paths -= 1;
                if siblings[position].paths == 0 {
                    siblings.swap_remove(position);
                }
            }
            if siblings.is_empty() {
                edges.remove(&pair[0]);
            }
        }
    });
}

fn dirty_path_contains(id: ElementId) -> bool {
    DIRTY_SUBTREE_COUNTS.with(|counts| counts.borrow().contains_key(&id))
}

pub(crate) fn invalidate_dirty_paths() {
    DIRTY_PATHS_READY.with(|ready| ready.set(false));
    DIRTY_INDEXED_ROOTS.with(|roots| roots.borrow_mut().clear());
    if REBUILD_TRAVERSAL_DEPTH.with(|depth| depth.get() > 0) {
        DIRTY_PATHS_INVALIDATED_DURING_TRAVERSAL.with(|invalidated| invalidated.set(true));
    }
}

struct RebuildTraversalGuard {
    outermost: bool,
    complete: bool,
    root: ElementId,
}

impl Drop for RebuildTraversalGuard {
    fn drop(&mut self) {
        REBUILD_TRAVERSAL_DEPTH.with(|depth| depth.set(depth.get() - 1));
        if self.outermost
            && self.complete
            && !DIRTY_PATHS_INVALIDATED_DURING_TRAVERSAL.with(Cell::get)
        {
            DIRTY_INDEXED_ROOTS.with(|roots| {
                roots.borrow_mut().insert(self.root);
            });
            DIRTY_PATHS_READY.with(|ready| ready.set(true));
        }
    }
}

fn begin_rebuild_traversal(root: ElementId) -> RebuildTraversalGuard {
    let outermost = REBUILD_TRAVERSAL_DEPTH.with(|depth| {
        let outermost = depth.get() == 0;
        if outermost {
            DIRTY_PATHS_INVALIDATED_DURING_TRAVERSAL.with(|invalidated| invalidated.set(false));
        }
        depth.set(depth.get() + 1);
        outermost
    });
    RebuildTraversalGuard {
        outermost,
        complete: false,
        root,
    }
}

struct RebuildPathGuard;

impl RebuildPathGuard {
    fn push(id: ElementId) -> Self {
        REBUILD_PATH.with(|path| path.borrow_mut().push(id));
        Self
    }
}

impl Drop for RebuildPathGuard {
    fn drop(&mut self) {
        REBUILD_PATH.with(|path| {
            path.borrow_mut().pop();
        });
    }
}

struct RebuildDescendGuard {
    active: bool,
}

impl RebuildDescendGuard {
    fn enter(active: bool) -> Self {
        if active {
            REBUILD_FORCE_DESCEND_DEPTH.with(|depth| depth.set(depth.get() + 1));
        }
        Self { active }
    }
}

impl Drop for RebuildDescendGuard {
    fn drop(&mut self) {
        if self.active {
            REBUILD_FORCE_DESCEND_DEPTH.with(|depth| depth.set(depth.get() - 1));
        }
    }
}


fn bounds_from_element(element: &dyn Element) -> Option<aimer_attribute::Bounds> {
    element.event_tree_bounds().map(|(start, end)| {
        aimer_attribute::Bounds::new(
            start.x,
            start.y,
            end.x - start.x,
            end.y - start.y,
        )
    })
}

impl<T> Element for T where T: VisitorElement + EventElement + LayoutElement + Rebuildable + Drawable
{}

pub trait Element: VisitorElement + EventElement + LayoutElement + Rebuildable + Drawable {
    /// Returns this element's stable logical identity.
    ///
    /// The identity remains unchanged when the owning [`AnyElement`] moves and
    /// is transferred to compatible generated elements during reconciliation.
    #[inline]
    fn id(&self) -> ElementId {
        self.element_id()
            .expect("all erased elements must carry an ElementId")
    }
    /// Erases this element into an inline-or-heap [`AnyElement`].
    ///
    /// Elements fitting `Rubick`'s configured size and alignment are embedded
    /// directly in the returned owner. Larger or over-aligned elements use one
    /// heap allocation. The historical method name is retained for source
    /// familiarity and does not imply that allocation occurred.
    ///
    /// Borrowing the owner provides a `dyn Element` view. Moving an inline
    /// owner also moves its concrete element, so callers must not rely on a
    /// stable payload address without pinning.
    ///
    /// This method requires a sized, `'static` concrete element because stable
    /// Rust does not support general implicit unsizing for custom smart
    /// pointers.
    fn boxed(self) -> AnyElement
    where
        Self: Sized + 'static,
    {
        AnyElement::erase(ElementNode {
            id: Cell::new(ElementId::next()),
            element: self,
        })
    }

    /// Erases this element using the supplied application UI allocator.
    ///
    /// The resulting retained element keeps its allocation source alive until
    /// the element is dropped, including when it outlives the allocator scope.
    #[inline]
    fn boxed_in(self, allocator: &UiAllocator) -> AnyElement
    where
        Self: Sized + 'static,
    {
        AnyElement::erase_in(
            ElementNode {
                id: Cell::new(ElementId::next()),
                element: self,
                },
            allocator,
        )
    }
}

// SAFETY: The template is `null::<ElementNode<E>>()` coerced to the target, so
// it carries exactly that node's vtable and a null data address.
/// The most unmapped draws one frame keeps; a runaway tree must not grow it
/// without bound.
const UNMAPPED_DRAW_LIMIT: usize = 1024;

/// Notes that `id` was drawn under retained presentation with no render node.
#[inline]
fn record_unmapped_draw(id: ElementId, name: &'static str) {
    UNMAPPED_DRAWS.with(|draws| {
        let mut draws = draws.borrow_mut();
        if draws.len() < UNMAPPED_DRAW_LIMIT && !draws.iter().any(|(seen, _)| *seen == id) {
            draws.push((id, name));
        }
    });
}

/// Notes that `id` had no retained paint to show this frame (it declined, or
/// its state was not valid enough to paint).
#[cfg(debug_assertions)]
fn record_declined_paint(id: ElementId, name: &'static str) {
    DECLINED_PAINT_ELEMENTS.with(|elements| {
        let mut elements = elements.borrow_mut();
        if elements.len() < UNMAPPED_DRAW_LIMIT && !elements.iter().any(|(seen, _)| *seen == id) {
            elements.push((id, name));
        }
    });
}

/// Takes the elements that had no retained paint to show since the last call.
/// Only debug builds measure this, so release builds see none.
#[doc(hidden)]
pub fn take_declined_paint_elements() -> Vec<(ElementId, &'static str)> {
    #[cfg(debug_assertions)]
    {
        DECLINED_PAINT_ELEMENTS.with(|elements| std::mem::take(&mut *elements.borrow_mut()))
    }
    #[cfg(not(debug_assertions))]
    {
        Vec::new()
    }
}

/// Takes the elements recorded as drawn without a render node since the last
/// call. Frame loops call this to discard stale entries at frame start and
/// again after their final render-tree sync to see which are still unmapped.
#[doc(hidden)]
pub fn take_unmapped_draws() -> Vec<(ElementId, &'static str)> {
    UNMAPPED_DRAWS.with(|draws| std::mem::take(&mut *draws.borrow_mut()))
}

/// Returns the generation of the currently installed element-tree structure.
///
/// The generation advances whenever a generated child subtree is replaced. It
/// can be used to invalidate path indexes without rescanning the tree for every
/// event.
#[inline]
pub fn element_tree_generation() -> u64 {
    ELEMENT_TREE_GENERATION.load(Ordering::Acquire)
}

/// Returns the generation of dynamically exposed children used by retained rendering.
///
/// This is separate from [`element_tree_generation`], which also invalidates
/// event-path indexes. Virtualized sources advance this generation when their
/// live child window changes without replacing the owning element.
#[inline]
pub fn retained_render_structure_generation() -> u64 {
    RETAINED_RENDER_STRUCTURE_GENERATION.load(Ordering::Acquire)
}

/// Notifies retained rendering that an element's live child window changed.
///
/// Use this when `visit_children` starts exposing a different set of live
/// children while the owning element remains in place. Ordinary generated-tree
/// replacements already advance [`element_tree_generation`].
#[doc(hidden)]
#[inline]
pub fn notify_retained_render_structure_changed() {
    RETAINED_RENDER_STRUCTURE_GENERATION
        .fetch_update(Ordering::Release, Ordering::Relaxed, |generation| {
            generation.checked_add(1)
        })
        .expect("exhausted all retained render structure generations");
}

/// Makes the next rebuild pass reach every ticking element again.
///
/// A rebuild pass normally visits only the elements that were marked dirty and
/// the paths leading to them, and a stateless ancestor does not descend at all
/// while the rebuild generation is the one it last saw. An element that
/// advances on its own clock, such as a running animation, is never marked, so
/// a tick that publishes nothing new (an eased animation's first steps round to
/// the start value) would leave the pass skipping it, and the animation would
/// stop where it stood. Such an element calls this from `rebuild_if_dirty` for
/// as long as it still has frames to produce. It repaints the target, like any
/// rebuild invalidation, and leaves layout alone.
#[doc(hidden)]
#[inline]
pub fn keep_ticking_elements_reachable() {
    advance_rebuild_invalidation_generation();
}

/// Keeps the calling element reachable by the next rebuild pass, without
/// repainting anything.
///
/// A rebuild pass visits only the elements marked dirty and the paths leading to
/// them. An element that rebuilds its own child on its own clock, such as a
/// running [`AnimatedBuilder`], is never marked, so the pass would skip it and
/// the animation would stand still. Calling this from `rebuild_if_dirty` for as
/// long as the element still has frames to produce makes the next pass descend
/// to it again.
///
/// Unlike [`keep_ticking_elements_reachable`] this does not mark paint
/// invalidations unknown or damage the whole target: the caller's own rebuild
/// reports what it changed. Each call is counted, see
/// [`rebuild_keepalive_count`], so a frame loop can tell a generation that moved
/// only to keep an element reachable from one that moved because something
/// changed.
///
/// [`AnimatedBuilder`]: https://docs.rs/aimer_animation
#[doc(hidden)]
#[inline]
pub fn keep_element_reachable() {
    invalidate_dirty_paths();
    advance_tracked_rebuild_invalidation_generation();
    REBUILD_KEEPALIVE_COUNT.fetch_add(1, Ordering::Release);
}

/// Keeps one element reachable by the rebuild pass for as long as it holds.
///
/// [`keep_element_reachable`] makes the next pass descend to the caller by
/// discarding the index of dirty paths, so every animating frame walks the whole
/// tree to find one element. This registers the path to the element in that
/// index instead: the pass prunes everything off the path and still reaches the
/// element, and the index stays valid for every other consumer.
///
/// The holder owns its registration. Releasing it, or dropping it while it still
/// holds, removes the path again, so an element that is dropped mid-animation
/// cannot leave a path behind that keeps its ancestors descending.
///
/// Nothing is recorded as a paint invalidation: the holder's own rebuild reports
/// what it changed, as with [`keep_element_reachable`].
#[doc(hidden)]
#[derive(Default)]
pub struct KeepReachable {
    /// The registered path, empty while nothing is held.
    path: RefCell<Vec<ElementId>>,
}

impl KeepReachable {
    /// Creates a holder that holds nothing.
    #[inline]
    pub const fn new() -> Self {
        Self { path: RefCell::new(Vec::new()) }
    }

    /// Keeps the element being rebuilt reachable for the next pass.
    ///
    /// Call from `rebuild_if_dirty` on every frame the element still has work
    /// to produce. The registration only changes when the element's path does.
    /// Outside a rebuild pass there is no path to register, so this falls back
    /// to [`keep_element_reachable`].
    pub fn hold(&self) {
        let registered = REBUILD_PATH.with(|current| {
            let current = current.borrow();
            if current.is_empty() {
                return false;
            }
            let mut path = self.path.borrow_mut();
            if *path != *current {
                if !path.is_empty() {
                    remove_dirty_path(&path);
                }
                path.clear();
                path.extend_from_slice(&current);
                add_dirty_path(&path);
            }
            true
        });
        if registered {
            // Stateless ancestors skip their subtree while this is unchanged.
            advance_tracked_rebuild_invalidation_generation();
            REBUILD_KEEPALIVE_COUNT.fetch_add(1, Ordering::Release);
        } else {
            keep_element_reachable();
        }
    }

    /// Stops keeping the element reachable.
    pub fn release(&self) {
        let mut path = self.path.borrow_mut();
        if !path.is_empty() {
            remove_dirty_path(&path);
            path.clear();
        }
    }
}

impl Drop for KeepReachable {
    fn drop(&mut self) {
        self.release();
    }
}

/// Runs `commit`, a reconciliation of a subtree that the caller then passes to
/// [`rebuild_replaced_subtree`], without discarding the index of dirty paths.
///
/// A replacement normally cannot say which retained paths it left stale, so it
/// discards the index and the next pass walks the whole tree to rebuild it. A
/// caller that replaces the subtree below one element, and re-registers every
/// path in that subtree afterwards, has already done what the walk would.
///
/// Anything outside the replaced subtree is untouched by a scoped
/// reconciliation, so its paths stay valid.
#[doc(hidden)]
pub fn preserving_dirty_paths<R>(commit: impl FnOnce() -> R) -> R {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            PRESERVE_DIRTY_PATHS_DEPTH.with(|depth| depth.set(depth.get() - 1));
        }
    }
    PRESERVE_DIRTY_PATHS_DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _restore = Restore;
    commit()
}

/// Runs the rebuild pass over a subtree that [`preserving_dirty_paths`] just
/// replaced.
///
/// Call from the replaced subtree's owner, inside its `rebuild_if_dirty`. The
/// pass descends the whole subtree once instead of pruning it. Nothing in a
/// fresh subtree has been visited, so it cannot be told apart from one with
/// work in it, and the descent is what registers the subtree: every element
/// sets its dirty-source path from the pass's own path, replacing the one a
/// carried state holder brought with it from wherever it used to be. An element
/// in the subtree that keeps itself reachable, such as a nested animator, also
/// gets to do so.
///
/// Falls back to discarding the index when there is no pass to root the paths
/// in.
#[doc(hidden)]
pub fn rebuild_replaced_subtree(root: &dyn Element, ctx: &BuildContext) {
    if REBUILD_PATH.with(|path| path.borrow().is_empty()) {
        invalidate_dirty_paths();
    }
    REBUILD_FORCE_DESCEND_DEPTH.with(|depth| depth.set(depth.get() + 1));
    root.rebuild_if_dirty(ctx);
    REBUILD_FORCE_DESCEND_DEPTH.with(|depth| depth.set(depth.get() - 1));
}

/// How many retained boundaries the rebuild walk entered and did not prune.
///
/// A boundary that returns early because its dirty path holds no work is not
/// counted, so the difference across a frame is the size of the subtree the
/// rebuild prepass really descended.
#[doc(hidden)]
#[inline]
pub fn rebuild_descent_count() -> u64 {
    REBUILD_DESCENTS.with(Cell::get)
}

/// Rebuilds the children of the element being rebuilt that dirty paths run
/// through, and only those.
///
/// For [`Rebuildable::rebuild_indexed_children`]. `child_at` is the
/// container's own positional access to its children. Returns `false`, having
/// rebuilt whatever it reached, when the pass cannot rely on the edges: a child
/// is not where the container can find it, nothing leads below this element, or
/// the index grew or was discarded while the children rebuilt. The caller then
/// visits every child, which is always correct because each child decides for
/// itself whether it has work.
#[doc(hidden)]
pub fn rebuild_dirty_children<'a>(
    len: usize,
    child_at: impl Fn(usize) -> Option<&'a dyn Element>,
    ctx: &BuildContext,
) -> bool {
    let Some(parent) = REBUILD_PATH.with(|path| path.borrow().last().copied()) else {
        return false;
    };
    let wanted: SmallVec<[(ElementId, usize); 4]> = DIRTY_EDGES.with(|edges| {
        edges.borrow().get(&parent).map_or_else(SmallVec::new, |siblings| {
            siblings.iter().map(|edge| (edge.child, edge.hint.get())).collect()
        })
    });
    if wanted.is_empty() {
        return false;
    }

    let mut found: SmallVec<[(usize, &'a dyn Element); 4]> = SmallVec::new();
    for (id, hint) in wanted {
        let mut located = child_at(hint).filter(|child| child.id() == id).map(|child| (hint, child));
        if located.is_none() {
            located = (0..len).find_map(|index| {
                child_at(index).filter(|child| child.id() == id).map(|child| (index, child))
            });
        }
        let Some((index, child)) = located else {
            return false;
        };
        if index != hint {
            DIRTY_EDGES.with(|edges| {
                if let Some(edge) = edges
                    .borrow()
                    .get(&parent)
                    .and_then(|siblings| siblings.iter().find(|edge| edge.child == id))
                {
                    edge.hint.set(index);
                }
            });
        }
        found.push((index, child));
    }
    // The order a full visit would have used.
    found.sort_unstable_by_key(|(index, _)| *index);
    found.dedup_by_key(|(index, _)| *index);

    let version = DIRTY_EDGE_VERSION.with(Cell::get);
    for (_, child) in found {
        child.rebuild_if_dirty(ctx);
    }
    // A child that marked a sibling dirty, or a path that was registered or
    // discarded, may have work the snapshot did not know about.
    DIRTY_EDGE_VERSION.with(Cell::get) == version && DIRTY_PATHS_READY.with(Cell::get)
}

/// How many stable-generation changes the log remembers. A reader that falls
/// further behind finds its cursor gone and must check every child.
const MAX_LOGGED_STABLE_GENERATIONS: usize = 1024;

struct StableGenerationLog {
    /// Sequence number of `ids[0]`.
    first: u64,
    ids: Vec<ElementId>,
}

/// Returns the current end of the log of stable-generation changes.
///
/// Take it before looking at any child, then hand it to
/// [`stable_generations_since`] on the next pass.
#[doc(hidden)]
#[inline]
pub fn stable_generation_cursor() -> u64 {
    STABLE_GENERATION_LOG.with(|log| {
        let log = log.borrow();
        log.first + log.ids.len() as u64
    })
}

/// Calls `visit` with every element whose stable generation changed since
/// `cursor`, and returns `true`; returns `false`, visiting nothing, when the
/// log no longer reaches back that far.
///
/// An element that opts into generation-independent sizing changes size only
/// when it is replaced, and replacing it sets its generation. A container that
/// last checked its children at `cursor` therefore need only look at the
/// elements reported here, plus the children that did not opt in.
#[doc(hidden)]
pub fn stable_generations_since(cursor: u64, visit: &mut dyn FnMut(ElementId)) -> bool {
    STABLE_GENERATION_LOG.with(|log| {
        let log = log.borrow();
        let Some(skip) = cursor.checked_sub(log.first) else {
            return false;
        };
        for id in log.ids.iter().skip(skip as usize) {
            visit(*id);
        }
        true
    })
}

fn note_stable_generation_change(id: ElementId) {
    STABLE_GENERATION_LOG.with(|log| {
        let mut log = log.borrow_mut();
        log.ids.push(id);
        if log.ids.len() > MAX_LOGGED_STABLE_GENERATIONS {
            let dropped = log.ids.len() / 2;
            log.ids.drain(..dropped);
            log.first += dropped as u64;
        }
    });
}

/// How many retained boundaries the rebuild walk entered, pruned or not.
///
/// Together with [`rebuild_descent_count`] this separates the cost of
/// deciding to skip a subtree from the cost of walking it.
#[doc(hidden)]
#[inline]
pub fn rebuild_visit_count() -> u64 {
    REBUILD_VISITS.with(Cell::get)
}

#[inline]
pub(crate) fn note_rebuild_visit() {
    REBUILD_VISITS.with(|count| count.set(count.get() + 1));
}

#[inline]
pub(crate) fn note_rebuild_descent() {
    REBUILD_DESCENTS.with(|count| count.set(count.get() + 1));
}

/// How many times [`keep_element_reachable`] has advanced the rebuild
/// generation.
#[doc(hidden)]
#[inline]
pub fn rebuild_keepalive_count() -> u64 {
    REBUILD_KEEPALIVE_COUNT.load(Ordering::Acquire)
}

/// Records that a state update just marked an element dirty, advancing the
/// rebuild generation.
///
/// The element rebuilds its own subtree, which the log of replaced subtrees
/// describes, so a frame loop may treat the advance as explained, see
/// [`rebuild_state_mark_count`]. A dependency-driven mark is not counted: it
/// reaches every element that read the dependency.
#[doc(hidden)]
#[inline]
pub fn note_state_update_marked() {
    REBUILD_STATE_MARK_COUNT.fetch_add(1, Ordering::Release);
}

/// How many times a state update advanced the rebuild generation.
#[doc(hidden)]
#[inline]
pub fn rebuild_state_mark_count() -> u64 {
    REBUILD_STATE_MARK_COUNT.load(Ordering::Acquire)
}

/// Invalidates event paths and retained render nodes for a hosted child change.
///
/// Unlike [`notify_element_tree_changed`], this does not invalidate layout or
/// force full paint damage. Use it when a hosted subtree is added to or removed
/// from an overlay whose layout bounds remain stable.
#[doc(hidden)]
pub fn notify_hosted_element_tree_changed() {
    advance_element_tree_generation();
    notify_retained_render_structure_changed();
}

/// Returns the generation of the most recent layout invalidation.
///
/// Layout caches include this value in their keys. A layout invalidation can
/// therefore retire cached measurements without recursively visiting every
/// descendant of the retained tree.
#[inline]
pub fn layout_invalidation_generation() -> u64 {
    LAYOUT_INVALIDATION_GENERATION.load(Ordering::Acquire)
}

/// Advances the layout invalidation generation.
#[inline]
pub(crate) fn advance_layout_invalidation_generation() {
    LAYOUT_INVALIDATION_GENERATION
        .fetch_update(Ordering::Release, Ordering::Relaxed, |generation| {
            generation.checked_add(1)
        })
        .expect("exhausted all layout invalidation generations");
    queue_element_invalidation(None, ElementChangeKind::Layout);
}

/// Returns the generation of the most recent rebuild invalidation.
///
/// Paint caches include this value so a state, style, text, or other rebuild
/// that can change a subtree's visual output retires its retained commands
/// before they are replayed.
#[inline]
pub fn rebuild_invalidation_generation() -> u64 {
    REBUILD_INVALIDATION_GENERATION.load(Ordering::Acquire)
}

/// Advances the rebuild invalidation generation.
#[inline]
pub(crate) fn advance_rebuild_invalidation_generation() {
    invalidate_dirty_paths();
    advance_tracked_rebuild_invalidation_generation();
    // Async and portable-state producers do not have an element-owned dirty
    // path to record. Retained paint therefore cannot identify a smaller
    // footprint safely; invalidate it conservatively and repaint the target.
    mark_paint_invalidations_unknown();
    mark_paint_damage_full();
}

#[inline]
fn advance_tracked_rebuild_invalidation_generation() {
    REBUILD_INVALIDATION_GENERATION
        .fetch_update(Ordering::Release, Ordering::Relaxed, |generation| {
            generation.checked_add(1)
        })
        .expect("exhausted all rebuild invalidation generations");
}

/// Runs one public dirty-marking operation under a single invalidation bump.
pub(crate) fn with_rebuild_invalidation<R>(operation: impl FnOnce() -> R) -> R {
    let outermost = REBUILD_INVALIDATION_DEPTH.with(|depth| depth.get() == 0);
    if outermost {
        advance_tracked_rebuild_invalidation_generation();
    }
    REBUILD_INVALIDATION_DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _guard = RebuildInvalidationGuard;
    operation()
}

fn advance_element_tree_generation() {
    advance_element_tree_generation_counter();
    note_unscoped_tree_change();
}

/// Advances the tree generation for the replacement of the subtree at `root`.
///
/// Every consumer of [`element_tree_generation`] still sees the advance. A
/// consumer that tracks [`unscoped_element_tree_generation`] sees only that a
/// subtree was replaced, and which one.
fn advance_element_tree_generation_scoped(root: Option<ElementId>) {
    advance_element_tree_generation_counter();
    note_scoped_rebuild(root);
}

fn advance_element_tree_generation_counter() {
    #[cfg(test)]
    let _generation_lock = TEST_ELEMENT_TREE_GENERATION_LOCK
        .lock()
        .expect("element-tree generation test lock must not be poisoned");

    if PRESERVE_DIRTY_PATHS_DEPTH.with(|depth| depth.get() == 0) {
        invalidate_dirty_paths();
    }
    ELEMENT_TREE_GENERATION
        .fetch_update(Ordering::Release, Ordering::Relaxed, |generation| {
            generation.checked_add(1)
        })
        .expect("exhausted all element-tree generations");
}

/// Notifies retained traversal that an element's exposed child tree changed.
///
/// Most widgets should update children through rebuild reconciliation. A
/// widget that installs a child after asynchronous loading can use this hook
/// when its `visit_children` result changes without replacing its element.
/// Layout caches are invalidated alongside retained traversal.
#[doc(hidden)]
pub fn notify_element_tree_changed() {
    advance_element_tree_generation();
    advance_layout_invalidation_generation();
}

#[cfg(test)]
pub(crate) fn test_generation_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_ELEMENT_TREE_GENERATION_LOCK
        .lock()
        .expect("element-tree generation test lock must not be poisoned")
}

pub(crate) fn structural_children(element: &dyn Element) -> SmallVec<[&dyn Element; 8]> {
    let mut children: SmallVec<[&dyn Element; 8]> = SmallVec::new();
    element.structural_children(&mut |child| children.push(child));
    children
}

#[inline]
fn structural_child_at(element: &dyn Element, target: usize) -> Option<&dyn Element> {
    let mut index = 0;
    let mut result = None;
    element.structural_children(&mut |child| {
        if index == target {
            result = Some(child);
        }
        index = index.saturating_add(1);
    });
    result
}

/// Whether two elements describe the same widget in the same role.
///
/// Same concrete element type, same debug name and the same reconciliation key —
/// two keyed elements match only on equal keys, and a keyed one never matches an
/// unkeyed one.
pub(crate) fn identities_are_compatible(old: &dyn Element, new: &dyn Element) -> bool {
    if old.element_type_id() != new.element_type_id() || old.debug_name() != new.debug_name() {
        return false;
    }

    match (old.reconciliation_key(), new.reconciliation_key()) {
        (None, None) => true,
        (Some(old), Some(new)) => old == new,
        _ => false,
    }
}

/// Transfers logical identities from an old subtree to compatible nodes in a
/// newly generated subtree.
///
/// Keyed children match by key regardless of sibling order. Unkeyed children
/// match only in the same sibling position. Incompatible or removed nodes keep
/// their newly allocated identities.
#[cfg(test)]
pub(crate) fn reconcile_element_identities(old: &dyn Element, new: &dyn Element) {
    crate::reconciliation_plan::plan_element_reconciliation(old, new).apply_identities();
}

/// Reconciles identities for a generated subtree and invalidates structural
/// path indexes.
pub(crate) fn reconcile_generated_tree(old: &dyn Element, new: &dyn Element) {
    crate::reconciliation_plan::plan_element_reconciliation(old, new)
        .commit_generated_tree()
        .expect("fresh reconciliation plan must remain valid until commit");
}

pub(crate) fn complete_generated_tree_reconciliation(old: &dyn Element, new: &dyn Element) {
    clear_removed_focus(old, new);
    // The replacement changes the subtree below `new` and nothing outside it.
    advance_element_tree_generation_scoped(new.element_id());
    new.set_subtree_generation(element_tree_generation());
}

fn event_callback_enabled(element: &dyn Element) -> bool {
    element.event_tree_role() != EventTreeRole::Transparent
}

fn clear_removed_focus(old: &dyn Element, new: &dyn Element) {
    let Some((focused_element, focused_id, focused_node)) = find_focused_element(old) else {
        return;
    };
    if contains_focus_attachment(new, focused_id, focused_node) {
        return;
    }

    focused_node.set_focused(false);
    if event_callback_enabled(focused_element) {
        let _ = focused_element.on_event(&ElementEvent::FocusLost);
    }
}

fn find_focused_element(element: &dyn Element) -> Option<(&dyn Element, ElementId, &FocusNode)> {
    if let (Some(id), Some(node)) = (element.element_id(), element.focus_node())
        && node.has_focus()
    {
        return Some((element, id, node));
    }
    for child in structural_children(element) {
        if let Some(focused) = find_focused_element(child) {
            return Some(focused);
        }
    }
    None
}

fn contains_focus_attachment(
    element: &dyn Element,
    focused_id: ElementId,
    focused_node: &FocusNode,
) -> bool {
    if element.element_id() == Some(focused_id)
        && element
            .focus_node()
            .is_some_and(|node| node.ptr_eq(focused_node))
    {
        return true;
    }
    structural_children(element)
        .into_iter()
        .any(|child| contains_focus_attachment(child, focused_id, focused_node))
}
