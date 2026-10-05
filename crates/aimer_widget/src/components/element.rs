use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_cupid::compositor::{SceneNodeDescriptor, SceneNodeId, SceneRevision};
use aimer_cupid::damage_region::{DamageRect, DamageSet};
use aimer_cupid::draw_cmd_v2::{RenderNodeId, RenderPaintSource, RenderTree};
use aimer_cupid::utilities::Rect;
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
mod node;

use node::ElementNode;
pub use event::{
    ElementPath, EventDispatchContext, EventDispatcher, broadcast_event, dispatch_event,
    dispatch_focused_event,
};

static NEXT_ELEMENT_ID: AtomicU64 = AtomicU64::new(1);
static ELEMENT_TREE_GENERATION: AtomicU64 = AtomicU64::new(0);
static RETAINED_RENDER_STRUCTURE_GENERATION: AtomicU64 = AtomicU64::new(0);
static REBUILD_INVALIDATION_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAYOUT_INVALIDATION_GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static TEST_ELEMENT_TREE_GENERATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    /// Active per-window v2 recording context. It is installed only while the
    /// UI thread traverses the retained element tree for one frame.
    static V2_RENDER_CONTEXT_STACK: RefCell<Vec<V2RenderContext>> = const { RefCell::new(Vec::new()) };

    /// Nested `mark_needs_rebuild` calls share one invalidation generation.
    /// Marking a large tree is already recursive; it must not also perform one
    /// atomic increment per descendant.
    static REBUILD_INVALIDATION_DEPTH: Cell<usize> = const { Cell::new(0) };

    /// Revisions are kept outside [`ElementNode`] so adding stable-layout
    /// tracking does not change the inline-erased element size. Only elements
    /// that opt into generation-independent sizing create entries here.
    static STABLE_SUBTREE_GENERATIONS: RefCell<HashMap<ElementId, u64>> =
        RefCell::new(HashMap::new());

    /// Number of dirty rebuild sources whose root-relative path crosses each
    /// retained element. Counts keep shared ancestors indexed until every
    /// dirty source below them has been rebuilt.
    static DIRTY_SUBTREE_COUNTS: RefCell<HashMap<ElementId, usize>> =
        RefCell::new(HashMap::new());

    /// Event dispatchers use this UI-thread epoch to coalesce generation checks
    /// across all nested dispatchers until the next completed frame.
    static EVENT_FRAME_EPOCH: Cell<Option<u64>> = const { Cell::new(None) };

    /// The path index is usable only after one complete walk of the retained
    /// tree without a structural replacement or an untracked invalidation.
    static DIRTY_PATHS_READY: Cell<bool> = const { Cell::new(false) };
    static DIRTY_INDEXED_ROOTS: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    static DIRTY_PATHS_INVALIDATED_DURING_TRAVERSAL: Cell<bool> = const { Cell::new(false) };
    static REBUILD_TRAVERSAL_DEPTH: Cell<usize> = const { Cell::new(0) };
    static REBUILD_PATH: RefCell<Vec<ElementId>> = const { RefCell::new(Vec::new()) };
    static REBUILD_FORCE_DESCEND_DEPTH: Cell<usize> = const { Cell::new(0) };
    static REBUILD_MARK_DEPTH: Cell<usize> = const { Cell::new(0) };
    static DRAW_DEPTH: Cell<usize> = const { Cell::new(0) };
    static DRAW_INVALIDATION_OWNER: Cell<Option<ElementId>> = const { Cell::new(None) };
    /// Paint invalidation metadata is retained for the current pair of
    /// rebuild/tree generations so a retained scroll tile can distinguish a
    /// local dirty element from an unrelated branch's rebuild.
    static PAINT_INVALIDATION_EPOCH: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
    static PAINT_INVALIDATED_ELEMENTS: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    static PAINT_INVALIDATED_LOCAL_ELEMENTS: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    static PAINT_INVALIDATED_SUBTREES: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    static PAINT_INVALIDATION_UNKNOWN: Cell<bool> = const { Cell::new(false) };
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
    /// One identity set per retained recording operation. A stack keeps a
    /// nested scroll's recording isolated from the tile currently being
    /// recorded by its parent.
    static PAINT_TRACKING_STACK: RefCell<Vec<HashSet<ElementId>>> = const { RefCell::new(Vec::new()) };
    #[cfg(feature = "frame-stats")]
    static DRAW_TRAVERSAL_COUNT: Cell<u64> = const { Cell::new(0) };
    #[cfg(feature = "frame-stats")]
    static ROUTED_EVENT_VISIT_COUNT: Cell<u64> = const { Cell::new(0) };
    #[cfg(test)]
    static HOVER_MEMBERSHIP_CHECK_COUNT: Cell<u64> = const { Cell::new(0) };
}

#[derive(Clone)]
pub(crate) struct V2RenderContext {
    pub(crate) tree: RenderTree,
    element_nodes: Rc<std::collections::HashMap<ElementId, RenderNodeId>>,
    legacy_island_depth: Rc<Cell<usize>>,
    prepared_root: Rc<Cell<Option<ElementId>>>,
    retained_presentation: bool,
}

impl V2RenderContext {
    #[inline]
    pub(crate) fn node_for_element(&self, element: ElementId) -> Option<RenderNodeId> {
        self.element_nodes.get(&element).copied()
    }

    #[inline]
    pub(crate) fn inside_legacy_island(&self) -> bool {
        self.legacy_island_depth.get() != 0
    }

    #[inline]
    pub(crate) fn uses_retained_presentation(&self) -> bool {
        self.retained_presentation && !self.inside_legacy_island()
    }

    pub(crate) fn enter_legacy_island(&self) -> LegacyIslandGuard {
        self.legacy_island_depth
            .set(self.legacy_island_depth.get() + 1);
        LegacyIslandGuard {
            depth: self.legacy_island_depth.clone(),
        }
    }

    #[inline]
    pub(crate) fn take_prepared_root(&self, element: ElementId) -> bool {
        self.prepared_root.get() == Some(element) && self.prepared_root.take().is_some()
    }
}

pub(crate) struct LegacyIslandGuard {
    depth: Rc<Cell<usize>>,
}

impl Drop for LegacyIslandGuard {
    fn drop(&mut self) {
        self.depth.set(self.depth.get() - 1);
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

/// Runs one element-tree traversal with the window's retained v2 render map.
#[doc(hidden)]
pub fn with_v2_render_tree_context<R>(
    tree: RenderTree,
    element_nodes: Rc<std::collections::HashMap<ElementId, RenderNodeId>>,
    prepared_root: Option<ElementId>,
    callback: impl FnOnce() -> R,
) -> R {
    with_v2_render_tree_context_mode(tree, element_nodes, prepared_root, false, callback)
}

/// Runs the element traversal with the retained tree as the active paint
/// source. Local v2 nodes suppress their duplicate legacy paint commands while
/// still traversing children; legacy islands temporarily resume recording.
#[doc(hidden)]
pub fn with_v2_render_tree_presentation_context<R>(
    tree: RenderTree,
    element_nodes: Rc<std::collections::HashMap<ElementId, RenderNodeId>>,
    prepared_root: Option<ElementId>,
    callback: impl FnOnce() -> R,
) -> R {
    with_v2_render_tree_context_mode(tree, element_nodes, prepared_root, true, callback)
}

fn with_v2_render_tree_context_mode<R>(
    tree: RenderTree,
    element_nodes: Rc<std::collections::HashMap<ElementId, RenderNodeId>>,
    prepared_root: Option<ElementId>,
    retained_presentation: bool,
    callback: impl FnOnce() -> R,
) -> R {
    V2_RENDER_CONTEXT_STACK.with(|stack| {
        stack.borrow_mut().push(V2RenderContext {
            tree,
            element_nodes,
            legacy_island_depth: Rc::new(Cell::new(0)),
            prepared_root: Rc::new(Cell::new(prepared_root)),
            retained_presentation,
        });
    });
    let _guard = V2RenderContextGuard;
    callback()
}

pub(crate) fn current_v2_render_context() -> Option<V2RenderContext> {
    V2_RENDER_CONTEXT_STACK.with(|stack| stack.borrow().last().cloned())
}

/// Returns whether the current draw is using the window's retained v2 tree
/// outside a legacy-island replay.
#[doc(hidden)]
pub fn has_active_v2_render_tree() -> bool {
    current_v2_render_context().is_some_and(|context| !context.inside_legacy_island())
}

/// Returns whether retained v2 paint is the current presentation source.
///
/// Legacy-island replay returns false because those subtrees are intentionally
/// recorded into the compatibility command list.
#[doc(hidden)]
pub fn has_active_v2_render_presentation() -> bool {
    current_v2_render_context().is_some_and(|context| context.uses_retained_presentation())
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
    let Some(context) = current_v2_render_context()
        .filter(|context| !context.inside_legacy_island())
    else {
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
        PAINT_INVALIDATED_ELEMENTS.with(|elements| elements.borrow_mut().clear());
        PAINT_INVALIDATED_LOCAL_ELEMENTS.with(|elements| elements.borrow_mut().clear());
        PAINT_INVALIDATED_SUBTREES.with(|subtrees| subtrees.borrow_mut().clear());
        PAINT_INVALIDATION_UNKNOWN.with(|unknown| unknown.set(false));
    }
}

fn record_paint_invalidation_path(path: &[ElementId]) {
    let Some(element) = path.last().copied() else {
        return;
    };
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATED_ELEMENTS.with(|elements| {
        elements.borrow_mut().insert(element);
    });
    PAINT_INVALIDATED_SUBTREES.with(|subtrees| {
        subtrees.borrow_mut().extend(path.iter().copied());
    });
}

fn record_local_paint_invalidation_path(path: &[ElementId]) {
    record_paint_invalidation_path(path);
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
    REBUILD_PATH.with(|path| {
        let path = path.borrow();
        if path.is_empty() {
            if owns_paint {
                record_local_paint_invalidation_path(&[element]);
            } else {
                record_paint_invalidation_path(&[element]);
            }
        } else {
            if owns_paint {
                record_local_paint_invalidation_path(&path);
            } else {
                record_paint_invalidation_path(&path);
            }
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
    PAINT_INVALIDATION_UNKNOWN.with(|unknown| unknown.set(true));
    queue_element_invalidation(None, ElementChangeKind::Unknown);
}

/// Starts collecting the logical element identities reached by one retained
/// paint recording operation.
#[doc(hidden)]
pub fn begin_paint_tracking() {
    PAINT_TRACKING_STACK.with(|stack| stack.borrow_mut().push(HashSet::new()));
}

/// Finishes the innermost retained paint recording operation and returns the
/// identities it reached. The returned set is empty when tracking was not
/// active.
#[doc(hidden)]
pub fn take_paint_tracking() -> Vec<ElementId> {
    PAINT_TRACKING_STACK.with(|stack| {
        stack
            .borrow_mut()
            .pop()
            .map(|elements| elements.into_iter().collect())
            .unwrap_or_default()
    })
}

#[inline]
fn record_paint_element(element: ElementId) {
    PAINT_TRACKING_STACK.with(|stack| {
        if let Some(elements) = stack.borrow_mut().last_mut() {
            elements.insert(element);
        }
    });
}

/// Returns whether an element in the current invalidation epoch was marked as
/// dirty or rebuilt.
#[doc(hidden)]
pub fn paint_element_was_invalidated(element: ElementId) -> bool {
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATED_ELEMENTS.with(|elements| elements.borrow().contains(&element))
}

/// Returns whether this element's own local retained paint was invalidated.
#[doc(hidden)]
pub fn local_paint_element_was_invalidated(element: ElementId) -> bool {
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATED_LOCAL_ELEMENTS.with(|elements| elements.borrow().contains(&element))
}

/// Marks one reconciled element's retained paint as stale in this invalidation epoch.
#[doc(hidden)]
pub(crate) fn mark_paint_element_invalidated(element: ElementId) {
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATED_ELEMENTS.with(|elements| {
        elements.borrow_mut().insert(element);
    });
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
    EVENT_PAINT_INVALIDATIONS.with(|invalidations| invalidations.borrow_mut().remove(&element))
}

/// Returns whether the current invalidation epoch crossed a retained subtree.
#[doc(hidden)]
pub fn paint_subtree_was_invalidated(root: ElementId) -> bool {
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATED_SUBTREES.with(|subtrees| subtrees.borrow().contains(&root))
}

/// Returns whether all known paint invalidations in the current epoch could be
/// attributed to an element path. Unknown producers must use the conservative
/// complete-cache invalidation path.
#[doc(hidden)]
pub fn paint_invalidations_are_known() -> bool {
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATION_UNKNOWN.with(|unknown| !unknown.get())
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

fn add_dirty_path(path: &[ElementId]) {
    DIRTY_SUBTREE_COUNTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        for id in path {
            *counts.entry(*id).or_default() += 1;
        }
    });
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
    #[cfg(test)]
    let _generation_lock = TEST_ELEMENT_TREE_GENERATION_LOCK
        .lock()
        .expect("element-tree generation test lock must not be poisoned");

    invalidate_dirty_paths();
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
    advance_element_tree_generation();
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
