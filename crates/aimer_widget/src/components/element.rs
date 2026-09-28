use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_cupid::compositor::{SceneNodeDescriptor, SceneNodeId, SceneRevision};
use aimer_cupid::damage_region::{DamageRect, DamageSet};
use aimer_cupid::utilities::Rect;
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
#[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
use aimer_events::window::request_animation_frame;
use aimer_focus::{FocusCandidate, FocusCandidates, FocusManager, FocusNode, FocusTrapId};
use aimer_rubick::ErasedFrom;
use smallvec::SmallVec;
use hashbrown::{HashMap, HashSet};

use crate::base::*;
use crate::components::event_element::{
    CaptureRequest, EventElement, EventResult, EventTreeRole, FollowUp, PointerKey,
};
use crate::components::layout_element::LayoutElement;
use crate::components::rebuildable::Rebuildable;
pub(crate) use crate::components::visitor_element::VisitorElement;
use crate::pointer_claim;
use crate::components::drawable::{CompositorAnimationDecision, CompositorAnimationFrame};
#[cfg(feature = "event-tree-exp")]
use crate::event_tree::{EventTargetId, EventTree};
use crate::{AnyElement, Drawable, Key};

type EventChildren<'a> = SmallVec<[&'a dyn Element; 32]>;

static NEXT_ELEMENT_ID: AtomicU64 = AtomicU64::new(1);
static ELEMENT_TREE_GENERATION: AtomicU64 = AtomicU64::new(0);
static REBUILD_INVALIDATION_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAYOUT_INVALIDATION_GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static TEST_ELEMENT_TREE_GENERATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
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
    /// Paint invalidation metadata is retained for the current pair of
    /// rebuild/tree generations so a retained scroll tile can distinguish a
    /// local dirty element from an unrelated branch's rebuild.
    static PAINT_INVALIDATION_EPOCH: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
    static PAINT_INVALIDATED_ELEMENTS: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    static PAINT_INVALIDATED_SUBTREES: RefCell<HashSet<ElementId>> = RefCell::new(HashSet::new());
    static PAINT_INVALIDATION_UNKNOWN: Cell<bool> = const { Cell::new(false) };
    /// Damage accumulated by retained paint owners during the current frame.
    /// The target is thread-local because widget trees and their canvases are
    /// single-threaded; the finished [`DamageSet`] is copied into the frame
    /// packet before presentation moves to another thread.
    static FRAME_PAINT_DAMAGE: RefCell<Option<DamageSet>> = const { RefCell::new(None) };
    /// A platform event can invalidate paint before the next frame opens its
    /// damage collector. Keep that request until [`begin_paint_frame`] consumes
    /// it instead of silently dropping the invalidation.
    static PENDING_PAINT_DAMAGE_FULL: Cell<bool> = const { Cell::new(false) };
    static FRAME_PAINT_TARGET: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
    /// One identity set per retained recording operation. A stack keeps a
    /// nested scroll's recording isolated from the tile currently being
    /// recorded by its parent.
    static PAINT_TRACKING_STACK: RefCell<Vec<HashSet<ElementId>>> = const { RefCell::new(Vec::new()) };
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    static DRAW_TRAVERSAL_COUNT: Cell<u64> = const { Cell::new(0) };
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    static ROUTED_EVENT_VISIT_COUNT: Cell<u64> = const { Cell::new(0) };
    #[cfg(test)]
    static HOVER_MEMBERSHIP_CHECK_COUNT: Cell<u64> = const { Cell::new(0) };
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
    FRAME_PAINT_DAMAGE
        .with(|damage| damage.borrow_mut().take())
        .unwrap_or_else(|| DamageSet::full(width, height))
}

/// Adds one conservative device-pixel footprint to the current frame.
#[doc(hidden)]
pub fn mark_paint_damage(rectangle: DamageRect) {
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

fn record_current_paint_invalidation(element: ElementId) {
    REBUILD_PATH.with(|path| {
        let path = path.borrow();
        if path.is_empty() {
            record_paint_invalidation_path(&[element]);
        } else {
            record_paint_invalidation_path(&path);
        }
    });
}

fn mark_paint_invalidations_unknown() {
    sync_paint_invalidation_epoch();
    PAINT_INVALIDATION_UNKNOWN.with(|unknown| unknown.set(true));
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
#[cfg(any(debug_assertions, feature = "frame-stats"))]
pub fn reset_draw_traversal_count() {
    DRAW_TRAVERSAL_COUNT.with(|count| count.set(0));
}

/// Takes the number of retained element draw calls observed since the last
/// reset. A draw call is counted for every element reached by the drawable
/// traversal, including a scroll container whose children may be culled.
#[cfg(any(debug_assertions, feature = "frame-stats"))]
pub fn take_draw_traversal_count() -> u64 {
    DRAW_TRAVERSAL_COUNT.with(Cell::get)
}

/// Resets the routed-event visit counter for the next measured input sample.
#[cfg(any(debug_assertions, feature = "frame-stats"))]
#[doc(hidden)]
pub fn reset_routed_event_visit_count() {
    ROUTED_EVENT_VISIT_COUNT.with(|count| count.set(0));
}

/// Takes the number of elements reached by routed pointer dispatch since the
/// last reset. Cached hit-chain replay contributes one visit per delivered
/// element, so this counter measures the work visible to the event path rather
/// than only calls into the uncached recursive walker.
#[cfg(any(debug_assertions, feature = "frame-stats"))]
#[doc(hidden)]
pub fn take_routed_event_visit_count() -> u64 {
    ROUTED_EVENT_VISIT_COUNT.with(Cell::get)
}

#[inline]
fn record_routed_event_visit() {
    crate::frame_work_stats::record_hit_test_visit();
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
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

#[cfg(any(debug_assertions, feature = "frame-stats"))]
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
                if let Some(path) = path.as_deref() {
                    record_paint_invalidation_path(path);
                    add_dirty_path(path);
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
            record_paint_invalidation_path(&new_path);
            add_dirty_path(&new_path);
            self.indexed.set(true);
        }
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

struct ElementNode<E> {
    id: Cell<ElementId>,
    element: E,
}

#[cfg(feature = "event-tree-exp")]
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

impl<E: Element + 'static> ElementNode<E> {
    #[inline]
    fn draw_live(&self, ctx: &BuildContext, frame: Option<CompositorAnimationFrame>) {
        let before = element_tree_generation();
        if let Some(frame) = frame {
            self.element.draw_with_compositor_animation(ctx, frame);
        } else {
            self.element.draw(ctx);
        }
        let after = element_tree_generation();
        if after != before {
            self.set_subtree_generation(after);
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
    fn draw_compositor_animation(&self, ctx: &BuildContext, frame: CompositorAnimationFrame) {
        let bounds = self.element.content_size(ctx);
        let priority = self.element.compositor_priority();
        let descriptor = SceneNodeDescriptor {
            id: SceneNodeId::from_raw(self.id.get().get()),
            bounds: Rect::new(0.0, 0.0, bounds.width, bounds.height),
            order: self.element.layer(),
            revision: SceneRevision::new(
                self.subtree_generation(),
                paint_element_was_invalidated(self.id.get())
                    .then_some(rebuild_invalidation_generation())
                    .unwrap_or(0),
                layout_invalidation_generation(),
                ctx.canvas.texture_cache_epoch(),
            ),
            // The provider has proved that its static paint is safe to retain;
            // the animation itself is represented by the scene state captured
            // after `frame.apply` below.
            cache_eligible: true,
            bounded: true,
            priority,
        };

        ctx.canvas.save();
        frame.apply(ctx);
        let scene_node = ctx.canvas.begin_scene_node(descriptor);
        self.element.sync_paint_geometry(ctx);

        let retained = if let Some(key) = crate::paint_isolated::PaintContract::new(
            ctx,
            bounds,
            self.subtree_generation(),
        ) {
            crate::paint_isolated::paint_or_replay_scene_node(
                self.id.get(),
                ctx,
                &self.element,
                key,
                priority,
                true,
            )
        } else {
            false
        };

        if retained {
            self.element.update_compositor_animation_damage(ctx, frame);
        }

        drop(scene_node);
        frame.clear(ctx);
        ctx.canvas.restore();

        if retained {
            if frame.active {
                request_animation_frame();
            }
        } else {
            // The retained recorder can reject a command stream or be
            // invalidated by an unknown paint source. Replay the same sample
            // on the live path so the controller is not ticked twice.
            self.draw_live(ctx, Some(frame));
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
impl<E> Drop for ElementNode<E> {
    fn drop(&mut self) {
        crate::paint_isolated::drop_scene_paint_cache(self.id.get());
    }
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
}

// SAFETY: The template is `null::<ElementNode<E>>()` coerced to the target, so
// it carries exactly that node's vtable and a null data address.
unsafe impl<E: Element + 'static> ErasedFrom<ElementNode<E>> for dyn Element {
    const TEMPLATE: *const Self = std::ptr::null::<ElementNode<E>>() as *const dyn Element;
}

impl<E: Element + 'static> VisitorElement for ElementNode<E> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.visit_children(visitor);
    }

    fn debug_name(&self) -> &'static str {
        self.element.debug_name()
    }

    fn element_type_id(&self) -> TypeId {
        TypeId::of::<E>()
    }

    fn reconciliation_key(&self) -> Option<&Key> {
        self.element.reconciliation_key()
    }

    fn element_id(&self) -> Option<ElementId> {
        Some(self.id.get())
    }

    fn set_element_id(&self, id: ElementId) {
        self.id.set(id);
    }
}

impl<E: Element + 'static> LayoutElement for ElementNode<E> {
    fn pos(&self) -> Option<Vec2d> {
        self.element.pos()
    }

    fn size(&self) -> Option<Size> {
        self.element.size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        crate::frame_work_stats::record_layout_call();
        self.element.layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.element.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.element.content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.element.layer()
    }

    fn flex(&self) -> Option<f32> {
        self.element.flex()
    }

    fn is_layout_stable(&self) -> bool {
        self.element.is_layout_stable()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.element.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        // Layout caches carry this generation in their keys, so invalidating
        // the retained root is one marker write rather than a recursive walk
        // through every child. The concrete element's old recursive method is
        // still available when a caller owns that concrete element directly;
        // erased trees use this boundary for the normal frame path.
        advance_layout_invalidation_generation();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.element.pos_start_end()
    }

    fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
        self.element.event_tree_bounds()
    }
}

impl<E: Element + 'static> Rebuildable for ElementNode<E> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        #[cfg(any(debug_assertions, feature = "frame-stats"))]
        crate::rebuild_stats::record_visit();
        let mut traversal = begin_rebuild_traversal(self.id.get());
        let _path = RebuildPathGuard::push(self.id.get());
        let own_dirty = element_has_dirty_work(&self.element);
        let path_ready = DIRTY_PATHS_READY.with(Cell::get)
            && (REBUILD_TRAVERSAL_DEPTH.with(|depth| depth.get() > 1)
                || DIRTY_INDEXED_ROOTS.with(|roots| roots.borrow().contains(&self.id.get())));
        let forced_descend = REBUILD_FORCE_DESCEND_DEPTH.with(|depth| depth.get() > 0);
        if path_ready
            && !forced_descend
            && !own_dirty
            && !dirty_path_contains(self.id.get())
        {
            traversal.complete = true;
            #[cfg(any(debug_assertions, feature = "frame-stats"))]
            crate::rebuild_stats::record_pruned();
            return;
        }

        set_element_rebuild_path(&self.element);
        let _descend = RebuildDescendGuard::enter(own_dirty);
        let before = element_tree_generation();
        self.element.rebuild_if_dirty(ctx);
        let after = element_tree_generation();
        if after != before {
            self.set_subtree_generation(after);
        }
        if own_dirty || after != before {
            record_current_paint_invalidation(self.id.get());
        }
        traversal.complete = true;
    }

    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        let before = element_tree_generation();
        self.element.adopt_runtime_state_from(old);
        let after = element_tree_generation();
        if after != before {
            self.set_subtree_generation(after);
        }
    }

    fn subtree_generation(&self) -> u64 {
        if !self.element.is_layout_stable() {
            return element_tree_generation();
        }
        STABLE_SUBTREE_GENERATIONS.with(|generations| {
            generations.borrow().get(&self.id.get()).copied().unwrap_or(0)
        })
    }

    fn set_subtree_generation(&self, generation: u64) {
        if self.element.is_layout_stable() {
            STABLE_SUBTREE_GENERATIONS.with(|generations| {
                generations.borrow_mut().insert(self.id.get(), generation);
            });
        }
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.element.option_any()
    }

    fn is_stateful_element(&self) -> bool {
        self.element.is_stateful_element()
    }

    fn is_carry_state(&self) -> bool {
        self.element.is_carry_state()
    }

    fn compositor_priority(&self) -> bool {
        self.element.compositor_priority()
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.element.with_rebuild_context(ctx, callback);
    }

    fn mark_needs_rebuild(&self) {
        let (_mark, outermost) = begin_rebuild_mark();
        let has_rebuild_source = element_has_rebuild_source(&self.element);
        // The erased owner is the boundary used by the retained tree. Keep one
        // invalidation generation for the complete recursive mark, including
        // custom containers that implement their own forwarding method.
        let unknown = outermost && !has_rebuild_source;
        with_rebuild_invalidation(|| {
            if unknown {
                // A custom rebuild producer has no path to publish. Preserve
                // the old conservative behavior so its mark can never make
                // the index incorrectly skip work.
                mark_paint_invalidations_unknown();
                invalidate_dirty_paths();
            }
            self.element.mark_needs_rebuild();
        });
        // A stateful element may delegate the mark to a carrying child without
        // marking itself. If that child contains an untracked custom producer,
        // the retained path index has no source to point at; fall back to the
        // conservative walk for this explicit recursive mark.
        if outermost && has_rebuild_source && !element_has_dirty_work(&self.element) {
            mark_paint_invalidations_unknown();
            invalidate_dirty_paths();
        }
    }
}

fn element_has_rebuild_source(element: &dyn Element) -> bool {
    let Some(value) = element.option_any() else {
        return false;
    };
    value.is::<crate::widget::stateless::StatelessElement>()
        || value.is::<crate::widget::stateful::StatefulElement>()
}

fn element_has_dirty_work(element: &dyn Element) -> bool {
    let Some(value) = element.option_any() else {
        return false;
    };
    if let Some(stateless) = value
        .downcast_ref::<crate::widget::stateless::StatelessElement>()
    {
        return stateless.dirty.get();
    }
    value
        .downcast_ref::<crate::widget::stateful::StatefulElement>()
        .is_some_and(|stateful| stateful.dirty.borrow().get())
}

fn set_element_rebuild_path(element: &dyn Element) {
    let Some(value) = element.option_any() else {
        return;
    };
    REBUILD_PATH.with(|path| {
        let path = path.borrow();
        if let Some(stateless) = value
            .downcast_ref::<crate::widget::stateless::StatelessElement>()
        {
            stateless.dirty_source.set_path(&path);
        } else if let Some(stateful) = value
            .downcast_ref::<crate::widget::stateful::StatefulElement>()
        {
            stateful.dirty_source.borrow().set_path(&path);
        }
    });
}

impl<E: Element + 'static> EventElement for ElementNode<E> {
    fn event_tree_role(&self) -> EventTreeRole {
        self.element.event_tree_role()
    }

    fn focus_node(&self) -> Option<&FocusNode> {
        self.element.focus_node()
    }

    fn autofocus(&self) -> bool {
        self.element.autofocus()
    }

    fn traps_focus(&self) -> bool {
        self.element.traps_focus()
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.element.on_event(event)
    }

    fn on_event_with_context(
        &self,
        event: &ElementEvent,
        context: &mut EventDispatchContext<'_, '_>,
    ) -> EventResult {
        self.element.on_event_with_context(event, context)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.event_children(visitor);
    }

    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.structural_children(visitor);
    }

    fn focus_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.focus_children(visitor);
    }

    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.hit_test_children(visitor);
    }

    fn hit_test_children_reversed<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.hit_test_children_reversed(visitor);
    }

    fn hit_test_children_at<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.element.hit_test_children_at(pos, visitor);
    }

    fn hit_test_children_at_reversed<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.element.hit_test_children_at_reversed(pos, visitor);
    }

    #[inline]
    fn has_overlapping_hit_targets(&self) -> bool {
        self.element.has_overlapping_hit_targets()
    }

}

impl<E: Element + 'static> Drawable for ElementNode<E> {
    fn draw(&self, ctx: &BuildContext) {
        crate::frame_work_stats::record_paint_call();
        #[cfg(any(debug_assertions, feature = "frame-stats"))]
        record_draw_traversal();
        record_paint_element(self.id.get());
        let (_draw, outermost) = begin_draw();
        if outermost {
            // Native frame dispatch enters through `draw`; keep the retained-
            // tree rebuild prepass here so direct draw callers also benefit
            // from the precise dirty-subtree index. Child draws belong to the
            // same pass and must not reset paths relative to a new root.
            self.rebuild_if_dirty(ctx);
        }
        let priority = self.element.compositor_priority();
        let stable = self.element.is_paint_stable() && self.element.is_layout_stable();
        let bounded = self.element.is_paint_bounded();

        #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
        if !stable && bounded {
            match self.element.compositor_animation(ctx) {
                CompositorAnimationDecision::Compositor(frame) => {
                    self.draw_compositor_animation(ctx, frame);
                    return;
                }
                CompositorAnimationDecision::Live(frame) => {
                    self.draw_live(ctx, Some(frame));
                    return;
                }
                CompositorAnimationDecision::None => {}
            }
        }

        let _scene_node = if (stable && bounded) || priority {
            let bounds = self.element.content_size(ctx);
            let descriptor = SceneNodeDescriptor {
                id: SceneNodeId::from_raw(self.id.get().get()),
                bounds: Rect::new(0.0, 0.0, bounds.width, bounds.height),
                order: self.element.layer(),
                revision: SceneRevision::new(
                    self.subtree_generation(),
                    paint_element_was_invalidated(self.id.get())
                        .then_some(rebuild_invalidation_generation())
                        .unwrap_or(0),
                    layout_invalidation_generation(),
                    ctx.canvas.texture_cache_epoch(),
                ),
                cache_eligible: stable && bounded,
                bounded,
                priority,
            };
            let scene_node = Some(ctx.canvas.begin_scene_node(descriptor));
            self.element.sync_paint_geometry(ctx);

            #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
            if stable
                && bounded
                && let Some(key) = crate::paint_isolated::PaintContract::new(
                    ctx,
                    bounds,
                    self.subtree_generation(),
                )
                && crate::paint_isolated::paint_or_replay_scene_node(
                    self.id.get(),
                    ctx,
                    &self.element,
                    key,
                    priority,
                    stable,
                )
            {
                return;
            }
            scene_node
        } else {
            None
        };
        self.draw_live(ctx, None);
    }

    #[inline]
    fn paint(&self, ctx: &BuildContext) {
        record_paint_element(self.id.get());
        self.element.paint(ctx);
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.element.sync_paint_geometry(ctx);
    }

    #[inline]
    fn prepare_layout(&self, ctx: &BuildContext) -> bool {
        self.element.prepare_layout(ctx)
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.element.is_paint_stable()
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        self.element.is_paint_bounded()
    }

    #[inline]
    fn compositor_animation(&self, ctx: &BuildContext) -> CompositorAnimationDecision {
        self.element.compositor_animation(ctx)
    }

    #[inline]
    fn draw_with_compositor_animation(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.element.draw_with_compositor_animation(ctx, frame);
    }

    #[inline]
    fn update_compositor_animation_damage(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.element
            .update_compositor_animation_damage(ctx, frame);
    }

    #[inline]
    fn draw_paint_islands(
        &self,
        retained_ctx: &BuildContext,
        live_ctx: &BuildContext,
        draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) -> bool {
        self.element.draw_paint_islands(
            retained_ctx,
            live_ctx,
            draw_stable,
            draw_dynamic,
        )
    }
}

impl VisitorElement for AnyElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().visit_children(visitor)
    }

    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }

    fn element_type_id(&self) -> std::any::TypeId {
        self.as_ref().element_type_id()
    }

    fn reconciliation_key(&self) -> Option<&Key> {
        self.as_ref().reconciliation_key()
    }

    fn element_id(&self) -> Option<ElementId> {
        self.as_ref().element_id()
    }

    fn set_element_id(&self, id: ElementId) {
        self.as_ref().set_element_id(id);
    }
}

impl LayoutElement for AnyElement {
    fn pos(&self) -> Option<Vec2d> {
        self.as_ref().pos()
    }

    fn size(&self) -> Option<Size> {
        self.as_ref().size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.as_ref().layer()
    }

    fn flex(&self) -> Option<f32> {
        self.as_ref().flex()
    }

    fn is_layout_stable(&self) -> bool {
        self.as_ref().is_layout_stable()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }

    fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().event_tree_bounds()
    }
}

impl Rebuildable for AnyElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref().rebuild_if_dirty(ctx)
    }

    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        self.as_ref().adopt_runtime_state_from(old)
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.as_ref().option_any()
    }

    fn is_stateful_element(&self) -> bool {
        self.as_ref().is_stateful_element()
    }

    fn is_carry_state(&self) -> bool {
        self.as_ref().is_carry_state()
    }

    fn compositor_priority(&self) -> bool {
        self.as_ref().compositor_priority()
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.as_ref().with_rebuild_context(ctx, callback)
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref().mark_needs_rebuild()
    }

    fn subtree_generation(&self) -> u64 {
        self.as_ref().subtree_generation()
    }

    fn set_subtree_generation(&self, generation: u64) {
        self.as_ref().set_subtree_generation(generation)
    }
}

impl EventElement for AnyElement {
    fn event_tree_role(&self) -> EventTreeRole {
        self.as_ref().event_tree_role()
    }

    fn focus_node(&self) -> Option<&FocusNode> {
        self.as_ref().focus_node()
    }

    fn autofocus(&self) -> bool {
        self.as_ref().autofocus()
    }

    fn traps_focus(&self) -> bool {
        self.as_ref().traps_focus()
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.as_ref().on_event(event)
    }

    fn on_event_with_context(
        &self,
        event: &ElementEvent,
        context: &mut EventDispatchContext<'_, '_>,
    ) -> EventResult {
        self.as_ref().on_event_with_context(event, context)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().event_children(visitor)
    }

    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().structural_children(visitor)
    }

    fn focus_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().focus_children(visitor)
    }

    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().hit_test_children(visitor)
    }

    fn hit_test_children_reversed<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().hit_test_children_reversed(visitor)
    }

    fn hit_test_children_at<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.as_ref().hit_test_children_at(pos, visitor)
    }

    fn hit_test_children_at_reversed<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.as_ref().hit_test_children_at_reversed(pos, visitor)
    }

    #[inline]
    fn has_overlapping_hit_targets(&self) -> bool {
        self.as_ref().has_overlapping_hit_targets()
    }

}

impl Drawable for AnyElement {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
    }

    #[inline]
    fn paint(&self, ctx: &BuildContext) {
        self.as_ref().paint(ctx)
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.as_ref().sync_paint_geometry(ctx)
    }

    #[inline]
    fn prepare_layout(&self, ctx: &BuildContext) -> bool {
        self.as_ref().prepare_layout(ctx)
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.as_ref().is_paint_stable()
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        self.as_ref().is_paint_bounded()
    }

    #[inline]
    fn compositor_animation(&self, ctx: &BuildContext) -> CompositorAnimationDecision {
        self.as_ref().compositor_animation(ctx)
    }

    #[inline]
    fn draw_with_compositor_animation(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref().draw_with_compositor_animation(ctx, frame)
    }

    #[inline]
    fn update_compositor_animation_damage(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref()
            .update_compositor_animation_damage(ctx, frame)
    }

    #[inline]
    fn draw_paint_islands(
        &self,
        retained_ctx: &BuildContext,
        live_ctx: &BuildContext,
        draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) -> bool {
        self.as_ref().draw_paint_islands(
            retained_ctx,
            live_ctx,
            draw_stable,
            draw_dynamic,
        )
    }
}

impl VisitorElement for Box<dyn Element> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().visit_children(visitor)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
    fn element_type_id(&self) -> std::any::TypeId {
        self.as_ref().element_type_id()
    }
    fn reconciliation_key(&self) -> Option<&Key> {
        self.as_ref().reconciliation_key()
    }
    fn element_id(&self) -> Option<ElementId> {
        self.as_ref().element_id()
    }
    fn set_element_id(&self, id: ElementId) {
        self.as_ref().set_element_id(id);
    }
}

impl LayoutElement for Box<dyn Element> {
    fn pos(&self) -> Option<Vec2d> {
        self.as_ref().pos()
    }
    fn size(&self) -> Option<Size> {
        self.as_ref().size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().content_size(ctx)
    }
    fn layer(&self) -> u32 {
        self.as_ref().layer()
    }

    fn flex(&self) -> Option<f32> {
        self.as_ref().flex()
    }

    fn is_layout_stable(&self) -> bool {
        self.as_ref().is_layout_stable()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }

    fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().event_tree_bounds()
    }
}

impl Rebuildable for Box<dyn Element> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref().rebuild_if_dirty(ctx)
    }

    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        self.as_ref().adopt_runtime_state_from(old)
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.as_ref().option_any()
    }

    fn is_stateful_element(&self) -> bool {
        self.as_ref().is_stateful_element()
    }

    fn is_carry_state(&self) -> bool {
        self.as_ref().is_carry_state()
    }

    fn compositor_priority(&self) -> bool {
        self.as_ref().compositor_priority()
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.as_ref().with_rebuild_context(ctx, callback)
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref().mark_needs_rebuild()
    }

    fn subtree_generation(&self) -> u64 {
        self.as_ref().subtree_generation()
    }

    fn set_subtree_generation(&self, generation: u64) {
        self.as_ref().set_subtree_generation(generation)
    }
}

impl EventElement for Box<dyn Element> {
    fn event_tree_role(&self) -> EventTreeRole {
        self.as_ref().event_tree_role()
    }

    fn focus_node(&self) -> Option<&FocusNode> {
        self.as_ref().focus_node()
    }

    fn autofocus(&self) -> bool {
        self.as_ref().autofocus()
    }

    fn traps_focus(&self) -> bool {
        self.as_ref().traps_focus()
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.as_ref().on_event(event)
    }

    fn on_event_with_context(
        &self,
        event: &ElementEvent,
        context: &mut EventDispatchContext<'_, '_>,
    ) -> EventResult {
        self.as_ref().on_event_with_context(event, context)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().event_children(visitor)
    }

    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().structural_children(visitor)
    }

    fn focus_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().focus_children(visitor)
    }

    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().hit_test_children(visitor)
    }

    fn hit_test_children_reversed<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().hit_test_children_reversed(visitor)
    }

    fn hit_test_children_at<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.as_ref().hit_test_children_at(pos, visitor)
    }

    fn hit_test_children_at_reversed<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.as_ref().hit_test_children_at_reversed(pos, visitor)
    }

    #[inline]
    fn has_overlapping_hit_targets(&self) -> bool {
        self.as_ref().has_overlapping_hit_targets()
    }

}

impl Drawable for Box<dyn Element> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
    }

    #[inline]
    fn paint(&self, ctx: &BuildContext) {
        self.as_ref().paint(ctx)
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.as_ref().sync_paint_geometry(ctx)
    }

    #[inline]
    fn prepare_layout(&self, ctx: &BuildContext) -> bool {
        self.as_ref().prepare_layout(ctx)
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.as_ref().is_paint_stable()
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        self.as_ref().is_paint_bounded()
    }

    #[inline]
    fn compositor_animation(&self, ctx: &BuildContext) -> CompositorAnimationDecision {
        self.as_ref().compositor_animation(ctx)
    }

    #[inline]
    fn draw_with_compositor_animation(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref().draw_with_compositor_animation(ctx, frame)
    }

    #[inline]
    fn update_compositor_animation_damage(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref()
            .update_compositor_animation_damage(ctx, frame)
    }

    #[inline]
    fn draw_paint_islands(
        &self,
        retained_ctx: &BuildContext,
        live_ctx: &BuildContext,
        draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) -> bool {
        self.as_ref().draw_paint_islands(
            retained_ctx,
            live_ctx,
            draw_stable,
            draw_dynamic,
        )
    }
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

/// A compact link from an indexed element to its structural parent.
///
/// The public name is retained for source compatibility, but the dispatcher no
/// longer stores a separately allocated root-relative slice for every element.
/// Links live in one reusable arena and are followed only when a captured owner
/// has to be resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElementPath {
    parent: Option<usize>,
    child_index: u32,
}

/// Provides a routed element with the dispatcher's shared pointer-capture
/// state while it forwards an event into a private child view.
///
/// The context is valid only for the duration of
/// [`EventElement::on_event_with_context`]. It is intentionally opaque: a
/// forwarding element may query capture state for its boundary and dispatch
/// its child, but it cannot retain the context after the current event.
pub struct EventDispatchContext<'dispatcher, 'tree> {
    dispatcher: &'dispatcher mut EventDispatcher,
    path_root: &'tree dyn Element,
    boundary: Option<ElementId>,
}

impl<'dispatcher, 'tree> EventDispatchContext<'dispatcher, 'tree> {
    #[inline]
    fn new(
        dispatcher: &'dispatcher mut EventDispatcher,
        path_root: &'tree dyn Element,
        boundary: Option<ElementId>,
    ) -> Self {
        Self {
            dispatcher,
            path_root,
            boundary,
        }
    }

    /// Returns whether a pointer is captured by this forwarding boundary.
    #[inline]
    pub fn is_captured(&self, pointer: PointerKey) -> bool {
        self.boundary.is_some_and(|boundary| {
            self.dispatcher
                .nested_captures
                .contains_key(&(boundary, pointer))
        })
    }

    /// Dispatches an event into the forwarding element's child view using the
    /// same path index and capture state as the owning dispatcher.
    #[inline]
    pub fn dispatch_child(
        &mut self,
        child: &dyn Element,
        pos: Vec2d,
        event: &ElementEvent,
    ) -> EventResult {
        self.dispatcher
            .dispatch_nested(self.path_root, self.boundary, child, pos, event)
    }
}

/// One retained element in the last uncaptured pointer hit chain.
///
/// The pointer is valid only while the dispatch root has the same subtree
/// generation and address as the cache entry. `EventDispatcher` is UI-thread
/// state, and the cache is retired before either condition can be reused.
#[derive(Clone, Copy)]
struct CachedHitElement {
    id: ElementId,
    element: *const (dyn Element + 'static),
}

struct CachedHitChain {
    pointer: PointerKey,
    root: ElementId,
    generation: u64,
    last_position: Vec2d,
    elements: SmallVec<[CachedHitElement; 16]>,
}

/// Retains an element pointer for a same-generation dispatch side path.
///
/// The caller must discard the pointer before the dispatch root changes its
/// identity or subtree generation. That is the same lifetime invariant used by
/// [`CachedHitElement`].
#[inline]
fn retained_element_pointer(element: &dyn Element) -> *const (dyn Element + 'static) {
    // SAFETY: callers keep this pointer only while the retained element tree
    // remains at the same root and subtree generation.
    unsafe { std::mem::transmute::<&dyn Element, *const (dyn Element + 'static)>(element) }
}

struct HoverHitChain {
    pointer: PointerKey,
    root: ElementId,
    generation: u64,
    elements: SmallVec<[ElementId; 16]>,
}

struct HitChainRecorder {
    elements: SmallVec<[CachedHitElement; 16]>,
    hover_elements: SmallVec<[ElementId; 16]>,
    empty_hit_test_nodes: SmallVec<[CachedHitElement; 8]>,
    cacheable: bool,
    forwarding_boundary: bool,
}

impl HitChainRecorder {
    #[inline]
    fn new() -> Self {
        Self {
            elements: SmallVec::new(),
            hover_elements: SmallVec::new(),
            empty_hit_test_nodes: SmallVec::new(),
            cacheable: true,
            forwarding_boundary: false,
        }
    }

    #[inline]
    fn record_element(&mut self, element: &dyn Element) {
        if let Some(id) = element.element_id() {
            self.hover_elements.push(id);
        }
        if !self.cacheable || self.forwarding_boundary {
            return;
        }
        #[cfg(feature = "event-tree-exp")]
        if element.event_tree_role() == EventTreeRole::IndexedHitTestBoundary {
            // A boundary can change its eligible child branches without any
            // target bounds moving. Replaying a single cached chain would skip
            // its position-aware child selection on the next pointer move.
            self.cacheable = false;
            self.elements.clear();
            return;
        }
        let Some(id) = element.element_id() else {
            self.cacheable = false;
            self.elements.clear();
            return;
        };
        if element.has_overlapping_hit_targets() {
            self.cacheable = false;
            self.elements.clear();
            return;
        }
        self.elements.push(CachedHitElement {
            id,
            // The pointer is retained only behind the generation/root checks
            // in `dispatch_cached_hit_chain`.
            element: retained_element_pointer(element),
        });
    }

    #[inline]
    fn record_hit_test_children(&mut self, count: usize) {
        if self.forwarding_boundary || count <= 1 {
            return;
        }
        self.cacheable = false;
        self.elements.clear();
    }

    #[inline]
    fn record_empty_hit_test_node(&mut self) {
        if !self.cacheable
            || self.forwarding_boundary
            || self.elements.is_empty()
        {
            return;
        }
        if let Some(element) = self.elements.last().copied() {
            self.empty_hit_test_nodes.push(element);
        }
    }

    #[inline]
    fn record_miss(&mut self) {
        if self.forwarding_boundary {
            return;
        }
        self.cacheable = false;
        self.elements.clear();
    }

    #[inline]
    fn mark_forwarding_boundary(&mut self) {
        if self.cacheable && !self.elements.is_empty() {
            self.forwarding_boundary = true;
        }
    }

    #[inline]
    fn finish_hover(
        &self,
        pointer: PointerKey,
        root: &dyn Element,
        generation: u64,
    ) -> Option<HoverHitChain> {
        Some(HoverHitChain {
            pointer,
            root: root.element_id()?,
            generation,
            elements: self.hover_elements.clone(),
        })
    }

    #[inline]
    fn finish(
        self,
        pointer: PointerKey,
        root: &dyn Element,
        generation: u64,
        position: Vec2d,
    ) -> Option<CachedHitChain> {
        let root_id = root.element_id()?;
        let first = self.elements.first()?;
        let empty_node_has_children = !self.forwarding_boundary
            && self.empty_hit_test_nodes.iter().any(|entry| {
                // SAFETY: the same root-generation invariant that protects
                // replay still holds while the just-completed route is being
                // recorded. These entries point into that retained tree.
                let element = unsafe { &*entry.element };
                let mut has_child = false;
                element.structural_children(&mut |_| has_child = true);
                has_child
        });
        (self.cacheable
            && !empty_node_has_children
            && first.id == root_id)
            .then_some(CachedHitChain {
                pointer,
                root: root_id,
                generation,
                last_position: position,
                elements: self.elements,
            })
    }
}

#[cfg(feature = "event-tree-exp")]
#[derive(Default)]
struct EventHitFrame {
    marks: Vec<u32>,
    boundary_filtered: Vec<u32>,
    epoch: u32,
    roots: Vec<EventTargetId>,
    active_children: Vec<Vec<EventTargetId>>,
    touched_parents: Vec<usize>,
}

#[cfg(feature = "event-tree-exp")]
impl EventHitFrame {
    fn prepare(&mut self, target_count: usize) -> u32 {
        for parent in self.touched_parents.drain(..) {
            if let Some(children) = self.active_children.get_mut(parent) {
                children.clear();
            }
        }
        self.roots.clear();
        self.marks.resize(target_count, 0);
        self.boundary_filtered.resize(target_count, 0);
        self.active_children.resize_with(target_count, Vec::new);

        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.marks.fill(0);
            self.boundary_filtered.fill(0);
            self.epoch = 1;
        }
        self.epoch
    }

    fn mark_hit_path(&mut self, tree: &EventTree, target: EventTargetId, epoch: u32) {
        let mut current = Some(target);
        while let Some(target) = current {
            let index = target.index();
            if self.marks[index] == epoch {
                break;
            }
            self.marks[index] = epoch;
            let parent = tree.get(target).and_then(|element| element.parent());
            if let Some(parent) = parent {
                let children = &mut self.active_children[parent.index()];
                if children.is_empty() {
                    self.touched_parents.push(parent.index());
                }
                children.push(target);
                current = Some(parent);
            } else {
                self.roots.push(target);
                current = None;
            }
        }
    }

    fn mark_boundary_filtered(&mut self, target: EventTargetId, epoch: u32) {
        self.boundary_filtered[target.index()] = epoch;
    }

    fn boundary_was_filtered(&self, target: EventTargetId, epoch: u32) -> bool {
        self.boundary_filtered.get(target.index()).copied() == Some(epoch)
    }
}

#[cfg(feature = "event-tree-exp")]
struct EventHitDepthGuard {
    depth: Rc<Cell<usize>>,
    previous: usize,
}

#[cfg(feature = "event-tree-exp")]
impl EventHitDepthGuard {
    fn enter(depth: Rc<Cell<usize>>) -> Self {
        let previous = depth.get();
        depth.set(
            previous
                .checked_add(1)
                .expect("event hit-query nesting overflow"),
        );
        Self { depth, previous }
    }
}

#[cfg(feature = "event-tree-exp")]
impl Drop for EventHitDepthGuard {
    fn drop(&mut self) {
        self.depth.set(self.previous);
    }
}

/// Routes pointer events and persists capture ownership across event calls.
///
/// Capture lookup is an average `O(1)` hash-map operation. The saved path is
/// then resolved from the current root, avoiding a full-tree capture scan.
/// Uncaptured, non-consuming pointer moves additionally replay the last
/// single hit chain after validating its element bounds and subtree
/// generation; overlapping containers and forwarding boundaries retain their
/// conservative full-walk behavior where necessary.
pub struct EventDispatcher {
    captures: HashMap<PointerKey, ElementId>,
    nested_captures: HashMap<(ElementId, PointerKey), ElementId>,
    path_indices: HashMap<ElementId, usize>,
    path_links: Vec<ElementPath>,
    indexed_subtree_generation: u64,
    indexed_root: Option<ElementId>,
    paths_dirty: bool,
    generation_checked_frame: Option<u64>,
    hit_chain_cache: Option<CachedHitChain>,
    hit_chain_recorder: Option<HitChainRecorder>,
    hover_chains: HashMap<PointerKey, HoverHitChain>,
    hover_membership_marks: Vec<u32>,
    hover_membership_epoch: u32,
    focus_scope: Option<ElementId>,
    focus: FocusManager<ElementId>,
    focus_candidates: FocusCandidates<ElementId>,
    #[cfg(feature = "event-tree-exp")]
    event_tree: EventTree,
    #[cfg(feature = "event-tree-exp")]
    event_target_by_element: HashMap<ElementId, EventTargetId>,
    #[cfg(feature = "event-tree-exp")]
    event_target_elements: HashMap<ElementId, *const (dyn Element + 'static)>,
    #[cfg(feature = "event-tree-exp")]
    indexed_root_address: Option<*const ()>,
    #[cfg(feature = "event-tree-exp")]
    event_tree_eligible: bool,
    #[cfg(feature = "event-tree-exp")]
    event_hit_frames: Vec<EventHitFrame>,
    #[cfg(feature = "event-tree-exp")]
    event_hit_depth: Rc<Cell<usize>>,
    #[cfg(feature = "event-tree-exp")]
    indexed_layout_generation: u64,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl EventDispatcher {
    /// Creates an empty dispatcher with no captured pointers.
    #[inline]
    pub fn new() -> Self {
        Self {
            captures: HashMap::new(),
            nested_captures: HashMap::new(),
            path_indices: HashMap::new(),
            path_links: Vec::new(),
            indexed_subtree_generation: u64::MAX,
            indexed_root: None,
            paths_dirty: true,
            generation_checked_frame: None,
            hit_chain_cache: None,
            hit_chain_recorder: None,
            hover_chains: HashMap::new(),
            hover_membership_marks: Vec::new(),
            hover_membership_epoch: 0,
            focus_scope: None,
            focus: FocusManager::new(),
            focus_candidates: FocusCandidates::new(),
            #[cfg(feature = "event-tree-exp")]
            event_tree: EventTree::new(),
            #[cfg(feature = "event-tree-exp")]
            event_target_by_element: HashMap::new(),
            #[cfg(feature = "event-tree-exp")]
            event_target_elements: HashMap::new(),
            #[cfg(feature = "event-tree-exp")]
            indexed_root_address: None,
            #[cfg(feature = "event-tree-exp")]
            event_tree_eligible: false,
            #[cfg(feature = "event-tree-exp")]
            event_hit_frames: Vec::new(),
            #[cfg(feature = "event-tree-exp")]
            event_hit_depth: Rc::new(Cell::new(0)),
            #[cfg(feature = "event-tree-exp")]
            indexed_layout_generation: u64::MAX,
        }
    }

    #[inline]
    fn invalidate_hit_chain(&mut self) {
        self.hit_chain_cache = None;
        self.hit_chain_recorder = None;
    }

    #[inline]
    fn begin_hit_chain_recording(&mut self) {
        self.hit_chain_recorder = Some(HitChainRecorder::new());
    }

    #[inline]
    fn record_hit_chain_element(&mut self, element: &dyn Element) {
        if let Some(recorder) = self.hit_chain_recorder.as_mut() {
            recorder.record_element(element);
        }
    }

    #[inline]
    fn record_hit_chain_children(&mut self, count: usize) {
        if let Some(recorder) = self.hit_chain_recorder.as_mut() {
            recorder.record_hit_test_children(count);
        }
    }

    #[inline]
    fn record_empty_hit_chain_node(&mut self) {
        if let Some(recorder) = self.hit_chain_recorder.as_mut() {
            recorder.record_empty_hit_test_node();
        }
    }

    #[inline]
    fn record_hit_chain_miss(&mut self) {
        if let Some(recorder) = self.hit_chain_recorder.as_mut() {
            recorder.record_miss();
        }
    }

    #[inline]
    fn mark_hit_chain_forwarding_boundary(&mut self) {
        if let Some(recorder) = self.hit_chain_recorder.as_mut() {
            recorder.mark_forwarding_boundary();
        }
    }

    fn finish_hit_chain_recording(
        &mut self,
        root: &dyn Element,
        pos: Vec2d,
        pointer: PointerKey,
        outcome: &RoutedEventResult,
    ) -> Option<HoverHitChain> {
        let generation = root.subtree_generation();
        let root_id = root.element_id();
        let recorder = self.hit_chain_recorder.take()?;
        let hover_chain = recorder.finish_hover(pointer, root, generation);
        self.hit_chain_cache = if outcome.result.is_consumed()
            || !matches!(outcome.result.capture_request(), CaptureRequest::None)
            || !matches!(outcome.result.follow_up(), FollowUp::None)
            || root_id != self.indexed_root
            || generation != self.indexed_subtree_generation
        {
            None
        } else {
            recorder.finish(pointer, root, generation, pos)
        };
        hover_chain
    }

    fn dispatch_uncaptured_pointer_move(
        &mut self,
        root: &dyn Element,
        pos: Vec2d,
        event: &ElementEvent,
        pointer: PointerKey,
    ) -> RoutedEventResult {
        let previous_hover = self.hover_chains.remove(&pointer);
        if let Some((outcome, current_hover)) =
            self.dispatch_cached_hit_chain(root, pos, event, pointer)
        {
            return self.finish_hover_transition(
                root,
                pointer,
                previous_hover,
                current_hover,
                outcome,
            );
        }

        self.begin_hit_chain_recording();
        let outcome = dispatch_routed_event(self, root, pos, event);
        let Some(current_hover) = self
            .finish_hit_chain_recording(root, pos, pointer, &outcome)
        else {
            return outcome;
        };
        self.finish_hover_transition(
            root,
            pointer,
            previous_hover,
            current_hover,
            outcome,
        )
    }

    fn finish_hover_transition(
        &mut self,
        root: &dyn Element,
        pointer: PointerKey,
        previous: Option<HoverHitChain>,
        current: HoverHitChain,
        mut outcome: RoutedEventResult,
    ) -> RoutedEventResult {
        let previous = previous.filter(|previous| {
            previous.pointer == pointer
                && previous.generation == current.generation
                && previous.root == current.root
        });
        let exit = ElementEvent::PointerExited(pointer.source, pointer.id);
        if let Some(previous) = previous {
            let current_mark = self.mark_hover_membership(&current.elements);
            for previous_element in previous.elements.iter().rev() {
                if self.is_current_hover_element(*previous_element, current_mark) {
                    continue;
                }

                let Some(element) = self.resolve_owner(root, *previous_element) else {
                    continue;
                };
                if event_callback_enabled(element) {
                    outcome.result = outcome.result.merge(element.on_event(&exit));
                }
            }
        }
        self.hover_chains.insert(pointer, current);
        outcome
    }

    /// Marks the current hover chain in reusable path-indexed scratch storage.
    ///
    /// Every element that can appear in a routed hover chain is present in the
    /// structural path index. A mark epoch avoids clearing the whole vector on
    /// every pointer move, and the vector itself retains capacity across moves
    /// and tree generations.
    #[inline]
    fn mark_hover_membership(&mut self, elements: &[ElementId]) -> u32 {
        if self.hover_membership_marks.len() < self.path_links.len() {
            self.hover_membership_marks.resize(self.path_links.len(), 0);
        }

        let next_epoch = self.hover_membership_epoch.wrapping_add(1);
        if next_epoch == 0 {
            self.hover_membership_marks.fill(0);
            self.hover_membership_epoch = 1;
        } else {
            self.hover_membership_epoch = next_epoch;
        }
        let mark = self.hover_membership_epoch;

        for &element in elements {
            let Some(path_index) = self.path_indices.get(&element).copied() else {
                continue;
            };
            if let Some(slot) = self.hover_membership_marks.get_mut(path_index) {
                *slot = mark;
            }
        }
        mark
    }

    #[inline]
    fn is_current_hover_element(&self, element: ElementId, mark: u32) -> bool {
        record_hover_membership_check();
        let Some(path_index) = self.path_indices.get(&element).copied() else {
            return false;
        };
        self.hover_membership_marks.get(path_index).copied() == Some(mark)
    }

    fn dispatch_cached_hit_chain(
        &mut self,
        root: &dyn Element,
        pos: Vec2d,
        event: &ElementEvent,
        pointer: PointerKey,
    ) -> Option<(RoutedEventResult, HoverHitChain)> {
        let mut cached = self.hit_chain_cache.take()?;
        let Some(first) = cached.elements.first() else {
            return None;
        };
        let generation = root.subtree_generation();
        let valid = cached.pointer == pointer
            && cached.root == root.element_id()?
            && cached.generation == generation
            && cached.generation == self.indexed_subtree_generation
            && self.indexed_root == Some(cached.root)
            && std::ptr::addr_eq(root as *const dyn Element, first.element);
        if !valid {
            return None;
        }

        for entry in &cached.elements {
            // SAFETY: the cache is used only when the root identity and its
            // subtree generation are unchanged. Reconciliation retires the
            // cache before an element can be replaced or moved, and the UI
            // tree is not concurrently mutated during dispatch.
            let element = unsafe { &*entry.element };
            if element.element_id() != Some(entry.id)
                || !contains(element, cached.last_position)
                || !contains(element, pos)
            {
                return None;
            }
        }

        let outcome = dispatch_cached_hit_chain_inner(self, root, &cached.elements, 0, pos, event);
        let hover_chain = HoverHitChain {
            pointer,
            root: cached.root,
            generation,
            elements: cached
                .elements
                .iter()
                .map(|element| element.id)
                .collect(),
        };
        if !outcome.result.is_consumed()
            && matches!(outcome.result.capture_request(), CaptureRequest::None)
            && matches!(outcome.result.follow_up(), FollowUp::None)
        {
            cached.last_position = pos;
            self.hit_chain_cache = Some(cached);
        }
        Some((outcome, hover_chain))
    }

    /// Places the focus of this dispatcher inside `trap`.
    ///
    /// A dispatch root that is presented *over* the application — a modal, whose
    /// content is not part of the tree it covers — belongs to the region its
    /// [`FocusTrap`](crate::focus::FocusTrap) confines. Focus is then granted
    /// here only while that region is the innermost trapping one, so the
    /// application underneath owns nothing while the overlay is up and gets its
    /// owner back when the trap is released.
    #[inline]
    pub fn with_focus_trap(mut self, trap: FocusTrapId) -> Self {
        self.focus.set_trap(Some(trap));
        self
    }

    /// Returns the region this dispatcher grants focus in, if it is confined.
    #[inline]
    pub fn focus_trap(&self) -> Option<FocusTrapId> {
        self.focus.trap()
    }

    /// Returns the element that currently owns keyboard focus, if any.
    #[inline]
    pub fn focused(&self) -> Option<ElementId> {
        self.focus.focused()
    }

    /// Returns the element whose subtree keyboard focus is confined to, if any.
    ///
    /// This is the innermost element of the last indexed tree that reported
    /// [`EventElement::traps_focus`].
    #[inline]
    pub fn focus_scope(&self) -> Option<ElementId> {
        self.focus_scope
    }

    /// Dispatches one event using persistent capture state.
    ///
    /// Uncaptured events use normal hit testing. A captured move, exit, or up
    /// resolves only the saved root-to-owner path. Pointer-up releases its
    /// capture after delivery, and cancellation is delivered once to every
    /// distinct captured owner before all captures are cleared.
    ///
    /// Focus-directed events — see [`ElementEvent::is_focus_directed`] — skip
    /// hit testing entirely and are offered to the whole tree until an element
    /// that owns keyboard focus consumes them, so text and composition reach the
    /// focused field no matter where the pointer is.
    ///
    /// Delivery is also where a [pointer claim](crate::pointer_claim) expires: a
    /// claim describes a gesture in progress, so it cannot outlive the pointer
    /// that made it. Whatever a descendant forgot to give back is released once
    /// the pointer goes up, and cancellation drops every claim at once — without
    /// which a single missed release would leave an enclosing scrollable
    /// standing down forever.
    pub fn dispatch(
        &mut self,
        root: &dyn Element,
        pos: Vec2d,
        event: &ElementEvent,
    ) -> EventResult {
        let result = self.route(root, pos, event);
        match event {
            ElementEvent::PointerUp(pointer) => {
                pointer_claim::release_pointer(PointerKey::new(pointer.source, pointer.id));
            }
            ElementEvent::PointerExited(source, id) => {
                self.hover_chains
                    .remove(&PointerKey::new(*source, *id));
            }
            ElementEvent::Cancel => {
                pointer_claim::release_all_pointers();
            }
            _ => {}
        }
        result.merge(self.settle_focus_requests(root))
    }

    /// Dispatches a forwarding element's private child view with this
    /// dispatcher's path index and capture state.
    ///
    /// A forwarding boundary remains the externally captured owner, while the
    /// nested owner is retained in [`Self::nested_captures`]. This preserves
    /// hover wrappers' ability to observe an out-of-bounds release without
    /// giving every wrapper its own dispatcher and path map.
    fn dispatch_nested(
        &mut self,
        path_root: &dyn Element,
        boundary: Option<ElementId>,
        root: &dyn Element,
        pos: Vec2d,
        event: &ElementEvent,
    ) -> EventResult {
        let pointer = event_pointer_key(event);
        self.mark_hit_chain_forwarding_boundary();
        let captured_owner = boundary.and_then(|boundary| {
            pointer.and_then(|pointer| {
                self.nested_captures
                    .get(&(boundary, pointer))
                    .copied()
            })
        });
        let was_captured = captured_owner.is_some();

        let outcome = if matches!(event, ElementEvent::Cancel) {
            if let Some(boundary) = boundary {
                self.cancel_nested_captures(path_root, boundary, event)
            } else {
                let mut children = EventChildren::new();
                dispatch_routed_event_inner(
                    self,
                    path_root,
                    root,
                    pos,
                    event,
                    &mut children,
                )
            }
        } else if let Some(owner) = captured_owner {
            match self.dispatch_nested_captured(path_root, owner, event) {
                Some(outcome) => outcome,
                None => {
                    if let (Some(boundary), Some(pointer)) = (boundary, pointer) {
                        self.nested_captures.remove(&(boundary, pointer));
                    }
                    RoutedEventResult {
                        result: EventResult::ignored(),
                        capture_owner: None,
                        focus_owner: None,
                    }
                }
            }
        } else {
            let mut children = EventChildren::new();
            dispatch_routed_event_inner(self, path_root, root, pos, event, &mut children)
        };

        if let Some(boundary) = boundary {
            match outcome.result.capture_request() {
                CaptureRequest::Capture(pointer) => {
                    if let Some(owner) = outcome.capture_owner {
                        self.nested_captures.insert((boundary, pointer), owner);
                    }
                }
                CaptureRequest::Release(pointer) => {
                    self.nested_captures.remove(&(boundary, pointer));
                }
                CaptureRequest::None => {}
            }
            if was_captured && matches!(event, ElementEvent::PointerUp(_))
                && let Some(pointer) = pointer {
                    self.nested_captures.remove(&(boundary, pointer));
                }
            if matches!(event, ElementEvent::Cancel) {
                self.nested_captures
                    .retain(|(captured_boundary, _), _| *captured_boundary != boundary);
            }
        }

        outcome.result.without_capture_request()
    }

    fn dispatch_nested_captured(
        &mut self,
        path_root: &dyn Element,
        owner: ElementId,
        event: &ElementEvent,
    ) -> Option<RoutedEventResult> {
        let target = resolve_element_path(path_root, owner, &self.path_indices, &self.path_links)?;
        if target.element_id() != Some(owner) {
            return None;
        }

        let result = {
            if event_callback_enabled(target) {
                let mut context = EventDispatchContext::new(self, path_root, Some(owner));
                target.on_event_with_context(event, &mut context)
            } else {
                EventResult::ignored()
            }
        };
        let capture_owner = (!matches!(result.capture_request(), CaptureRequest::None))
            .then_some(owner);
        Some(RoutedEventResult {
            result,
            capture_owner,
            focus_owner: None,
        })
    }

    fn cancel_nested_captures(
        &mut self,
        path_root: &dyn Element,
        boundary: ElementId,
        event: &ElementEvent,
    ) -> RoutedEventResult {
        let owners: HashSet<ElementId> = self
            .nested_captures
            .iter()
            .filter_map(|((captured_boundary, _), owner)| {
                (*captured_boundary == boundary).then_some(*owner)
            })
            .collect();
        let mut result = EventResult::ignored();
        for owner in owners {
            if let Some(outcome) = self.dispatch_nested_captured(path_root, owner, event) {
                result = result.merge(outcome.result);
            }
        }
        self.nested_captures
            .retain(|(captured_boundary, _), _| *captured_boundary != boundary);
        RoutedEventResult {
            result,
            capture_owner: None,
            focus_owner: None,
        }
    }

    /// Grants the focus a handler asked for while this event was delivered.
    ///
    /// Focus is resolved once, before the event is routed, so a handler that
    /// calls [`FocusNode::request_focus`] — a button focusing the field it
    /// belongs to — records its wish after the only pass that would have read
    /// it. Left there, the wish would wait for whatever input happens next: a
    /// mouse hides that behind the pixel it moves after a click, but a finger
    /// that taps and lifts sends nothing more, so the field would be focused by
    /// the *next* tap.
    ///
    /// Nothing is walked for an event that asked for nothing:
    /// [`FocusManager::begin_synchronization`] compares the request counter it
    /// recorded moments ago, so the common case is a handful of comparisons.
    #[inline]
    fn settle_focus_requests(&mut self, root: &dyn Element) -> EventResult {
        self.synchronize_paths(root);
        self.synchronize_focus(root)
    }

    /// Routes one event, leaving pointer-claim housekeeping to
    /// [`Self::dispatch`].
    fn route(&mut self, root: &dyn Element, pos: Vec2d, event: &ElementEvent) -> EventResult {
        let pointer = event_pointer_key(event);
        let routes_to_capture = matches!(
            event,
            ElementEvent::PointerMove(_)
                | ElementEvent::PointerUp(_)
                | ElementEvent::PointerExited(_, _)
        );
        let was_captured = routes_to_capture
            && pointer.is_some_and(|pointer| self.captures.contains_key(&pointer));
        let uncaptured_pointer_move = matches!(event, ElementEvent::PointerMove(_))
            && !was_captured
            && pointer.is_some();
        if !uncaptured_pointer_move {
            self.invalidate_hit_chain();
        }

        self.synchronize_paths(root);
        let focus_result = self.synchronize_focus(root);

        if let ElementEvent::KeyInput {
            key: NamedKey::Tab,
            action: KeyAction::Pressed | KeyAction::Repeat,
            modifiers,
        } = event
            && let Some(traversal_result) = self.traverse_focus(root, modifiers.shift)
        {
            return focus_result
                .merge(traversal_result)
                .merge(EventResult::consumed());
        }

        if matches!(event, ElementEvent::Cancel) {
            return focus_result.merge(
                self.cancel_captures(root, event)
                    .without_capture_request(),
            );
        }

        if event.is_focus_directed() {
            let focused_result = self.dispatch_to_focused(root, event);
            if focused_result.is_consumed() || !self.focus.is_suspended() {
                return focus_result.merge(focused_result);
            }

            // Focus is trapped elsewhere, so this tree owns none of it. The
            // trapping region is presented by an element of this tree — an
            // overlay host dispatching into its own root — so the event is
            // routed to reach it, which is the only way typed text arrives at
            // the field inside a modal.
            let outcome = dispatch_routed_event(self, root, pos, event);
            return focus_result
                .merge(focused_result)
                .merge(outcome.result.without_capture_request().without_follow_up());
        }

        if matches!(event, ElementEvent::KeyInput { .. }) {
            let focused_result = self.dispatch_to_focused(root, event);
            if focused_result.is_consumed() {
                return focus_result.merge(focused_result);
            }

            let outcome = dispatch_routed_event(self, root, pos, event);
            return focus_result
                .merge(focused_result)
                .merge(outcome.result.without_capture_request().without_follow_up());
        }

        if routes_to_capture
            && was_captured
            && let Some(pointer) = pointer
        {
            let result = self.dispatch_captured(root, pointer, event);
            return focus_result.merge(self.run_follow_up(root, pos, pointer, result));
        }

        let outcome = if let Some(pointer) = pointer.filter(|_| uncaptured_pointer_move) {
            self.dispatch_uncaptured_pointer_move(root, pos, event, pointer)
        } else {
            dispatch_routed_event(self, root, pos, event)
        };
        self.apply_capture_request(outcome.result.capture_request(), outcome.capture_owner);
        let pointer_focus_result = if matches!(event, ElementEvent::PointerDown(_))
            && self.press_may_move_focus(root, outcome.focus_owner.as_ref())
        {
            self.transition_focus(root, outcome.focus_owner.clone())
        } else {
            EventResult::ignored()
        };
        let result = outcome.result.without_capture_request();
        match pointer {
            Some(pointer) => {
                focus_result
                    .merge(pointer_focus_result)
                    .merge(self.run_follow_up(root, pos, pointer, result))
            }
            None => focus_result
                .merge(pointer_focus_result)
                .merge(result.without_follow_up()),
        }
    }

    /// Runs the extra routed pass a handler asked for, if it asked for one.
    ///
    /// This is the whole of drag routing: the element carrying the drag owns the
    /// pointer and therefore hears about it alone, so it asks for one more
    /// ordinary hit-tested dispatch at the position already in hand, and the
    /// topmost element under the pointer receives the drag event. Nothing
    /// happens — no traversal, no allocation — unless a handler asked, so an
    /// application that never drags pays only the cost of reading one field.
    ///
    /// A [`FollowUp::DragDrop`] ends the drag, so the capture that asked for it
    /// is released once the drop has been delivered.
    fn run_follow_up(
        &mut self,
        root: &dyn Element,
        pos: Vec2d,
        pointer: PointerKey,
        result: EventResult,
    ) -> EventResult {
        let follow_up = result.follow_up();
        let follow_up_event = match follow_up {
            FollowUp::None => return result,
            FollowUp::DragOver => ElementEvent::DragOver {
                pos,
                source: pointer.source,
                id: pointer.id,
            },
            FollowUp::DragDrop => ElementEvent::DragDrop {
                pos,
                source: pointer.source,
                id: pointer.id,
            },
        };

        let outcome = dispatch_routed_event(self, root, pos, &follow_up_event);
        if matches!(follow_up, FollowUp::DragDrop) {
            self.captures.remove(&pointer);
        }

        result
            .merge(outcome.result)
            .without_capture_request()
            .without_follow_up()
    }

    /// Returns the element currently owning `pointer`, if any.
    #[inline]
    pub fn captured_owner(&self, pointer: PointerKey) -> Option<ElementId> {
        self.captures.get(&pointer).copied()
    }

    /// Returns the number of independently captured pointers.
    #[inline]
    pub fn capture_count(&self) -> usize {
        self.captures.len()
    }

    /// Returns whether `pointer` currently has a live capture entry.
    #[inline]
    pub fn is_captured(&self, pointer: PointerKey) -> bool {
        self.captures.contains_key(&pointer)
    }

    /// Clears all capture entries without delivering cancellation.
    ///
    /// Use this only after the owning boundary has already broadcast a single
    /// cancellation event to its subtree.
    #[inline]
    pub fn clear_captures(&mut self) {
        self.captures.clear();
        self.nested_captures.clear();
    }

    /// Delivers cancellation to the owner of `pointer` and releases it.
    ///
    /// This is primarily used by nested routing boundaries when an ancestor
    /// wins gesture arbitration after a descendant initially captured.
    pub fn cancel_pointer(&mut self, root: &dyn Element, pointer: PointerKey) -> EventResult {
        self.synchronize_paths(root);
        let Some(owner) = self.captures.remove(&pointer) else {
            return EventResult::ignored();
        };
        let Some(target) =
            resolve_element_path(root, owner, &self.path_indices, &self.path_links)
        else {
            return EventResult::ignored();
        };
        if target.element_id() != Some(owner) {
            return EventResult::ignored();
        }
        if event_callback_enabled(target) {
            let mut context = EventDispatchContext::new(self, root, Some(owner));
            target
                .on_event_with_context(&ElementEvent::Cancel, &mut context)
                .without_capture_request()
        } else {
            EventResult::ignored()
        }
    }

    fn synchronize_paths(&mut self, root: &dyn Element) {
        let root_id = root.element_id();
        #[cfg(feature = "event-tree-exp")]
        let root_address = root as *const dyn Element as *const ();
        #[cfg(feature = "event-tree-exp")]
        let root_address_changed = self.indexed_root_address != Some(root_address);
        #[cfg(feature = "event-tree-exp")]
        let layout_generation = layout_invalidation_generation();
        #[cfg(feature = "event-tree-exp")]
        let event_layout_changed = self.indexed_layout_generation != layout_generation;
        let generation = match current_event_frame() {
            Some(frame) => {
                if self.generation_checked_frame != Some(frame) {
                    self.generation_checked_frame = Some(frame);
                    let generation = root.subtree_generation();
                    self.paths_dirty = self.indexed_subtree_generation != generation
                        || self.indexed_root != root_id;
                    Some(generation)
                } else if self.indexed_root != root_id {
                    self.paths_dirty = true;
                    Some(root.subtree_generation())
                } else {
                    None
                }
            }
            None => {
                let generation = root.subtree_generation();
                self.paths_dirty = self.indexed_subtree_generation != generation
                    || self.indexed_root != root_id;
                Some(generation)
            }
        };

        #[cfg(feature = "event-tree-exp")]
        if root_address_changed {
            self.paths_dirty = true;
        }

        #[cfg(feature = "event-tree-exp")]
        let rebuild_paths = self.paths_dirty;
        #[cfg(feature = "event-tree-exp")]
        if !rebuild_paths && !event_layout_changed {
            return;
        }
        #[cfg(not(feature = "event-tree-exp"))]
        if !self.paths_dirty {
            return;
        }

        let generation = generation.unwrap_or_else(|| root.subtree_generation());

        #[cfg(feature = "event-tree-exp")]
        if event_layout_changed {
            self.invalidate_hit_chain();
            self.hover_chains.clear();
        }

        if self.paths_dirty {
            self.invalidate_hit_chain();
            self.hover_chains.clear();
            self.path_indices.clear();
            self.path_links.clear();
            self.focus_scope = None;
            index_element_links(
                root,
                None,
                0,
                &mut self.path_links,
                &mut self.path_indices,
                &mut self.focus_scope,
            );
            self.captures
                .retain(|_, owner| self.path_indices.contains_key(owner));
            let path_indices = &self.path_indices;
            self.nested_captures.retain(|(boundary, _), owner| {
                path_indices.contains_key(boundary) && path_indices.contains_key(owner)
            });
            self.focus
                .retain_history(|owner| path_indices.contains_key(owner));
            self.indexed_subtree_generation = generation;
            self.indexed_root = root_id;
            self.paths_dirty = false;
        }
        #[cfg(feature = "event-tree-exp")]
        if rebuild_paths {
            self.event_tree = EventTree::new();
            self.event_target_by_element.clear();
            self.event_target_elements.clear();
            let mut eligible = true;
            build_indexed_event_tree(
                root,
                None,
                None,
                &mut self.event_tree,
                &mut self.event_target_by_element,
                &mut self.event_target_elements,
                &self.path_indices,
                &mut eligible,
            );
            self.event_tree_eligible = eligible && !self.event_tree.elements().is_empty();
            self.indexed_root_address = Some(root_address);
        } else if event_layout_changed && self.event_tree_eligible {
            for (element_id, target) in &self.event_target_by_element {
                let bounds = resolve_element_path(
                    root,
                    *element_id,
                    &self.path_indices,
                    &self.path_links,
                )
                .and_then(bounds_from_element);
                self.event_tree.update_bounds(*target, bounds);
            }
        }
        #[cfg(feature = "event-tree-exp")]
        {
            self.indexed_layout_generation = layout_generation;
        }
    }

    /// Resolves the focus owner for this frame, notifying both sides of a
    /// change.
    ///
    /// The decision itself belongs to [`FocusManager`]: this method only walks
    /// the tree for candidates and turns the reported transition into element
    /// events. A frame in which neither the tree nor any node asked for a change
    /// stops at the manager's gate, so no traversal happens at all.
    fn synchronize_focus(&mut self, root: &dyn Element) -> EventResult {
        let Some(request_generation) = self
            .focus
            .begin_synchronization(self.indexed_subtree_generation, self.indexed_root)
        else {
            return EventResult::ignored();
        };

        self.focus.set_scope(self.focus_scope);
        self.collect_candidates(root);
        let target = self.focus.resolve(&self.focus_candidates);

        let result = self.transition_focus(root, target);
        self.focus.mark_synchronized(
            self.indexed_subtree_generation,
            self.indexed_root,
            request_generation,
        );
        result
    }

    /// Gathers the focusable targets focus may be given to this frame.
    ///
    /// A trapping scope confines focus by omission: the walk starts at the scope
    /// instead of the root, so every target outside it is simply never offered —
    /// neither to [`FocusManager::resolve`] nor to traversal. A scope whose
    /// element can no longer be resolved has left the tree, and the whole tree is
    /// offered again.
    fn collect_candidates(&mut self, root: &dyn Element) {
        let scope = self
            .focus_scope
            .and_then(|scope| self.resolve_owner(root, scope))
            .unwrap_or(root);
        self.focus_candidates.clear();
        collect_focus_candidates(scope, &mut self.focus_candidates);
    }

    /// Returns whether a press is allowed to change who owns focus.
    ///
    /// A press reaches whatever is under it, including the tree behind an inline
    /// dialog, so it is the one way focus could leave a trapping scope without
    /// ever being offered by [`Self::collect_candidates`]. While a scope traps,
    /// a press therefore only moves focus when it landed on a target the scope
    /// contains: a press anywhere else — the dialog's own chrome as much as the
    /// application behind it — leaves the scope's owner alone rather than
    /// blurring it, which is what a press on a dialog's title bar should do.
    ///
    /// Without a scope every press decides focus, as it always has, and the
    /// check is one comparison against `None`.
    fn press_may_move_focus(
        &mut self,
        root: &dyn Element,
        target: Option<&FocusCandidate<ElementId>>,
    ) -> bool {
        if self.focus_scope.is_none() {
            return true;
        }
        let Some(target) = target else {
            return false;
        };
        self.collect_candidates(root);
        self.focus_candidates
            .iter()
            .any(|candidate| candidate.is_attached_to(target.id, &target.node))
    }

    /// Hands focus to `target` and delivers the resulting notifications.
    ///
    /// The losing element only hears about it while the node it reported is
    /// still the one attached to its identity; a node that left the tree with
    /// its element has nothing to notify.
    fn transition_focus(
        &mut self,
        root: &dyn Element,
        target: Option<FocusCandidate<ElementId>>,
    ) -> EventResult {
        let transition = self.focus.transition(target);
        let mut result = EventResult::ignored();

        if let Some(lost) = transition.lost
            && let Some(element) = self.resolve_owner(root, lost.id)
            && element
                .focus_node()
                .is_some_and(|node| node.ptr_eq(&lost.node))
        {
            if event_callback_enabled(element) {
                result = result.merge(element.on_event(&ElementEvent::FocusLost));
            }
        }
        if let Some(gained) = transition.gained
            && let Some(element) = self.resolve_owner(root, gained.id)
        {
            if event_callback_enabled(element) {
                result = result.merge(element.on_event(&ElementEvent::FocusGained));
            }
        }
        result
    }

    fn traverse_focus(&mut self, root: &dyn Element, reverse: bool) -> Option<EventResult> {
        self.collect_candidates(root);
        let target = self.focus.traverse(&self.focus_candidates, reverse)?;
        Some(self.transition_focus(root, Some(target)))
    }

    fn dispatch_to_focused(
        &self,
        root: &dyn Element,
        event: &ElementEvent,
    ) -> EventResult {
        let Some(focused) = self.focus.owner() else {
            return EventResult::ignored();
        };
        let Some(target) = self.resolve_owner(root, focused.id) else {
            return EventResult::ignored();
        };
        if !target
            .focus_node()
            .is_some_and(|node| node.ptr_eq(&focused.node))
        {
            return EventResult::ignored();
        }
        if event_callback_enabled(target) {
            target.on_event(event).without_capture_request()
        } else {
            EventResult::ignored()
        }
    }

    fn resolve_owner<'a>(
        &self,
        root: &'a dyn Element,
        owner: ElementId,
    ) -> Option<&'a dyn Element> {
        resolve_element_path(root, owner, &self.path_indices, &self.path_links)
    }

    fn dispatch_captured(
        &mut self,
        root: &dyn Element,
        pointer: PointerKey,
        event: &ElementEvent,
    ) -> EventResult {
        let Some(owner) = self.captures.get(&pointer).copied() else {
            return EventResult::ignored();
        };
        let Some(target) =
            resolve_element_path(root, owner, &self.path_indices, &self.path_links)
        else {
            self.captures.remove(&pointer);
            return EventResult::ignored();
        };
        if target.element_id() != Some(owner) {
            self.captures.remove(&pointer);
            return EventResult::ignored();
        }

        let result = if event_callback_enabled(target) {
            let mut context = EventDispatchContext::new(self, root, Some(owner));
            target.on_event_with_context(event, &mut context)
        } else {
            EventResult::ignored()
        };
        self.apply_capture_request(result.capture_request(), Some(owner));
        if matches!(event, ElementEvent::PointerUp(_)) {
            self.captures.remove(&pointer);
        }
        result.without_capture_request()
    }

    fn cancel_captures(&mut self, root: &dyn Element, event: &ElementEvent) -> EventResult {
        let owners: HashSet<ElementId> = self.captures.values().copied().collect();
        let mut result = EventResult::ignored();
        for owner in owners {
            let Some(target) =
                resolve_element_path(root, owner, &self.path_indices, &self.path_links)
            else {
                continue;
            };
            if target.element_id() == Some(owner) {
                if event_callback_enabled(target) {
                    let mut context = EventDispatchContext::new(self, root, Some(owner));
                    result = result.merge(target.on_event_with_context(event, &mut context));
                }
            }
        }
        self.captures.clear();
        result
    }

    fn apply_capture_request(&mut self, request: CaptureRequest, owner: Option<ElementId>) {
        match request {
            CaptureRequest::None => {}
            CaptureRequest::Capture(pointer) => {
                if let Some(owner) = owner {
                    self.captures.insert(pointer, owner);
                }
            }
            CaptureRequest::Release(pointer) => {
                self.captures.remove(&pointer);
            }
        }
    }
}

struct RoutedEventResult {
    result: EventResult,
    capture_owner: Option<ElementId>,
    focus_owner: Option<FocusCandidate<ElementId>>,
}

/// Gathers every focusable attachment of the tree, in traversal order.
fn collect_focus_candidates(
    element: &dyn Element,
    candidates: &mut FocusCandidates<ElementId>,
) {
    if let (Some(id), Some(node)) = (element.element_id(), element.focus_node()) {
        candidates.push(FocusCandidate::new(id, node.clone(), element.autofocus()));
    }
    element.focus_children(&mut |child| collect_focus_candidates(child, candidates));
}

fn event_pointer_key(event: &ElementEvent) -> Option<PointerKey> {
    match event {
        ElementEvent::PointerDown(pointer)
        | ElementEvent::PointerUp(pointer)
        | ElementEvent::PointerMove(pointer) => Some(PointerKey::new(pointer.source, pointer.id)),
        ElementEvent::PointerExited(source, id) => Some(PointerKey::new(*source, *id)),
        _ => None,
    }
}

#[inline]
fn event_callback_enabled(element: &dyn Element) -> bool {
    #[cfg(feature = "event-tree-exp")]
    {
        element.event_tree_role() != EventTreeRole::Transparent
    }
    #[cfg(not(feature = "event-tree-exp"))]
    {
        let _ = element;
        true
    }
}

#[cfg(feature = "event-tree-exp")]
fn build_indexed_event_tree(
    element: &dyn Element,
    parent: Option<EventTargetId>,
    parent_branch: Option<ElementId>,
    tree: &mut EventTree,
    target_by_element: &mut HashMap<ElementId, EventTargetId>,
    target_elements: &mut HashMap<ElementId, *const (dyn Element + 'static)>,
    path_indices: &HashMap<ElementId, usize>,
    eligible: &mut bool,
) {
    let role = element.event_tree_role();
    if role == EventTreeRole::Legacy {
        *eligible = false;
        return;
    }

    let is_indexed_target = matches!(
        role,
        EventTreeRole::IndexedTarget | EventTreeRole::IndexedHitTestBoundary
    );
    let mut child_branch = parent_branch;
    let parent = if is_indexed_target {
        let Some(id) = element.element_id().filter(|id| path_indices.contains_key(id)) else {
            *eligible = false;
            return;
        };
        if parent.is_some() && parent_branch.is_none() {
            *eligible = false;
            return;
        }
        let target = tree.register_under(id, parent, parent_branch);
        target_by_element.insert(id, target);
        target_elements.insert(id, retained_element_pointer(element));
        let bounds = bounds_from_element(element);
        tree.update_bounds(target, bounds);
        child_branch = None;
        Some(target)
    } else {
        parent
    };

    element.event_children(&mut |child| {
        let branch_start = tree.elements().len();
        let parent_branch = if parent.is_some() {
            child_branch.or_else(|| child.element_id())
        } else {
            None
        };
        build_indexed_event_tree(
            child,
            parent,
            parent_branch,
            tree,
            target_by_element,
            target_elements,
            path_indices,
            eligible,
        );
        if role == EventTreeRole::IndexedHitTestBoundary
            && let (Some(boundary), Some(branch)) = (parent, child.element_id())
        {
            tree.register_boundary_branch(boundary, branch, branch_start..tree.elements().len());
        }
    });

    if is_indexed_target
        && let Some(target) = parent
    {
        tree.finish_subtree(target);
    }
}

/// Indexes every element in a reusable parent-link arena, reporting the
/// innermost focus scope on the way.
///
/// The scope is discovered here rather than by a walk of its own because this
/// walk already visits the whole tree once per structural generation. Each
/// trapping element seen overwrites the previous one, so the last of them in
/// depth-first order wins — which is the innermost scope of the deepest
/// trapping branch, and for siblings the one presented last.
fn index_element_links(
    element: &dyn Element,
    parent: Option<usize>,
    child_index: u32,
    links: &mut Vec<ElementPath>,
    path_indices: &mut HashMap<ElementId, usize>,
    scope: &mut Option<ElementId>,
) {
    let link_index = links.len();
    links.push(ElementPath {
        parent,
        child_index,
    });

    if let Some(id) = element.element_id() {
        path_indices.insert(id, link_index);
        if element.traps_focus() {
            *scope = Some(id);
        }
    }

    let mut child_index = 0u32;
    element.structural_children(&mut |child| {
        let index = child_index;
        child_index = child_index
            .checked_add(1)
            .expect("exhausted structural child indexes");
        index_element_links(child, Some(link_index), index, links, path_indices, scope);
    });
}

fn resolve_element_path<'a>(
    root: &'a dyn Element,
    owner: ElementId,
    path_indices: &HashMap<ElementId, usize>,
    links: &[ElementPath],
) -> Option<&'a dyn Element> {
    let mut child_indexes: SmallVec<[u32; 16]> = SmallVec::new();
    let mut link_index = *path_indices.get(&owner)?;
    let mut remaining = links.len();

    loop {
        remaining = remaining.checked_sub(1)?;
        let link = *links.get(link_index)?;
        let Some(parent) = link.parent else {
            break;
        };
        child_indexes.push(link.child_index);
        link_index = parent;
    }

    let mut current = root;
    for index in child_indexes.iter().rev() {
        current = structural_child_at(current, *index as usize)?;
    }
    (current.element_id() == Some(owner)).then_some(current)
}

fn dispatch_routed_event(
    dispatcher: &mut EventDispatcher,
    root: &dyn Element,
    pos: Vec2d,
    event: &ElementEvent,
) -> RoutedEventResult {
    #[cfg(feature = "event-tree-exp")]
    if dispatcher.event_tree_eligible
        && dispatcher.indexed_root == root.element_id()
        && dispatcher.indexed_root_address
            == Some(root as *const dyn Element as *const ())
    {
        return dispatch_indexed_event(dispatcher, root, pos, event);
    }
    let mut children = EventChildren::new();
    dispatch_routed_event_inner(dispatcher, root, root, pos, event, &mut children)
}

#[cfg(feature = "event-tree-exp")]
fn dispatch_indexed_event(
    dispatcher: &mut EventDispatcher,
    root: &dyn Element,
    pos: Vec2d,
    event: &ElementEvent,
) -> RoutedEventResult {
    let frame_index = dispatcher.event_hit_depth.get();
    let _depth_guard = EventHitDepthGuard::enter(dispatcher.event_hit_depth.clone());
    if dispatcher.event_hit_frames.len() <= frame_index {
        dispatcher
            .event_hit_frames
            .resize_with(frame_index + 1, EventHitFrame::default);
    }
    let event_tree = &dispatcher.event_tree;
    let epoch = dispatcher.event_hit_frames[frame_index].prepare(event_tree.elements().len());
    for root_target in event_tree.roots().iter().rev().copied() {
        let Some(root_entry) = event_tree.get(root_target) else {
            continue;
        };
        let root_id = root_entry.element_id();
        let Some(root_element) = resolve_indexed_event_element(root, dispatcher, root_id) else {
            continue;
        };
        if !contains(root_element, pos) {
            continue;
        }

        if root_element.event_tree_role() == EventTreeRole::IndexedHitTestBoundary {
            dispatcher.event_hit_frames[frame_index].mark_hit_path(event_tree, root_target, epoch);
            let mut branch_ranges: SmallVec<[(usize, usize); 16]> = SmallVec::new();
            root_element.hit_test_children_at(pos, &mut |child| {
                if let Some(branch) = child.element_id()
                    && let Some(range) = event_tree.boundary_branch_range(root_target, branch)
                {
                    branch_ranges.push((range.start, range.end));
                }
            });
            dispatcher.event_hit_frames[frame_index]
                .mark_boundary_filtered(root_target, epoch);
            for (start, end) in branch_ranges.into_iter().rev() {
                if end == start + 1 {
                    let target = EventTargetId::from_index(start);
                    if event_tree.target_contains(target, pos.x, pos.y) {
                        dispatcher.event_hit_frames[frame_index]
                            .mark_hit_path(event_tree, target, epoch);
                    }
                    continue;
                }
                for target in event_tree.hit_test_range(start..end, pos.x, pos.y) {
                    dispatcher.event_hit_frames[frame_index].mark_hit_path(event_tree, target, epoch);
                }
            }
        } else {
            let range = root_entry.subtree_range(root_target);
            if range.start == 0 && range.end == event_tree.elements().len() {
                for target in event_tree.hit_test(pos.x, pos.y) {
                    dispatcher.event_hit_frames[frame_index].mark_hit_path(event_tree, target, epoch);
                }
            } else {
                for target in event_tree.hit_test_range(range, pos.x, pos.y) {
                    dispatcher.event_hit_frames[frame_index].mark_hit_path(event_tree, target, epoch);
                }
            }
        }
    }

    let mut result = RoutedEventResult {
        result: EventResult::ignored(),
        capture_owner: None,
        focus_owner: None,
    };
    let active_root_count = dispatcher.event_hit_frames[frame_index].roots.len();
    for root_index in 0..active_root_count {
        let root_target = dispatcher.event_hit_frames[frame_index].roots[root_index];
        let child = dispatch_indexed_target_inner(
            dispatcher,
            frame_index,
            epoch,
            root,
            root_target,
            pos,
            event,
        );
        result.result = result.result.merge(child.result);
        if result.focus_owner.is_none() {
            result.focus_owner = child.focus_owner;
        }
        if result.capture_owner.is_none() {
            result.capture_owner = child.capture_owner;
        }
        if child.result.is_consumed() {
            break;
        }
    }
    result
}

#[cfg(feature = "event-tree-exp")]
fn dispatch_indexed_target_inner(
    dispatcher: &mut EventDispatcher,
    frame_index: usize,
    epoch: u32,
    path_root: &dyn Element,
    target: EventTargetId,
    pos: Vec2d,
    event: &ElementEvent,
) -> RoutedEventResult {
    if dispatcher.event_hit_frames[frame_index].marks[target.index()] != epoch {
        return RoutedEventResult {
            result: EventResult::ignored(),
            capture_owner: None,
            focus_owner: None,
        };
    }
    let Some(target_element) = dispatcher.event_tree.get(target) else {
        return RoutedEventResult {
            result: EventResult::ignored(),
            capture_owner: None,
            focus_owner: None,
        };
    };
    let id = target_element.element_id();
    let Some(element) = resolve_indexed_event_element(path_root, dispatcher, id)
    else {
        return RoutedEventResult {
            result: EventResult::ignored(),
            capture_owner: None,
            focus_owner: None,
        };
    };
    if !contains(element, pos) {
        return RoutedEventResult {
            result: EventResult::ignored(),
            capture_owner: None,
            focus_owner: None,
        };
    }

    record_routed_event_visit();
    dispatcher.record_hit_chain_element(element);
    let mut outcome = RoutedEventResult {
        result: EventResult::ignored(),
        capture_owner: None,
        focus_owner: None,
    };
    let hit_test_boundary = element.event_tree_role() == EventTreeRole::IndexedHitTestBoundary
        && !dispatcher.event_hit_frames[frame_index].boundary_was_filtered(target, epoch);
    let mut hit_test_branches: SmallVec<[ElementId; 16]> = SmallVec::new();
    if hit_test_boundary {
        element.hit_test_children_at(pos, &mut |child| {
            if let Some(id) = child.element_id() {
                hit_test_branches.push(id);
            }
        });
    }
    let child_count = dispatcher.event_hit_frames[frame_index].active_children[target.index()].len();
    let hit_child_count = if hit_test_boundary {
        (0..child_count)
            .filter(|child_index| {
                let child_target = dispatcher.event_hit_frames[frame_index].active_children
                    [target.index()][*child_index];
                dispatcher
                    .event_tree
                    .get(child_target)
                    .and_then(|child| child.parent_branch())
                    .is_some_and(|branch| hit_test_branches.contains(&branch))
            })
            .count()
    } else {
        child_count
    };
    dispatcher.record_hit_chain_children(hit_child_count);
    if hit_child_count == 0 {
        dispatcher.record_empty_hit_chain_node();
    }
    for child_index in 0..child_count {
        let child_target = dispatcher.event_hit_frames[frame_index].active_children[target.index()][child_index];
        if hit_test_boundary
            && !dispatcher
                .event_tree
                .get(child_target)
                .and_then(|child| child.parent_branch())
                .is_some_and(|branch| hit_test_branches.contains(&branch))
        {
            continue;
        }
        let child = dispatch_indexed_target_inner(
            dispatcher,
            frame_index,
            epoch,
            path_root,
            child_target,
            pos,
            event,
        );
        outcome.result = outcome.result.merge(child.result);
        if outcome.focus_owner.is_none() {
            outcome.focus_owner = child.focus_owner;
        }
        if outcome.capture_owner.is_none() {
            outcome.capture_owner = child.capture_owner.or_else(|| {
                (!matches!(child.result.capture_request(), CaptureRequest::None)).then_some(id)
            });
        }
        if child.result.is_consumed() {
            break;
        }
    }

    if outcome.focus_owner.is_none() {
        outcome.focus_owner = focus_candidate_at(element, pos);
    }
    if !outcome.result.is_consumed() {
        let own_result = {
            let mut context = EventDispatchContext::new(dispatcher, path_root, Some(id));
            element.on_event_with_context(event, &mut context)
        };
        if outcome.capture_owner.is_none()
            && !matches!(own_result.capture_request(), CaptureRequest::None)
        {
            outcome.capture_owner = Some(id);
        }
        outcome.result = outcome.result.merge(own_result);
        if outcome.focus_owner.is_none() {
            outcome.focus_owner = focus_candidate_at(element, pos);
        }
    }
    outcome
}

#[cfg(feature = "event-tree-exp")]
fn resolve_indexed_event_element<'a>(
    root: &'a dyn Element,
    dispatcher: &EventDispatcher,
    id: ElementId,
) -> Option<&'a dyn Element> {
    let root_id = root.element_id()?;
    let root_address = root as *const dyn Element as *const ();
    if dispatcher.indexed_root != Some(root_id)
        || dispatcher.indexed_root_address != Some(root_address)
    {
        return None;
    }
    let element = *dispatcher.event_target_elements.get(&id)?;

    // SAFETY: `synchronize_paths` builds this map from elements below the exact
    // root address and subtree generation checked above. Rebuild/reconciliation
    // retires the map before targets move or are replaced, and dispatch runs
    // synchronously on the UI thread without concurrent tree mutation.
    let element: &'a dyn Element = unsafe { &*element };
    (element.element_id() == Some(id)).then_some(element)
}

fn dispatch_cached_hit_chain_inner(
    dispatcher: &mut EventDispatcher,
    path_root: &dyn Element,
    elements: &[CachedHitElement],
    index: usize,
    pos: Vec2d,
    event: &ElementEvent,
) -> RoutedEventResult {
    let entry = elements[index];
    // SAFETY: `dispatch_cached_hit_chain` validated the root address, element
    // identities, and subtree generation before entering this replay. The
    // retained UI tree cannot be concurrently mutated during dispatch.
    let root = unsafe { &*entry.element };
    record_routed_event_visit();

    let mut result = EventResult::ignored();
    let mut capture_owner = None;
    let mut focus_owner = None;
    let mut stopped = false;

    if index + 1 < elements.len() {
        let child_outcome = dispatch_cached_hit_chain_inner(
            dispatcher,
            path_root,
            elements,
            index + 1,
            pos,
            event,
        );
        result = result.merge(child_outcome.result);
        if focus_owner.is_none() {
            focus_owner = child_outcome.focus_owner;
        }
        if capture_owner.is_none() {
            capture_owner = child_outcome.capture_owner.or_else(|| {
                (!matches!(child_outcome.result.capture_request(), CaptureRequest::None))
                    .then(|| root.element_id())
                    .flatten()
            });
        }
        if child_outcome.result.is_consumed() {
            stopped = true;
        }
    }

    if stopped {
        if focus_owner.is_none() {
            focus_owner = focus_candidate_at(root, pos);
        }
        return RoutedEventResult {
            result,
            capture_owner,
            focus_owner,
        };
    }

    let own_result = if !event_callback_enabled(root) {
        EventResult::ignored()
    } else {
        let mut context = EventDispatchContext::new(dispatcher, path_root, root.element_id());
        root.on_event_with_context(event, &mut context)
    };
    if focus_owner.is_none() {
        focus_owner = focus_candidate_at(root, pos);
    }
    if capture_owner.is_none() && !matches!(own_result.capture_request(), CaptureRequest::None) {
        capture_owner = root.element_id();
    }
    result = result.merge(own_result);

    RoutedEventResult {
        result,
        capture_owner,
        focus_owner,
    }
}

/// Whether `pos` lies within `element`'s laid-out bounds.
///
/// An element that reports no bounds is taken to be everywhere, which is what
/// keeps a wrapper that never lays anything out from swallowing the events of
/// the subtree it stands for.
#[inline]
fn contains(element: &dyn Element, pos: Vec2d) -> bool {
    element.pos_start_end().is_none_or(|(start, end)| {
        pos.x >= start.x && pos.x <= end.x && pos.y >= start.y && pos.y <= end.y
    })
}

/// Offers `element` as the focus target of a press at `pos`, if it is one.
///
/// The bounds are only consulted once the element has answered with a node, so
/// the overwhelming majority of elements — which are not focus targets — cost a
/// single call.
#[inline]
fn focus_candidate_at(element: &dyn Element, pos: Vec2d) -> Option<FocusCandidate<ElementId>> {
    let node = element.focus_node()?;
    let id = element.element_id()?;
    contains(element, pos).then(|| FocusCandidate::new(id, node.clone(), element.autofocus()))
}

fn dispatch_routed_event_inner<'tree>(
    dispatcher: &mut EventDispatcher,
    path_root: &dyn Element,
    root: &'tree dyn Element,
    pos: Vec2d,
    event: &ElementEvent,
    children: &mut EventChildren<'tree>,
) -> RoutedEventResult {
    if !contains(root, pos) {
        dispatcher.record_hit_chain_miss();
        return RoutedEventResult {
            result: EventResult::ignored(),
            capture_owner: None,
            focus_owner: None,
        };
    }
    record_routed_event_visit();
    dispatcher.record_hit_chain_element(root);

    let mut result = EventResult::ignored();
    let mut capture_owner = None;
    let mut focus_owner = None;
    let mut stopped = false;
    let start = children.len();
    let mut hit_test_children = 0;
    root.hit_test_children_at(pos, &mut |child| {
        hit_test_children += 1;
        children.push(child);
    });
    dispatcher.record_hit_chain_children(hit_test_children);
    if hit_test_children == 0 {
        dispatcher.record_empty_hit_chain_node();
    }

    while children.len() > start {
        if stopped {
            children.truncate(start);
            break;
        }
        let child = children
            .pop()
            .expect("routed event scratch contains an element beyond its entry length");
        let child_outcome = dispatch_routed_event_inner(
            dispatcher,
            path_root,
            child,
            pos,
            event,
            children,
        );
        result = result.merge(child_outcome.result);
        if focus_owner.is_none() {
            focus_owner = child_outcome.focus_owner;
        }
        if capture_owner.is_none() {
            capture_owner = child_outcome.capture_owner.or_else(|| {
                (!matches!(child_outcome.result.capture_request(), CaptureRequest::None))
                    .then(|| root.element_id())
                    .flatten()
            });
        }
        if child_outcome.result.is_consumed() {
            // The press stopped here, but it landed inside this element all the
            // same: a control that takes a press for itself still sits within
            // whatever region encloses it, and that region is what the focus
            // should move to. Giving up the search here would leave the press
            // with no target at all — and a press with no target takes the
            // keyboard away, so clicking a field inside a focusable region
            // would blur it.
            stopped = true;
        }
    }
    children.truncate(start);

    if stopped {
        if focus_owner.is_none() {
            focus_owner = focus_candidate_at(root, pos);
        }
        return RoutedEventResult {
            result,
            capture_owner,
            focus_owner,
        };
    }

    let own_result = if !event_callback_enabled(root) {
        EventResult::ignored()
    } else {
        let mut context = EventDispatchContext::new(dispatcher, path_root, root.element_id());
        root.on_event_with_context(event, &mut context)
    };
    if focus_owner.is_none() {
        focus_owner = focus_candidate_at(root, pos);
    }
    if capture_owner.is_none() && !matches!(own_result.capture_request(), CaptureRequest::None) {
        capture_owner = root.element_id();
    }
    result = result.merge(own_result);

    RoutedEventResult {
        result,
        capture_owner,
        focus_owner,
    }
}

/// Perform a hit-test on the element tree and dispatch the event to the deepest
/// hit element. Returns the effects produced by the target element.
pub fn dispatch_event(root: &dyn Element, pos: Vec2d, event: &ElementEvent) -> EventResult {
    #[cfg(feature = "event-tree-exp")]
    {
        let mut dispatcher = EventDispatcher::new();
        dispatcher.synchronize_paths(root);
        if dispatcher.event_tree_eligible {
            return dispatch_indexed_event(&mut dispatcher, root, pos, event).result;
        }
    }
    let mut children = EventChildren::new();
    dispatch_event_inner(root, pos, event, &mut children)
}

fn dispatch_event_inner<'a>(
    root: &'a dyn Element,
    pos: Vec2d,
    event: &ElementEvent,
    children: &mut EventChildren<'a>,
) -> EventResult {
    if !contains(root, pos) {
        return EventResult::ignored();
    }

    let mut result = EventResult::ignored();
    let start = children.len();
    root.hit_test_children_at(pos, &mut |child| {
        if contains(child, pos) {
            children.push(child);
        }
    });

    while children.len() > start {
        let child = children
            .pop()
            .expect("event child scratch contains an element beyond its entry length");
        let child_result = dispatch_event_inner(child, pos, event, children);
        result = result.merge(child_result);
        if child_result.is_consumed() {
            children.truncate(start);
            return result;
        }
    }
    children.truncate(start);

    if !event_callback_enabled(root) {
        result
    } else {
        result.merge(root.on_event(event))
    }
}

/// Broadcast an event to every element in the tree, regardless of hit-testing.
/// Returns the combined effects produced by every element.
pub fn broadcast_event(root: &dyn Element, event: &ElementEvent) -> EventResult {
    #[cfg(feature = "event-tree-exp")]
    {
        let mut dispatcher = EventDispatcher::new();
        dispatcher.synchronize_paths(root);
        if dispatcher.event_tree_eligible {
            let mut result = EventResult::ignored();
            for target in dispatcher.event_tree.roots().iter().rev().copied() {
                result = result.merge(broadcast_indexed_target(
                    root,
                    &dispatcher.event_tree,
                    &dispatcher.path_indices,
                    &dispatcher.path_links,
                    target,
                    event,
                ));
            }
            return result;
        }
    }
    let mut children = EventChildren::new();
    broadcast_event_inner(root, event, &mut children)
}

#[cfg(feature = "event-tree-exp")]
fn broadcast_indexed_target(
    root: &dyn Element,
    tree: &EventTree,
    path_indices: &HashMap<ElementId, usize>,
    path_links: &[ElementPath],
    target: EventTargetId,
    event: &ElementEvent,
) -> EventResult {
    let Some(entry) = tree.get(target) else {
        return EventResult::ignored();
    };
    let mut result = EventResult::ignored();
    for child in entry.children().iter().rev().copied() {
        result = result.merge(broadcast_indexed_target(
            root,
            tree,
            path_indices,
            path_links,
            child,
            event,
        ));
    }
    if let Some(element) = resolve_element_path(root, entry.element_id(), path_indices, path_links) {
        result.merge(element.on_event(event))
    } else {
        result
    }
}

fn broadcast_event_inner<'a>(
    root: &'a dyn Element,
    event: &ElementEvent,
    children: &mut EventChildren<'a>,
) -> EventResult {
    let mut result = EventResult::ignored();
    let start = children.len();
    root.event_children(&mut |child| children.push(child));

    while children.len() > start {
        let child = children
            .pop()
            .expect("event child scratch contains an element beyond its entry length");
        result = result.merge(broadcast_event_inner(child, event, children));
    }
    children.truncate(start);

    if !event_callback_enabled(root) {
        result
    } else {
        result.merge(root.on_event(event))
    }
}

/// Deliver a focus-directed event to the element that owns keyboard focus.
///
/// Keyboard text and input-method composition carry no meaningful position, so
/// the tree is walked depth-first and every element is offered the event until
/// one consumes it; elements without focus ignore it. Unlike
/// [`broadcast_event`], delivery stops at the first consumer, so a focused field
/// nested inside another field never sees the same text twice.
pub fn dispatch_focused_event(root: &dyn Element, event: &ElementEvent) -> EventResult {
    #[cfg(feature = "event-tree-exp")]
    {
        let mut dispatcher = EventDispatcher::new();
        dispatcher.synchronize_paths(root);
        if dispatcher.event_tree_eligible {
            for target in dispatcher.event_tree.roots().iter().rev().copied() {
                let result = dispatch_focused_indexed_target(
                    root,
                    &dispatcher.event_tree,
                    &dispatcher.path_indices,
                    &dispatcher.path_links,
                    target,
                    event,
                );
                if result.is_consumed() {
                    return result;
                }
            }
            return EventResult::ignored();
        }
    }
    let mut children = EventChildren::new();
    dispatch_focused_event_inner(root, event, &mut children)
}

#[cfg(feature = "event-tree-exp")]
fn dispatch_focused_indexed_target(
    root: &dyn Element,
    tree: &EventTree,
    path_indices: &HashMap<ElementId, usize>,
    path_links: &[ElementPath],
    target: EventTargetId,
    event: &ElementEvent,
) -> EventResult {
    let Some(entry) = tree.get(target) else {
        return EventResult::ignored();
    };
    for child in entry.children().iter().rev().copied() {
        let result = dispatch_focused_indexed_target(
            root,
            tree,
            path_indices,
            path_links,
            child,
            event,
        );
        if result.is_consumed() {
            return result;
        }
    }
    resolve_element_path(root, entry.element_id(), path_indices, path_links)
        .map(|element| element.on_event(event))
        .unwrap_or_else(EventResult::ignored)
}

fn dispatch_focused_event_inner<'a>(
    root: &'a dyn Element,
    event: &ElementEvent,
    children: &mut EventChildren<'a>,
) -> EventResult {
    let mut result = EventResult::ignored();
    let start = children.len();
    root.event_children(&mut |child| children.push(child));

    while children.len() > start {
        let child = children
            .pop()
            .expect("event child scratch contains an element beyond its entry length");
        result = result.merge(dispatch_focused_event_inner(child, event, children));
        if result.is_consumed() {
            children.truncate(start);
            return result;
        }
    }
    children.truncate(start);

    if !event_callback_enabled(root) {
        result
    } else {
        result.merge(root.on_event(event))
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use aimer_events::element::{KeyAction, Modifiers, NamedKey};
    use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};
    use aimer_rubick::INLINE_CAPACITY;

    use super::*;
    use crate::focus::FocusTrap;
    use crate::{FocusNode, Key};

    #[test]
    fn paint_damage_starts_full_once_and_tracks_later_local_regions() {
        begin_paint_frame(37, 23);
        assert!(take_paint_frame_damage(37, 23).is_full());

        begin_paint_frame(37, 23);
        assert!(take_paint_frame_damage(37, 23).is_empty());

        begin_paint_frame(37, 23);
        mark_paint_damage(DamageRect::new(4, 5, 6, 7));
        assert_eq!(
            take_paint_frame_damage(37, 23).regions(),
            &[DamageRect::new(4, 5, 6, 7)]
        );

        begin_paint_frame(38, 23);
        assert!(take_paint_frame_damage(38, 23).is_full());
    }

    #[test]
    fn full_damage_requested_between_frames_is_applied_to_the_next_frame() {
        begin_paint_frame(41, 29);
        assert!(take_paint_frame_damage(41, 29).is_full());

        mark_paint_damage_full();
        begin_paint_frame(41, 29);

        assert!(take_paint_frame_damage(41, 29).is_full());
    }

    #[test]
    fn untracked_rebuild_invalidations_force_full_damage() {
        begin_paint_frame(41, 29);
        let _ = take_paint_frame_damage(41, 29);

        advance_rebuild_invalidation_generation();
        begin_paint_frame(41, 29);

        assert!(take_paint_frame_damage(41, 29).is_full());
    }

    struct StructuralTraversalElement {
        event_child: AnyElement,
        visual_child: AnyElement,
        direct_calls: Rc<Cell<usize>>,
    }

    impl VisitorElement for StructuralTraversalElement {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.visual_child.as_ref());
        }

        fn debug_name(&self) -> &'static str {
            "StructuralTraversalElement"
        }
    }

    impl EventElement for StructuralTraversalElement {
        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.event_child.as_ref());
        }

        fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            self.direct_calls.set(self.direct_calls.get() + 1);
            visitor(self.event_child.as_ref());
            visitor(self.visual_child.as_ref());
        }
    }

    impl LayoutElement for StructuralTraversalElement {}

    impl Drawable for StructuralTraversalElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for StructuralTraversalElement {}

    struct DefaultStructuralTraversalElement {
        event_child: AnyElement,
        visual_child: AnyElement,
    }

    impl VisitorElement for DefaultStructuralTraversalElement {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.visual_child.as_ref());
        }

        fn debug_name(&self) -> &'static str {
            "DefaultStructuralTraversalElement"
        }
    }

    impl EventElement for DefaultStructuralTraversalElement {
        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.event_child.as_ref());
        }
    }

    impl LayoutElement for DefaultStructuralTraversalElement {}

    impl Drawable for DefaultStructuralTraversalElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for DefaultStructuralTraversalElement {}

    #[test]
    fn default_structural_traversal_preserves_event_visual_union() {
        let element = DefaultStructuralTraversalElement {
            event_child: DowncastableElement.boxed(),
            visual_child: DowncastableElement.boxed(),
        };

        let children = structural_children(&element);

        assert_eq!(children.len(), 2);
        assert!(std::ptr::eq(children[0], element.event_child.as_ref()));
        assert!(std::ptr::eq(children[1], element.visual_child.as_ref()));
    }

    #[test]
    fn structural_children_uses_element_specific_traversal() {
        let element = StructuralTraversalElement {
            event_child: DowncastableElement.boxed(),
            visual_child: DowncastableElement.boxed(),
            direct_calls: Rc::new(Cell::new(0)),
        };

        let children = structural_children(&element);

        assert_eq!(element.direct_calls.get(), 1);
        assert_eq!(children.len(), 2);
        assert!(std::ptr::eq(children[0], element.event_child.as_ref()));
        assert!(std::ptr::eq(children[1], element.visual_child.as_ref()));
    }

    #[test]
    fn erased_element_forwards_structural_traversal() {
        let direct_calls = Rc::new(Cell::new(0));
        let element = StructuralTraversalElement {
            event_child: DowncastableElement.boxed(),
            visual_child: DowncastableElement.boxed(),
            direct_calls: direct_calls.clone(),
        };
        let erased = element.boxed();

        let _ = structural_children(erased.as_ref());

        assert_eq!(direct_calls.get(), 1);
    }

    #[test]
    fn focus_dispatch_uses_structural_traversal() {
        let direct_calls = Rc::new(Cell::new(0));
        let root = StructuralTraversalElement {
            event_child: DowncastableElement.boxed(),
            visual_child: DowncastableElement.boxed(),
            direct_calls: direct_calls.clone(),
        }
        .boxed();
        let event = ElementEvent::KeyInput {
            key: NamedKey::Tab,
            action: KeyAction::Pressed,
            modifiers: Modifiers::default(),
        };

        let mut dispatcher = EventDispatcher::new();
        let _ = dispatcher.dispatch(&root, Vec2d::default(), &event);

        assert!(direct_calls.get() > 0);
    }

    struct DowncastableElement;

    impl VisitorElement for DowncastableElement {
        fn debug_name(&self) -> &'static str {
            "DowncastableElement"
        }
    }

    impl EventElement for DowncastableElement {}
    impl LayoutElement for DowncastableElement {}
    impl Drawable for DowncastableElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for DowncastableElement {
        fn option_any(&self) -> Option<&dyn Any> {
            Some(self)
        }
    }

    #[test]
    fn boxed_element_delegates_runtime_downcasting() {
        let element: Box<dyn Element> = Box::new(DowncastableElement);

        assert!(
            element
                .option_any()
                .is_some_and(|value| value.is::<DowncastableElement>())
        );
    }

    struct StorageElement<const N: usize>([u8; N]);

    impl<const N: usize> VisitorElement for StorageElement<N> {
        fn debug_name(&self) -> &'static str {
            "StorageElement"
        }
    }

    impl<const N: usize> EventElement for StorageElement<N> {}
    impl<const N: usize> LayoutElement for StorageElement<N> {}
    impl<const N: usize> Drawable for StorageElement<N> {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl<const N: usize> Rebuildable for StorageElement<N> {
        fn option_any(&self) -> Option<&dyn Any> {
            Some(self)
        }
    }

    #[test]
    fn erased_elements_select_inline_or_heap_storage_and_dispatch_after_moves() {
        let inline = StorageElement([]).boxed();
        let heap = StorageElement([0; INLINE_CAPACITY + 1]).boxed();

        assert!(inline.is_inline());
        assert!(heap.is_heap());

        let owners = std::hint::black_box([inline, heap]);
        assert_eq!(owners[0].debug_name(), "StorageElement");
        assert!(
            owners[1]
                .option_any()
                .is_some_and(|value| { value.is::<StorageElement<{ INLINE_CAPACITY + 1 }>>() })
        );
    }

    struct IdentityLeaf {
        key: Option<Key>,
    }

    impl IdentityLeaf {
        fn unkeyed() -> Self {
            Self { key: None }
        }

        fn keyed(key: &'static str) -> Self {
            Self {
                key: Some(Key::Static(key)),
            }
        }
    }

    impl VisitorElement for IdentityLeaf {
        fn debug_name(&self) -> &'static str {
            "IdentityLeaf"
        }

        fn reconciliation_key(&self) -> Option<&Key> {
            self.key.as_ref()
        }
    }

    impl EventElement for IdentityLeaf {}
    impl LayoutElement for IdentityLeaf {}
    impl Drawable for IdentityLeaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for IdentityLeaf {}

    struct ReplacementLeaf;

    impl VisitorElement for ReplacementLeaf {
        fn debug_name(&self) -> &'static str {
            "ReplacementLeaf"
        }
    }

    impl EventElement for ReplacementLeaf {}
    impl LayoutElement for ReplacementLeaf {}
    impl Drawable for ReplacementLeaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for ReplacementLeaf {}

    struct IdentityBranch(Vec<AnyElement>);

    impl VisitorElement for IdentityBranch {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.0 {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "IdentityBranch"
        }
    }

    impl EventElement for IdentityBranch {}
    impl LayoutElement for IdentityBranch {}
    impl Drawable for IdentityBranch {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for IdentityBranch {}

    struct StableRoot {
        children: Vec<AnyElement>,
        visits: Rc<Cell<usize>>,
    }

    impl VisitorElement for StableRoot {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            self.visits.set(self.visits.get() + 1);
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "StableRoot"
        }
    }

    impl EventElement for StableRoot {}
    impl LayoutElement for StableRoot {
        fn is_layout_stable(&self) -> bool {
            true
        }
    }
    impl Drawable for StableRoot {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for StableRoot {}

    fn child_ids(element: &dyn Element) -> Vec<ElementId> {
        let mut ids = Vec::new();
        element.visit_children(&mut |child| ids.push(child.id()));
        ids
    }

    #[test]
    fn boxed_elements_have_unique_ids_that_survive_owner_moves() {
        let first = IdentityLeaf::unkeyed().boxed();
        let second = IdentityLeaf::unkeyed().boxed();
        let first_id = first.id();

        assert_ne!(first_id, second.id());

        let moved = std::hint::black_box(vec![second, first]);
        assert_eq!(moved[1].id(), first_id);
    }

    #[test]
    fn reconciliation_preserves_keyed_ids_across_reorder() {
        let old = IdentityBranch(vec![
            IdentityLeaf::keyed("a").boxed(),
            IdentityLeaf::keyed("b").boxed(),
        ])
        .boxed();
        let new = IdentityBranch(vec![
            IdentityLeaf::keyed("b").boxed(),
            IdentityLeaf::keyed("a").boxed(),
        ])
        .boxed();
        let old_ids = child_ids(old.as_ref());

        reconcile_element_identities(old.as_ref(), new.as_ref());

        assert_eq!(child_ids(new.as_ref()), vec![old_ids[1], old_ids[0]]);
    }

    #[test]
    fn reconciliation_preserves_compatible_unkeyed_ids_by_position() {
        let old = IdentityBranch(vec![
            IdentityLeaf::unkeyed().boxed(),
            IdentityLeaf::unkeyed().boxed(),
        ])
        .boxed();
        let new = IdentityBranch(vec![
            IdentityLeaf::unkeyed().boxed(),
            IdentityLeaf::unkeyed().boxed(),
        ])
        .boxed();
        let old_ids = child_ids(old.as_ref());

        reconcile_element_identities(old.as_ref(), new.as_ref());

        assert_eq!(child_ids(new.as_ref()), old_ids);
    }

    #[test]
    fn reconciliation_assigns_replacements_a_new_id() {
        let old = IdentityBranch(vec![IdentityLeaf::unkeyed().boxed()]).boxed();
        let new = IdentityBranch(vec![ReplacementLeaf.boxed()]).boxed();
        let old_id = child_ids(old.as_ref())[0];
        let replacement_id = child_ids(new.as_ref())[0];

        reconcile_element_identities(old.as_ref(), new.as_ref());

        assert_ne!(child_ids(new.as_ref())[0], old_id);
        assert_eq!(child_ids(new.as_ref())[0], replacement_id);
    }

    #[test]
    fn generated_tree_reconciliation_advances_the_structure_generation() {
        let old = IdentityLeaf::unkeyed().boxed();
        let new = IdentityLeaf::unkeyed().boxed();
        let generation = element_tree_generation();

        reconcile_generated_tree(old.as_ref(), new.as_ref());

        assert!(element_tree_generation() > generation);
        assert_eq!(new.id(), old.id());
    }

    #[test]
    fn dispatcher_reindexes_at_most_once_per_event_frame() {
        let visits = Rc::new(Cell::new(0));
        let root = StableRoot {
            children: Vec::new(),
            visits: visits.clone(),
        }
        .boxed();
        let root_generation = root.subtree_generation();
        let mut dispatcher = EventDispatcher::new();

        begin_event_frame();
        dispatcher.synchronize_paths(root.as_ref());
        assert_eq!(dispatcher.indexed_subtree_generation, root_generation);
        let first_index_visits = visits.get();
        assert!(first_index_visits > 0);

        advance_element_tree_generation();
        assert_ne!(element_tree_generation(), root_generation);
        assert_eq!(root.subtree_generation(), root_generation);

        dispatcher.synchronize_paths(root.as_ref());
        assert_eq!(visits.get(), first_index_visits);

        let changed_generation = root_generation + 1;
        root.set_subtree_generation(changed_generation);
        dispatcher.synchronize_paths(root.as_ref());
        assert_eq!(
            visits.get(),
            first_index_visits,
            "a generation change after the first dispatch waits for the next frame"
        );

        begin_event_frame();
        dispatcher.synchronize_paths(root.as_ref());

        assert_eq!(dispatcher.indexed_subtree_generation, changed_generation);
        let second_index_visits = visits.get();
        assert!(second_index_visits > first_index_visits);

        dispatcher.synchronize_paths(root.as_ref());
        assert_eq!(visits.get(), second_index_visits);
    }

    struct LayoutInvalidationLeaf {
        invalidations: Rc<Cell<usize>>,
    }

    impl VisitorElement for LayoutInvalidationLeaf {
        fn debug_name(&self) -> &'static str {
            "LayoutInvalidationLeaf"
        }
    }

    impl EventElement for LayoutInvalidationLeaf {}

    impl LayoutElement for LayoutInvalidationLeaf {
        fn invalidate_layout(&self) {
            self.invalidations.set(self.invalidations.get() + 1);
        }
    }

    impl Drawable for LayoutInvalidationLeaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for LayoutInvalidationLeaf {}

    struct LayoutInvalidationBranch {
        child: AnyElement,
    }

    impl VisitorElement for LayoutInvalidationBranch {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.child.as_ref());
        }

        fn debug_name(&self) -> &'static str {
            "LayoutInvalidationBranch"
        }
    }

    impl EventElement for LayoutInvalidationBranch {}

    impl LayoutElement for LayoutInvalidationBranch {
        fn invalidate_layout(&self) {
            self.child.invalidate_layout();
        }
    }

    impl Drawable for LayoutInvalidationBranch {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for LayoutInvalidationBranch {}

    #[test]
    fn erased_layout_invalidation_marks_once_without_walking_children() {
        let invalidations = Rc::new(Cell::new(0));
        let root = LayoutInvalidationBranch {
            child: LayoutInvalidationLeaf {
                invalidations: invalidations.clone(),
            }
            .boxed(),
        }
        .boxed();
        let generation = layout_invalidation_generation();

        root.invalidate_layout();

        assert!(layout_invalidation_generation() > generation);
        assert_eq!(invalidations.get(), 0);
    }

    struct RoutedElement {
        children: Vec<AnyElement>,
        bounds: Option<(Vec2d, Vec2d)>,
        events: Rc<Cell<usize>>,
        capture_on_down: bool,
        release_on_move: bool,
    }

    impl VisitorElement for RoutedElement {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "RoutedElement"
        }
    }

    impl EventElement for RoutedElement {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            match event {
                ElementEvent::PointerDown(pointer) if self.capture_on_down => {
                    EventResult::consumed()
                        .with_pointer_capture(PointerKey::new(pointer.source, pointer.id))
                }
                ElementEvent::PointerMove(pointer) if self.release_on_move => {
                    EventResult::consumed()
                        .with_pointer_release(PointerKey::new(pointer.source, pointer.id))
                }
                _ => EventResult::consumed(),
            }
        }
    }

    impl LayoutElement for RoutedElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            self.bounds
        }
    }

    impl Drawable for RoutedElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for RoutedElement {}

    struct HitChainCacheLeaf {
        bounds: (Vec2d, Vec2d),
        events: Rc<Cell<usize>>,
        consume_moves: bool,
    }

    impl VisitorElement for HitChainCacheLeaf {
        fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

        fn debug_name(&self) -> &'static str {
            "HitChainCacheLeaf"
        }
    }

    impl EventElement for HitChainCacheLeaf {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            if matches!(event, ElementEvent::PointerMove(_)) {
                self.events.set(self.events.get() + 1);
            }
            if self.consume_moves && matches!(event, ElementEvent::PointerMove(_)) {
                EventResult::consumed()
            } else {
                EventResult::ignored()
            }
        }
    }

    impl LayoutElement for HitChainCacheLeaf {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }

    impl Drawable for HitChainCacheLeaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for HitChainCacheLeaf {}

    struct HitChainCacheRoot {
        child: AnyElement,
        hit_tests: Rc<Cell<usize>>,
    }

    impl VisitorElement for HitChainCacheRoot {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.child.as_ref());
        }

        fn debug_name(&self) -> &'static str {
            "HitChainCacheRoot"
        }
    }

    impl EventElement for HitChainCacheRoot {
        fn hit_test_children_at<'a>(
            &'a self,
            pos: Vec2d,
            visitor: &mut dyn FnMut(&'a dyn Element),
        ) {
            self.hit_tests.set(self.hit_tests.get() + 1);
            if contains(self.child.as_ref(), pos) {
                visitor(self.child.as_ref());
            }
        }
    }

    impl LayoutElement for HitChainCacheRoot {
        fn is_layout_stable(&self) -> bool {
            true
        }
    }

    impl Drawable for HitChainCacheRoot {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for HitChainCacheRoot {}

    struct HitChainCacheForwarder {
        child: AnyElement,
        bounds: (Vec2d, Vec2d),
        events: Rc<Cell<usize>>,
    }

    impl VisitorElement for HitChainCacheForwarder {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.child.as_ref());
        }

        fn debug_name(&self) -> &'static str {
            "HitChainCacheForwarder"
        }
    }

    impl EventElement for HitChainCacheForwarder {
        fn event_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

        fn on_event_with_context(
            &self,
            event: &ElementEvent,
            context: &mut EventDispatchContext<'_, '_>,
        ) -> EventResult {
            if matches!(event, ElementEvent::PointerMove(_)) {
                self.events.set(self.events.get() + 1);
            }
            let pos = match event {
                ElementEvent::PointerMove(info) => info.pos,
                _ => Vec2d::default(),
            };
            context.dispatch_child(self.child.as_ref(), pos, event)
        }
    }

    impl LayoutElement for HitChainCacheForwarder {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }

    impl Drawable for HitChainCacheForwarder {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for HitChainCacheForwarder {}

    struct HoverProbe {
        bounds: (Vec2d, Vec2d),
        hovered: Rc<Cell<bool>>,
    }

    impl VisitorElement for HoverProbe {
        fn debug_name(&self) -> &'static str {
            "HoverProbe"
        }
    }

    impl EventElement for HoverProbe {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            match event {
                ElementEvent::PointerMove(_) => {
                    self.hovered.set(true);
                    EventResult::ignored()
                }
                ElementEvent::PointerExited(_, _) => {
                    self.hovered.set(false);
                    EventResult::ignored()
                }
                _ => EventResult::ignored(),
            }
        }
    }

    impl LayoutElement for HoverProbe {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }

    impl Drawable for HoverProbe {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for HoverProbe {}

    struct HoverProbeRoot {
        bounds: (Vec2d, Vec2d),
        children: Vec<AnyElement>,
    }

    impl VisitorElement for HoverProbeRoot {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "HoverProbeRoot"
        }
    }

    impl EventElement for HoverProbeRoot {
        fn hit_test_children_at<'a>(
            &'a self,
            pos: Vec2d,
            visitor: &mut dyn FnMut(&'a dyn Element),
        ) {
            for child in &self.children {
                if contains(child.as_ref(), pos) {
                    visitor(child.as_ref());
                }
            }
        }
    }

    impl LayoutElement for HoverProbeRoot {
        fn is_layout_stable(&self) -> bool {
            true
        }

        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }

    impl Drawable for HoverProbeRoot {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for HoverProbeRoot {}

    #[test]
    fn moving_between_siblings_exits_the_previous_hover_target() {
        let first_hovered = Rc::new(Cell::new(false));
        let second_hovered = Rc::new(Cell::new(false));
        let root = HoverProbeRoot {
            bounds: (Vec2d::default(), Vec2d { x: 100.0, y: 100.0 }),
            children: vec![
                HoverProbe {
                    bounds: (Vec2d::default(), Vec2d { x: 40.0, y: 40.0 }),
                    hovered: first_hovered.clone(),
                }
                .boxed(),
                HoverProbe {
                    bounds: (
                        Vec2d { x: 60.0, y: 0.0 },
                        Vec2d { x: 100.0, y: 40.0 },
                    ),
                    hovered: second_hovered.clone(),
                }
                .boxed(),
            ],
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();

        let first = Vec2d { x: 20.0, y: 20.0 };
        let second = Vec2d { x: 80.0, y: 20.0 };
        let outside = Vec2d { x: 20.0, y: 80.0 };
        let move_pointer = |dispatcher: &mut EventDispatcher, pos| {
            let _ = dispatcher.dispatch(
                root.as_ref(),
                pos,
                &ElementEvent::PointerMove(PointerInfo::mouse(
                    pos,
                    PointerButton::Primary,
                )),
            );
        };

        move_pointer(&mut dispatcher, first);
        assert!(first_hovered.get());
        assert!(!second_hovered.get());

        move_pointer(&mut dispatcher, second);
        assert!(!first_hovered.get());
        assert!(second_hovered.get());

        move_pointer(&mut dispatcher, outside);
        assert!(!second_hovered.get());
    }

    #[test]
    fn uncaptured_pointer_moves_reuse_the_last_hit_chain() {
        let hit_tests = Rc::new(Cell::new(0));
        let leaf_events = Rc::new(Cell::new(0));
        let root = HitChainCacheRoot {
            child: HitChainCacheLeaf {
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                events: leaf_events.clone(),
                consume_moves: false,
            }
            .boxed(),
            hit_tests: hit_tests.clone(),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let pos = Vec2d { x: 5.0, y: 5.0 };

        reset_routed_event_visit_count();
        let _ = dispatcher.dispatch(
            root.as_ref(),
            pos,
            &ElementEvent::PointerMove(PointerInfo::mouse(pos, PointerButton::Primary)),
        );
        let first_visits = take_routed_event_visit_count();
        let first_hit_tests = hit_tests.get();

        reset_routed_event_visit_count();
        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 6.0, y: 6.0 },
            &ElementEvent::PointerMove(PointerInfo::mouse(
                Vec2d { x: 6.0, y: 6.0 },
                PointerButton::Primary,
            )),
        );
        let second_visits = take_routed_event_visit_count();

        assert_eq!(hit_tests.get(), first_hit_tests);
        assert_eq!(first_visits, 2);
        assert_eq!(second_visits, first_visits);
        assert_eq!(leaf_events.get(), 2);
    }

    #[test]
    fn overlapping_hover_chain_membership_is_linear() {
        const CHILD_COUNT: usize = 64;
        let bounds = (Vec2d::default(), Vec2d { x: 100.0, y: 100.0 });
        let hovered: Vec<_> = (0..CHILD_COUNT).map(|_| Rc::new(Cell::new(false))).collect();
        let children = hovered
            .iter()
            .map(|hovered| {
                HoverProbe {
                    bounds,
                    hovered: hovered.clone(),
                }
                .boxed()
            })
            .collect();
        let root = HoverProbeRoot { bounds, children }.boxed();
        let mut dispatcher = EventDispatcher::new();
        let first = Vec2d { x: 10.0, y: 10.0 };
        let second = Vec2d { x: 20.0, y: 20.0 };

        reset_hover_membership_check_count();
        for pos in [first, second] {
            let _ = dispatcher.dispatch(
                root.as_ref(),
                pos,
                &ElementEvent::PointerMove(PointerInfo::mouse(
                    pos,
                    PointerButton::Primary,
                )),
            );
        }

        assert!(hovered.iter().all(|hovered| hovered.get()));
        let checks = take_hover_membership_check_count();
        assert!(
            checks <= (CHILD_COUNT as u64 + 1) * 2,
            "hover membership checks must remain linear, observed {checks}"
        );
    }

    #[test]
    fn cached_hit_chain_falls_back_when_the_pointer_leaves_the_chain() {
        let hit_tests = Rc::new(Cell::new(0));
        let leaf_events = Rc::new(Cell::new(0));
        let root = HitChainCacheRoot {
            child: HitChainCacheLeaf {
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                events: leaf_events.clone(),
                consume_moves: false,
            }
            .boxed(),
            hit_tests: hit_tests.clone(),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let inside = Vec2d { x: 5.0, y: 5.0 };

        let _ = dispatcher.dispatch(
            root.as_ref(),
            inside,
            &ElementEvent::PointerMove(PointerInfo::mouse(inside, PointerButton::Primary)),
        );
        let first_hit_tests = hit_tests.get();

        let outside = Vec2d { x: 50.0, y: 50.0 };
        let _ = dispatcher.dispatch(
            root.as_ref(),
            outside,
            &ElementEvent::PointerMove(PointerInfo::mouse(
                outside,
                PointerButton::Primary,
            )),
        );
        let outside_hit_tests = hit_tests.get();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            inside,
            &ElementEvent::PointerMove(PointerInfo::mouse(inside, PointerButton::Primary)),
        );

        assert!(outside_hit_tests > first_hit_tests);
        assert!(hit_tests.get() > outside_hit_tests);
        assert_eq!(leaf_events.get(), 2);
    }

    #[test]
    fn cached_hit_chain_is_invalidated_by_a_subtree_generation_change() {
        let hit_tests = Rc::new(Cell::new(0));
        let leaf_events = Rc::new(Cell::new(0));
        let root = HitChainCacheRoot {
            child: HitChainCacheLeaf {
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                events: leaf_events.clone(),
                consume_moves: false,
            }
            .boxed(),
            hit_tests: hit_tests.clone(),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let pos = Vec2d { x: 5.0, y: 5.0 };

        let _ = dispatcher.dispatch(
            root.as_ref(),
            pos,
            &ElementEvent::PointerMove(PointerInfo::mouse(pos, PointerButton::Primary)),
        );
        let first_hit_tests = hit_tests.get();

        let generation = root.subtree_generation();
        root.set_subtree_generation(generation.wrapping_add(1));
        let _ = dispatcher.dispatch(
            root.as_ref(),
            pos,
            &ElementEvent::PointerMove(PointerInfo::mouse(pos, PointerButton::Primary)),
        );

        assert!(hit_tests.get() > first_hit_tests);
        assert_eq!(leaf_events.get(), 2);
    }

    #[test]
    fn consuming_pointer_moves_are_not_cached() {
        let hit_tests = Rc::new(Cell::new(0));
        let leaf_events = Rc::new(Cell::new(0));
        let root = HitChainCacheRoot {
            child: HitChainCacheLeaf {
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                events: leaf_events.clone(),
                consume_moves: true,
            }
            .boxed(),
            hit_tests: hit_tests.clone(),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let first_pos = Vec2d { x: 5.0, y: 5.0 };
        let second_pos = Vec2d { x: 6.0, y: 6.0 };

        let _ = dispatcher.dispatch(
            root.as_ref(),
            first_pos,
            &ElementEvent::PointerMove(PointerInfo::mouse(
                first_pos,
                PointerButton::Primary,
            )),
        );
        let first_hit_tests = hit_tests.get();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            second_pos,
            &ElementEvent::PointerMove(PointerInfo::mouse(
                second_pos,
                PointerButton::Primary,
            )),
        );

        assert!(hit_tests.get() > first_hit_tests);
        assert_eq!(leaf_events.get(), 2);
    }

    #[test]
    fn cached_hit_chain_replays_a_forwarding_boundary_once() {
        let hit_tests = Rc::new(Cell::new(0));
        let forwarder_events = Rc::new(Cell::new(0));
        let leaf_events = Rc::new(Cell::new(0));
        let root = HitChainCacheRoot {
            child: HitChainCacheForwarder {
                child: HitChainCacheLeaf {
                    bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                    events: leaf_events.clone(),
                    consume_moves: false,
                }
                .boxed(),
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                events: forwarder_events.clone(),
            }
            .boxed(),
            hit_tests: hit_tests.clone(),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let first_pos = Vec2d { x: 5.0, y: 5.0 };
        let second_pos = Vec2d { x: 6.0, y: 6.0 };

        let _ = dispatcher.dispatch(
            root.as_ref(),
            first_pos,
            &ElementEvent::PointerMove(PointerInfo::mouse(
                first_pos,
                PointerButton::Primary,
            )),
        );
        let first_hit_tests = hit_tests.get();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            second_pos,
            &ElementEvent::PointerMove(PointerInfo::mouse(
                second_pos,
                PointerButton::Primary,
            )),
        );

        assert_eq!(hit_tests.get(), first_hit_tests);
        assert_eq!(forwarder_events.get(), 2);
        assert_eq!(leaf_events.get(), 2);
    }

    fn routed_leaf(
        bounds: (Vec2d, Vec2d),
        events: Rc<Cell<usize>>,
        capture_on_down: bool,
        release_on_move: bool,
    ) -> AnyElement {
        RoutedElement {
            children: Vec::new(),
            bounds: Some(bounds),
            events,
            capture_on_down,
            release_on_move,
        }
        .boxed()
    }

    struct ForwardOnlyHitTestElement {
        child: AnyElement,
    }

    impl VisitorElement for ForwardOnlyHitTestElement {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.child.as_ref());
        }

        fn debug_name(&self) -> &'static str {
            "ForwardOnlyHitTestElement"
        }
    }

    impl EventElement for ForwardOnlyHitTestElement {
        fn hit_test_children_at<'a>(
            &'a self,
            _pos: Vec2d,
            visitor: &mut dyn FnMut(&'a dyn Element),
        ) {
            visitor(self.child.as_ref());
        }

        fn hit_test_children_reversed<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {
            panic!("routed dispatch should use the shared forward hit-test scratch");
        }
    }

    impl LayoutElement for ForwardOnlyHitTestElement {}

    impl Drawable for ForwardOnlyHitTestElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for ForwardOnlyHitTestElement {}

    #[test]
    fn routed_dispatch_uses_position_aware_forward_scratch() {
        let events = Rc::new(Cell::new(0));
        let root = ForwardOnlyHitTestElement {
            child: routed_leaf(
                (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                events.clone(),
                false,
                false,
            ),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();

        let result = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::mouse(
                Vec2d { x: 5.0, y: 5.0 },
                PointerButton::Primary,
            )),
        );

        assert!(result.is_consumed());
        assert_eq!(events.get(), 1);
    }

    #[test]
    fn routed_hit_testing_does_not_descend_through_an_outside_parent() {
        let child_events = Rc::new(Cell::new(0));
        let child = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            child_events.clone(),
            false,
            false,
        );
        let parent = RoutedElement {
            children: vec![child],
            bounds: Some((
                Vec2d { x: 20.0, y: 20.0 },
                Vec2d { x: 30.0, y: 30.0 },
            )),
            events: Rc::new(Cell::new(0)),
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();
        let root = RoutedIdentityBranch(vec![parent]).boxed();
        let pos = Vec2d { x: 5.0, y: 5.0 };

        let _ = EventDispatcher::new().dispatch(
            root.as_ref(),
            pos,
            &ElementEvent::PointerDown(PointerInfo::mouse(pos, PointerButton::Primary)),
        );

        assert_eq!(child_events.get(), 0);
    }

    /// An element that carries a drag: it takes the pointer on press and then
    /// asks for the drag to be routed to whoever is underneath.
    struct DragCarrier {
        bounds: (Vec2d, Vec2d),
        follows_up: bool,
    }

    impl VisitorElement for DragCarrier {
        fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

        fn debug_name(&self) -> &'static str {
            "DragCarrier"
        }
    }

    impl EventElement for DragCarrier {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            match event {
                ElementEvent::PointerDown(pointer) => EventResult::consumed()
                    .with_pointer_capture(PointerKey::new(pointer.source, pointer.id)),
                ElementEvent::PointerMove(_) if self.follows_up => {
                    EventResult::consumed().with_follow_up(FollowUp::DragOver)
                }
                ElementEvent::PointerUp(_) if self.follows_up => {
                    EventResult::consumed().with_follow_up(FollowUp::DragDrop)
                }
                _ => EventResult::consumed(),
            }
        }
    }

    impl LayoutElement for DragCarrier {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }

    impl Drawable for DragCarrier {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for DragCarrier {}

    /// An element that counts the drags routed onto it.
    struct DragReceiver {
        bounds: (Vec2d, Vec2d),
        overs: Rc<Cell<usize>>,
        drops: Rc<Cell<usize>>,
    }

    impl VisitorElement for DragReceiver {
        fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

        fn debug_name(&self) -> &'static str {
            "DragReceiver"
        }
    }

    impl EventElement for DragReceiver {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            match event {
                ElementEvent::DragOver { .. } => {
                    self.overs.set(self.overs.get() + 1);
                    EventResult::consumed()
                }
                ElementEvent::DragDrop { .. } => {
                    self.drops.set(self.drops.get() + 1);
                    EventResult::consumed()
                }
                _ => EventResult::ignored(),
            }
        }
    }

    impl LayoutElement for DragReceiver {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }

    impl Drawable for DragReceiver {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for DragReceiver {}

    /// A carrier on the left, a receiver on the right, side by side and not
    /// overlapping — the shape of a card being dragged out of one column and
    /// onto another.
    fn drag_tree(
        follows_up: bool,
        overs: Rc<Cell<usize>>,
        drops: Rc<Cell<usize>>,
    ) -> (AnyElement, Vec2d, Vec2d) {
        let carrier = DragCarrier {
            bounds: (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            follows_up,
        }
        .boxed();
        let receiver = DragReceiver {
            bounds: (Vec2d { x: 20.0, y: 0.0 }, Vec2d { x: 30.0, y: 10.0 }),
            overs,
            drops,
        }
        .boxed();
        let root = RoutedIdentityBranch(vec![carrier, receiver]).boxed();

        (
            root,
            Vec2d { x: 5.0, y: 5.0 },
            Vec2d { x: 25.0, y: 5.0 },
        )
    }

    /// A parent that is transparent to hit testing: it has no bounds of its own
    /// and does not consume, so a routed pass reaches both of its children.
    struct RoutedIdentityBranch(Vec<AnyElement>);

    impl VisitorElement for RoutedIdentityBranch {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.0 {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "RoutedIdentityBranch"
        }
    }

    impl EventElement for RoutedIdentityBranch {}

    impl LayoutElement for RoutedIdentityBranch {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            None
        }
    }

    impl Drawable for RoutedIdentityBranch {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for RoutedIdentityBranch {}

    /// The element carrying a drag owns the pointer, so it is the only element
    /// that hears the move — which is exactly why the drag has to be routed
    /// separately to whoever is underneath.
    #[test]
    fn a_captured_drag_reaches_the_element_under_the_pointer() {
        let overs = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let (root, source, over_receiver) = drag_tree(true, overs.clone(), drops.clone());
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            source,
            &ElementEvent::PointerDown(PointerInfo::mouse(source, PointerButton::Primary)),
        );
        let _ = dispatcher.dispatch(
            root.as_ref(),
            over_receiver,
            &ElementEvent::PointerMove(PointerInfo::mouse(over_receiver, PointerButton::Primary)),
        );

        assert_eq!(overs.get(), 1, "the receiver was not told about the drag");
        assert_eq!(drops.get(), 0);
    }

    /// A drop is the end of the drag: it is delivered once, and the pointer
    /// goes back to routing normally.
    #[test]
    fn a_drop_is_delivered_once_and_releases_the_capture() {
        let overs = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let (root, source, over_receiver) = drag_tree(true, overs.clone(), drops.clone());
        let pointer = PointerKey::new(PointerSource::Mouse, 0);
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            source,
            &ElementEvent::PointerDown(PointerInfo::mouse(source, PointerButton::Primary)),
        );
        assert!(dispatcher.is_captured(pointer));

        let _ = dispatcher.dispatch(
            root.as_ref(),
            over_receiver,
            &ElementEvent::PointerUp(PointerInfo::mouse(over_receiver, PointerButton::Primary)),
        );

        assert_eq!(drops.get(), 1);
        assert!(!dispatcher.is_captured(pointer));
    }

    /// The cost of drag support for an application that never drags: a captured
    /// move that asks for nothing traverses nothing.
    #[test]
    fn a_capture_that_asks_for_no_follow_up_traverses_nothing() {
        let overs = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let (root, source, over_receiver) = drag_tree(false, overs.clone(), drops.clone());
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            source,
            &ElementEvent::PointerDown(PointerInfo::mouse(source, PointerButton::Primary)),
        );
        let _ = dispatcher.dispatch(
            root.as_ref(),
            over_receiver,
            &ElementEvent::PointerMove(PointerInfo::mouse(over_receiver, PointerButton::Primary)),
        );

        assert_eq!(overs.get(), 0);
    }

    #[test]
    fn focus_directed_text_reaches_elements_the_pointer_is_not_over() {
        let node = FocusNode::new();
        let events = Rc::new(Cell::new(0));
        let root = FocusKeyElement {
            node: node.clone(),
            bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
            key_events: events.clone(),
            consume_keys: Rc::new(Cell::new(true)),
        }
        .boxed();
        let outside = Vec2d {
            x: f32::MIN,
            y: f32::MIN,
        };

        let mut dispatcher = EventDispatcher::new();
        node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), outside, &ElementEvent::Cancel);
        let result = dispatcher.dispatch(
            root.as_ref(),
            outside,
            &ElementEvent::TextInput {
                text: "你好".into(),
                action: KeyAction::Pressed,
                modifiers: Modifiers::default(),
            },
        );

        assert!(result.is_consumed());
        assert_eq!(events.get(), 1);
    }

    #[test]
    fn named_keys_stay_positional() {
        let events = Rc::new(Cell::new(0));
        let leaf = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            events.clone(),
            false,
            false,
        );
        let root = RoutedElement {
            children: vec![leaf],
            bounds: Some((Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 })),
            events: Rc::new(Cell::new(0)),
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();

        let result = EventDispatcher::new().dispatch(
            root.as_ref(),
            Vec2d { x: 50.0, y: 50.0 },
            &ElementEvent::KeyInput {
                key: NamedKey::ArrowDown,
                action: KeyAction::Pressed,
                modifiers: Modifiers::default(),
            },
        );

        assert!(!result.is_consumed());
        assert_eq!(events.get(), 0);
    }

    #[test]
    fn focus_directed_delivery_stops_at_the_first_consumer() {
        let first_node = FocusNode::new();
        let first_events = Rc::new(Cell::new(0));
        let second_events = Rc::new(Cell::new(0));
        let root = IdentityBranch(vec![
            FocusKeyElement {
                node: first_node.clone(),
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                key_events: first_events.clone(),
                consume_keys: Rc::new(Cell::new(true)),
            }
            .boxed(),
            FocusKeyElement {
                node: FocusNode::new(),
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                key_events: second_events.clone(),
                consume_keys: Rc::new(Cell::new(true)),
            }
            .boxed(),
        ])
        .boxed();

        let mut dispatcher = EventDispatcher::new();
        first_node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::ImePreedit {
                text: "ni".into(),
                cursor: None,
            },
        );

        assert_eq!(first_events.get(), 1);
        assert_eq!(second_events.get(), 0);
    }

    #[test]
    fn captured_dispatch_visits_only_the_saved_path() {
        let target_events = Rc::new(Cell::new(0));
        let unrelated_events = Rc::new(Cell::new(0));
        let target = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            target_events.clone(),
            true,
            false,
        );
        let target_id = target.id();
        let mut children = Vec::new();
        for _ in 0..128 {
            children.push(routed_leaf(
                (Vec2d { x: 20.0, y: 20.0 }, Vec2d { x: 30.0, y: 30.0 }),
                unrelated_events.clone(),
                false,
                false,
            ));
        }
        children.push(target);
        let root = RoutedElement {
            children,
            bounds: None,
            events: Rc::new(Cell::new(0)),
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let touch = PointerKey::new(PointerSource::Touch, 9);

        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                touch.source,
                touch.id,
                PointerButton::Primary,
            )),
        );
        target_events.set(0);
        unrelated_events.set(0);

        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 500.0, y: 500.0 },
            &ElementEvent::PointerMove(PointerInfo::new(
                Vec2d { x: 500.0, y: 500.0 },
                touch.source,
                touch.id,
                PointerButton::Primary,
            )),
        );

        assert_eq!(dispatcher.captured_owner(touch), Some(target_id));
        assert_eq!(target_events.get(), 1);
        assert_eq!(unrelated_events.get(), 0);
    }

    #[test]
    fn equal_mouse_and_touch_ids_capture_independently() {
        let mouse_events = Rc::new(Cell::new(0));
        let touch_events = Rc::new(Cell::new(0));
        let mouse = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            mouse_events,
            true,
            false,
        );
        let mouse_id = mouse.id();
        let touch = routed_leaf(
            (Vec2d { x: 20.0, y: 20.0 }, Vec2d { x: 30.0, y: 30.0 }),
            touch_events,
            true,
            false,
        );
        let touch_id = touch.id();
        let root = RoutedElement {
            children: vec![mouse, touch],
            bounds: None,
            events: Rc::new(Cell::new(0)),
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let mouse_pointer = PointerKey::new(PointerSource::Mouse, 0);
        let touch_pointer = PointerKey::new(PointerSource::Touch, 0);

        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                mouse_pointer.source,
                mouse_pointer.id,
                PointerButton::Primary,
            )),
        );
        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 25.0, y: 25.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 25.0, y: 25.0 },
                touch_pointer.source,
                touch_pointer.id,
                PointerButton::Primary,
            )),
        );

        assert_eq!(dispatcher.captured_owner(mouse_pointer), Some(mouse_id));
        assert_eq!(dispatcher.captured_owner(touch_pointer), Some(touch_id));
    }

    #[test]
    fn explicit_release_request_clears_capture() {
        let events = Rc::new(Cell::new(0));
        let target = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            events,
            true,
            true,
        );
        let pointer = PointerKey::new(PointerSource::Mouse, 0);
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            target.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );
        let _ = dispatcher.dispatch(
            target.as_ref(),
            Vec2d { x: 50.0, y: 50.0 },
            &ElementEvent::PointerMove(PointerInfo::new(
                Vec2d { x: 50.0, y: 50.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );

        assert_eq!(dispatcher.captured_owner(pointer), None);
    }

    #[test]
    fn compatible_rebuild_preserves_active_capture() {
        let old_target_events = Rc::new(Cell::new(0));
        let old = RoutedElement {
            children: vec![routed_leaf(
                (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
                old_target_events,
                true,
                false,
            )],
            bounds: None,
            events: Rc::new(Cell::new(0)),
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();
        let pointer = PointerKey::new(PointerSource::Touch, 3);
        let mut dispatcher = EventDispatcher::new();
        let _ = dispatcher.dispatch(
            old.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );
        let owner = dispatcher.captured_owner(pointer);

        let rebuilt_target_events = Rc::new(Cell::new(0));
        let rebuilt = RoutedElement {
            children: vec![routed_leaf(
                (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
                rebuilt_target_events.clone(),
                true,
                false,
            )],
            bounds: None,
            events: Rc::new(Cell::new(0)),
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();
        reconcile_generated_tree(old.as_ref(), rebuilt.as_ref());

        let _ = dispatcher.dispatch(
            rebuilt.as_ref(),
            Vec2d { x: 50.0, y: 50.0 },
            &ElementEvent::PointerMove(PointerInfo::new(
                Vec2d { x: 50.0, y: 50.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );

        assert_eq!(dispatcher.captured_owner(pointer), owner);
        assert_eq!(rebuilt_target_events.get(), 1);
    }

    struct CaptureChainElement {
        child: Option<AnyElement>,
        pointer: PointerKey,
    }

    impl VisitorElement for CaptureChainElement {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            if let Some(child) = &self.child {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "CaptureChainElement"
        }
    }

    impl EventElement for CaptureChainElement {
        fn on_event(&self, _event: &ElementEvent) -> EventResult {
            EventResult::ignored().with_pointer_capture(self.pointer)
        }
    }

    impl LayoutElement for CaptureChainElement {}
    impl Drawable for CaptureChainElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for CaptureChainElement {}

    #[test]
    fn deepest_capture_request_wins() {
        let pointer = PointerKey::new(PointerSource::Touch, 6);
        let child = CaptureChainElement {
            child: None,
            pointer,
        }
        .boxed();
        let child_id = child.id();
        let root = CaptureChainElement {
            child: Some(child),
            pointer,
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d::default(),
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d::default(),
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );

        assert_eq!(dispatcher.captured_owner(pointer), Some(child_id));
    }

    #[test]
    fn pointer_up_and_cancel_release_captures() {
        let events = Rc::new(Cell::new(0));
        let target = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            events,
            true,
            false,
        );
        let root = RoutedElement {
            children: vec![target],
            bounds: None,
            events: Rc::new(Cell::new(0)),
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let mouse = PointerKey::new(PointerSource::Mouse, 0);
        let touch = PointerKey::new(PointerSource::Touch, 7);

        for pointer in [mouse, touch] {
            let _ = dispatcher.dispatch(
                root.as_ref(),
                Vec2d { x: 5.0, y: 5.0 },
                &ElementEvent::PointerDown(PointerInfo::new(
                    Vec2d { x: 5.0, y: 5.0 },
                    pointer.source,
                    pointer.id,
                    PointerButton::Primary,
                )),
            );
        }
        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 50.0, y: 50.0 },
            &ElementEvent::PointerUp(PointerInfo::new(
                Vec2d { x: 50.0, y: 50.0 },
                mouse.source,
                mouse.id,
                PointerButton::Primary,
            )),
        );
        assert_eq!(dispatcher.captured_owner(mouse), None);
        assert!(dispatcher.captured_owner(touch).is_some());

        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        assert_eq!(dispatcher.capture_count(), 0);
    }

    #[test]
    fn removed_owner_clears_capture_without_falling_back() {
        let old_events = Rc::new(Cell::new(0));
        let old = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            old_events,
            true,
            false,
        );
        let mut dispatcher = EventDispatcher::new();
        let pointer = PointerKey::new(PointerSource::Touch, 5);
        let _ = dispatcher.dispatch(
            old.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );

        let replacement = ReplacementLeaf.boxed();
        reconcile_generated_tree(old.as_ref(), replacement.as_ref());
        let result = dispatcher.dispatch(
            replacement.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerMove(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );

        assert_eq!(dispatcher.captured_owner(pointer), None);
        assert_eq!(result, EventResult::ignored());
    }

    #[test]
    fn invalid_saved_path_clears_capture_without_falling_back() {
        let events = Rc::new(Cell::new(0));
        let target = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            events.clone(),
            true,
            false,
        );
        let pointer = PointerKey::new(PointerSource::Touch, 11);
        let mut dispatcher = EventDispatcher::new();
        let _ = dispatcher.dispatch(
            target.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );
        let owner = dispatcher
            .captured_owner(pointer)
            .expect("pointer must be captured after down");
        dispatcher
            .path_indices
            .insert(owner, usize::MAX);
        events.set(0);

        let move_event = ElementEvent::PointerMove(PointerInfo::new(
            Vec2d { x: 5.0, y: 5.0 },
            pointer.source,
            pointer.id,
            PointerButton::Primary,
        ));
        let result = dispatcher.dispatch_captured(target.as_ref(), pointer, &move_event);

        assert_eq!(result, EventResult::ignored());
        assert_eq!(dispatcher.captured_owner(pointer), None);
        assert_eq!(events.get(), 0);
    }

    struct CapturingElement {
        events: Cell<usize>,
    }

    impl VisitorElement for CapturingElement {
        fn debug_name(&self) -> &'static str {
            "CapturingElement"
        }
    }

    impl EventElement for CapturingElement {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            match event {
                ElementEvent::PointerDown(pointer) if pointer.id == 7 => {
                    EventResult::consumed()
                        .with_pointer_capture(PointerKey::new(pointer.source, 7))
                }
                _ => EventResult::consumed(),
            }
        }
    }

    impl LayoutElement for CapturingElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }))
        }
    }

    impl Drawable for CapturingElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for CapturingElement {}

    struct TreeElement {
        children: Vec<AnyElement>,
        events: Rc<Cell<usize>>,
        role: EventTreeRole,
        bounds: Option<Rc<Cell<Option<(Vec2d, Vec2d)>>>>,
    }

    impl VisitorElement for TreeElement {
        fn debug_name(&self) -> &'static str {
            "TreeElement"
        }
    }

    impl EventElement for TreeElement {
        fn event_tree_role(&self) -> EventTreeRole {
            self.role
        }

        fn on_event(&self, _event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            EventResult::consumed()
        }

        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }
    }

    impl LayoutElement for TreeElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            self.bounds.as_ref().and_then(|bounds| bounds.get())
        }
    }

    impl Drawable for TreeElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for TreeElement {}

    struct EffectTreeElement {
        children: Vec<EffectTreeElement>,
        result: EventResult,
    }

    impl VisitorElement for EffectTreeElement {
        fn debug_name(&self) -> &'static str {
            "EffectTreeElement"
        }
    }

    impl EventElement for EffectTreeElement {
        fn on_event(&self, _event: &ElementEvent) -> EventResult {
            self.result
        }

        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child);
            }
        }
    }

    impl LayoutElement for EffectTreeElement {}
    impl Drawable for EffectTreeElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for EffectTreeElement {}

    #[test]
    fn dispatch_preserves_non_consuming_child_effects() {
        let element = EffectTreeElement {
            children: vec![EffectTreeElement {
                children: Vec::new(),
                result: EventResult::redraw(),
            }],
            result: EventResult::ignored(),
        };

        let result = dispatch_event(&element, Vec2d { x: 5.0, y: 5.0 }, &ElementEvent::Cancel);

        assert!(!result.is_consumed());
        assert!(result.needs_redraw());
    }

    #[test]
    fn captured_pointer_move_is_delivered_outside_element_bounds() {
        let events = Rc::new(Cell::new(0));
        let element = CapturingElement {
            events: Cell::new(0),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let _ = dispatcher.dispatch(
            element.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::touch(Vec2d { x: 5.0, y: 5.0 }, 7)),
        );
        let event = ElementEvent::PointerMove(PointerInfo::touch(Vec2d { x: 50.0, y: 50.0 }, 7));

        assert!(
            dispatcher
                .dispatch(element.as_ref(), Vec2d { x: 50.0, y: 50.0 }, &event)
                .is_consumed()
        );
        let _ = events;
    }

    #[test]
    fn cancel_pointer_reaches_captured_element_outside_bounds() {
        let element = CapturingElement {
            events: Cell::new(0),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let pointer = PointerKey::new(PointerSource::Touch, 7);
        let _ = dispatcher.dispatch(
            element.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );

        assert!(
            dispatcher
                .cancel_pointer(element.as_ref(), pointer)
                .is_consumed()
        );
        assert_eq!(dispatcher.capture_count(), 0);
    }

    #[test]
    fn cancel_pointer_without_capture_is_ignored() {
        let element = CapturingElement {
            events: Cell::new(0),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();

        assert_eq!(
            dispatcher.cancel_pointer(element.as_ref(), PointerKey::new(PointerSource::Touch, 8),),
            EventResult::ignored()
        );
    }

    #[test]
    fn cancel_pointer_is_not_delivered_twice_to_captured_target() {
        let element = CapturingElement {
            events: Cell::new(0),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let pointer = PointerKey::new(PointerSource::Touch, 7);
        let _ = dispatcher.dispatch(
            element.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );

        assert!(
            dispatcher
                .cancel_pointer(element.as_ref(), pointer)
                .is_consumed()
        );
        assert_eq!(
            dispatcher.cancel_pointer(element.as_ref(), pointer),
            EventResult::ignored()
        );
    }

    #[test]
    fn recursive_dispatch_helpers_reuse_inline_scratch_without_spilling() {
        let root_events = Rc::new(Cell::new(0));
        let transparent_events = Rc::new(Cell::new(0));
        let target_events = Rc::new(Cell::new(0));
        let element = TreeElement {
            children: vec![TreeElement {
                children: vec![TreeElement {
                    children: Vec::new(),
                    events: target_events.clone(),
                    role: EventTreeRole::IndexedTarget,
                    bounds: None,
                }
                .boxed()],
                events: transparent_events.clone(),
                role: EventTreeRole::Transparent,
                bounds: None,
            }
            .boxed()],
            events: root_events.clone(),
            role: EventTreeRole::IndexedTarget,
            bounds: None,
        };
        let event = ElementEvent::Cancel;
        let mut children = EventChildren::new();

        assert!(
            dispatch_event_inner(&element, Vec2d { x: 5.0, y: 5.0 }, &event, &mut children)
                .is_consumed()
        );
        assert!(children.is_empty());
        assert!(!children.spilled());

        assert!(broadcast_event_inner(&element, &event, &mut children).is_consumed());
        assert!(children.is_empty());
        assert!(!children.spilled());
        assert_eq!(root_events.get(), 1);
        if cfg!(feature = "event-tree-exp") {
            assert_eq!(transparent_events.get(), 0);
            assert_eq!(target_events.get(), 2);
        } else {
            assert_eq!(transparent_events.get(), 1);
            assert_eq!(target_events.get(), 2);
        }
    }

    #[cfg(feature = "event-tree-exp")]
    #[test]
    fn indexed_dispatch_flattens_transparent_nodes_and_keeps_target_order() {
        let root_events = Rc::new(Cell::new(0));
        let transparent_events = Rc::new(Cell::new(0));
        let target_events = Rc::new(Cell::new(0));
        let target = TreeElement {
            children: Vec::new(),
            events: target_events.clone(),
            role: EventTreeRole::IndexedTarget,
            bounds: None,
        }
        .boxed();
        let transparent = TreeElement {
            children: vec![target],
            events: transparent_events.clone(),
            role: EventTreeRole::Transparent,
            bounds: None,
        }
        .boxed();
        let root = TreeElement {
            children: vec![transparent],
            events: root_events.clone(),
            role: EventTreeRole::IndexedTarget,
            bounds: None,
        }
        .boxed();

        let mut dispatcher = EventDispatcher::new();
        let result = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 1.0, y: 1.0 },
            &ElementEvent::DragOver {
                pos: Vec2d { x: 1.0, y: 1.0 },
                source: PointerSource::Touch,
                id: 1,
            },
        );

        assert!(result.is_consumed());
        assert_eq!(target_events.get(), 1);
        assert_eq!(transparent_events.get(), 0);
        assert_eq!(root_events.get(), 0);
    }

    #[cfg(feature = "event-tree-exp")]
    struct IndexedHitTestBoundaryRoot {
        children: Vec<AnyElement>,
        selected_child: ElementId,
        alternate_child: Option<ElementId>,
    }

    #[cfg(feature = "event-tree-exp")]
    impl VisitorElement for IndexedHitTestBoundaryRoot {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "IndexedHitTestBoundaryRoot"
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl EventElement for IndexedHitTestBoundaryRoot {
        fn event_tree_role(&self) -> EventTreeRole {
            EventTreeRole::IndexedHitTestBoundary
        }

        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            self.visit_children(visitor);
        }

        fn hit_test_children_at<'a>(
            &'a self,
            pos: Vec2d,
            visitor: &mut dyn FnMut(&'a dyn Element),
        ) {
            let selected_child = if pos.x >= 5.0 {
                self.alternate_child.unwrap_or(self.selected_child)
            } else {
                self.selected_child
            };
            for child in &self.children {
                if child.id() == selected_child {
                    visitor(child.as_ref());
                }
            }
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl LayoutElement for IndexedHitTestBoundaryRoot {}

    #[cfg(feature = "event-tree-exp")]
    impl Drawable for IndexedHitTestBoundaryRoot {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    #[cfg(feature = "event-tree-exp")]
    impl Rebuildable for IndexedHitTestBoundaryRoot {}

    #[cfg(feature = "event-tree-exp")]
    struct IndexedBoundaryRouteTarget {
        id: usize,
        events: Rc<RefCell<Vec<usize>>>,
    }

    #[cfg(feature = "event-tree-exp")]
    impl VisitorElement for IndexedBoundaryRouteTarget {
        fn debug_name(&self) -> &'static str {
            "IndexedBoundaryRouteTarget"
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl EventElement for IndexedBoundaryRouteTarget {
        fn event_tree_role(&self) -> EventTreeRole {
            EventTreeRole::IndexedTarget
        }

        fn on_event(&self, _event: &ElementEvent) -> EventResult {
            self.events.borrow_mut().push(self.id);
            EventResult::ignored()
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl LayoutElement for IndexedBoundaryRouteTarget {}

    #[cfg(feature = "event-tree-exp")]
    impl Drawable for IndexedBoundaryRouteTarget {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    #[cfg(feature = "event-tree-exp")]
    impl Rebuildable for IndexedBoundaryRouteTarget {}

    #[cfg(feature = "event-tree-exp")]
    #[test]
    fn indexed_hit_test_boundary_filters_positions_but_not_broadcasts() {
        let hits = Rc::new(RefCell::new(Vec::new()));
        let first_target = IndexedBoundaryRouteTarget {
            id: 1,
            events: hits.clone(),
        }
        .boxed();
        let third_target = IndexedBoundaryRouteTarget {
            id: 3,
            events: hits.clone(),
        }
        .boxed();
        let first_branch = TreeElement {
            children: vec![first_target, third_target],
            events: Rc::new(Cell::new(0)),
            role: EventTreeRole::Transparent,
            bounds: None,
        }
        .boxed();
        let selected_child = first_branch.id();
        let second_target = IndexedBoundaryRouteTarget {
            id: 2,
            events: hits.clone(),
        }
        .boxed();
        let second_branch = TreeElement {
            children: vec![second_target],
            events: Rc::new(Cell::new(0)),
            role: EventTreeRole::Transparent,
            bounds: None,
        }
        .boxed();
        let root = IndexedHitTestBoundaryRoot {
            children: vec![first_branch, second_branch],
            selected_child,
            alternate_child: None,
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        dispatcher.synchronize_paths(root.as_ref());
        assert!(dispatcher.event_tree_eligible);

        let event = ElementEvent::DragOver {
            pos: Vec2d { x: 1.0, y: 1.0 },
            source: PointerSource::Mouse,
            id: 1,
        };
        assert_eq!(
            dispatcher.dispatch(root.as_ref(), Vec2d { x: 1.0, y: 1.0 }, &event),
            EventResult::ignored()
        );
        assert_eq!(*hits.borrow(), [3, 1]);

        hits.borrow_mut().clear();
        assert_eq!(broadcast_event(root.as_ref(), &ElementEvent::Cancel), EventResult::ignored());
        assert_eq!(*hits.borrow(), [2, 3, 1]);
    }

    #[cfg(feature = "event-tree-exp")]
    struct IndexedBoundaryHoverTarget {
        id: usize,
        pointer_moves: Rc<RefCell<Vec<usize>>>,
    }

    #[cfg(feature = "event-tree-exp")]
    impl VisitorElement for IndexedBoundaryHoverTarget {
        fn debug_name(&self) -> &'static str {
            "IndexedBoundaryHoverTarget"
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl EventElement for IndexedBoundaryHoverTarget {
        fn event_tree_role(&self) -> EventTreeRole {
            EventTreeRole::IndexedTarget
        }

        fn on_event(&self, event: &ElementEvent) -> EventResult {
            if matches!(event, ElementEvent::PointerMove(_)) {
                self.pointer_moves.borrow_mut().push(self.id);
            }
            EventResult::ignored()
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl LayoutElement for IndexedBoundaryHoverTarget {}

    #[cfg(feature = "event-tree-exp")]
    impl Drawable for IndexedBoundaryHoverTarget {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    #[cfg(feature = "event-tree-exp")]
    impl Rebuildable for IndexedBoundaryHoverTarget {}

    #[cfg(feature = "event-tree-exp")]
    #[test]
    fn indexed_hit_test_boundary_rechecks_pointer_moves_instead_of_replaying_a_stale_chain() {
        let pointer_moves = Rc::new(RefCell::new(Vec::new()));
        let first_target = IndexedBoundaryHoverTarget {
            id: 1,
            pointer_moves: pointer_moves.clone(),
        }
        .boxed();
        let first_branch = TreeElement {
            children: vec![first_target],
            events: Rc::new(Cell::new(0)),
            role: EventTreeRole::Transparent,
            bounds: None,
        }
        .boxed();
        let first_branch_id = first_branch.id();
        let second_target = IndexedBoundaryHoverTarget {
            id: 2,
            pointer_moves: pointer_moves.clone(),
        }
        .boxed();
        let second_branch = TreeElement {
            children: vec![second_target],
            events: Rc::new(Cell::new(0)),
            role: EventTreeRole::Transparent,
            bounds: None,
        }
        .boxed();
        let second_branch_id = second_branch.id();
        let root = IndexedHitTestBoundaryRoot {
            children: vec![first_branch, second_branch],
            selected_child: first_branch_id,
            alternate_child: Some(second_branch_id),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();

        for pos in [Vec2d { x: 1.0, y: 1.0 }, Vec2d { x: 10.0, y: 1.0 }] {
            let event = ElementEvent::PointerMove(PointerInfo::mouse(
                pos,
                PointerButton::Primary,
            ));
            let _ = dispatcher.dispatch(root.as_ref(), pos, &event);
        }

        assert_eq!(*pointer_moves.borrow(), [1, 2]);
    }

    #[cfg(feature = "event-tree-exp")]
    struct DynamicBoundsTarget {
        visits: Rc<Cell<usize>>,
    }

    #[cfg(feature = "event-tree-exp")]
    impl VisitorElement for DynamicBoundsTarget {
        fn debug_name(&self) -> &'static str {
            "DynamicBoundsTarget"
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl EventElement for DynamicBoundsTarget {
        fn event_tree_role(&self) -> EventTreeRole {
            EventTreeRole::IndexedTarget
        }

        fn on_event(&self, _event: &ElementEvent) -> EventResult {
            self.visits.set(self.visits.get() + 1);
            EventResult::ignored()
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl LayoutElement for DynamicBoundsTarget {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((Vec2d::ZERO, Vec2d { x: 10.0, y: 10.0 }))
        }

        fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
            None
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl Drawable for DynamicBoundsTarget {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    #[cfg(feature = "event-tree-exp")]
    impl Rebuildable for DynamicBoundsTarget {}

    #[cfg(feature = "event-tree-exp")]
    #[test]
    fn event_tree_keeps_dynamic_bounds_as_candidates_but_checks_live_bounds() {
        let visits = Rc::new(Cell::new(0));
        let root = DynamicBoundsTarget {
            visits: visits.clone(),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        dispatcher.synchronize_paths(root.as_ref());

        assert!(dispatcher.event_tree_eligible);
        assert_eq!(dispatcher.event_tree.hit_test(50.0, 50.0).count(), 1);

        let pos = Vec2d { x: 50.0, y: 50.0 };
        let result = dispatcher.dispatch(
            root.as_ref(),
            pos,
            &ElementEvent::PointerMove(PointerInfo::mouse(pos, PointerButton::Primary)),
        );
        assert_eq!(result, EventResult::ignored());
        assert_eq!(visits.get(), 0);
    }

    #[cfg(feature = "event-tree-exp")]
    #[test]
    fn indexed_bounds_rebuild_after_layout_invalidation() {
        let target_events = Rc::new(Cell::new(0));
        let bounds = Rc::new(Cell::new(Some((
            Vec2d { x: 0.0, y: 0.0 },
            Vec2d { x: 10.0, y: 10.0 },
        ))));
        let target = TreeElement {
            children: Vec::new(),
            events: target_events.clone(),
            role: EventTreeRole::IndexedTarget,
            bounds: Some(bounds.clone()),
        }
        .boxed();
        let root = TreeElement {
            children: vec![target],
            events: Rc::new(Cell::new(0)),
            role: EventTreeRole::Transparent,
            bounds: None,
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let event = ElementEvent::DragOver {
            pos: Vec2d { x: 5.0, y: 5.0 },
            source: PointerSource::Touch,
            id: 2,
        };

        assert!(dispatcher.dispatch(root.as_ref(), Vec2d { x: 5.0, y: 5.0 }, &event).is_consumed());
        bounds.set(Some((
            Vec2d { x: 40.0, y: 40.0 },
            Vec2d { x: 50.0, y: 50.0 },
        )));
        advance_layout_invalidation_generation();
        assert!(!dispatcher.dispatch(root.as_ref(), Vec2d { x: 5.0, y: 5.0 }, &event).is_consumed());
        let moved_event = ElementEvent::DragOver {
            pos: Vec2d { x: 45.0, y: 45.0 },
            source: PointerSource::Touch,
            id: 2,
        };
        assert!(dispatcher.dispatch(root.as_ref(), Vec2d { x: 45.0, y: 45.0 }, &moved_event).is_consumed());
        assert_eq!(target_events.get(), 2);
    }

    #[cfg(feature = "event-tree-exp")]
    #[test]
    fn indexed_event_tree_rebuilds_after_a_subtree_replacement() {
        let first_events = Rc::new(Cell::new(0));
        let next_events = Rc::new(Cell::new(0));
        let first = TreeElement {
            children: Vec::new(),
            events: first_events.clone(),
            role: EventTreeRole::IndexedTarget,
            bounds: None,
        }
        .boxed();
        let next = TreeElement {
            children: Vec::new(),
            events: next_events.clone(),
            role: EventTreeRole::IndexedTarget,
            bounds: None,
        }
        .boxed();
        let root = SwitchingEventRoot {
            first,
            next,
            use_next: Cell::new(false),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let event = ElementEvent::DragOver {
            pos: Vec2d { x: 5.0, y: 5.0 },
            source: PointerSource::Touch,
            id: 3,
        };

        assert!(dispatcher.dispatch(root.as_ref(), Vec2d { x: 5.0, y: 5.0 }, &event).is_consumed());
        root.as_ref()
            .option_any()
            .expect("switching root remains downcastable")
            .downcast_ref::<SwitchingEventRoot>()
            .expect("the indexed test root keeps its concrete type")
            .use_next
            .set(true);
        advance_element_tree_generation();

        assert!(dispatcher.dispatch(root.as_ref(), Vec2d { x: 5.0, y: 5.0 }, &event).is_consumed());
        assert_eq!(first_events.get(), 1);
        assert_eq!(next_events.get(), 1);
    }

    #[cfg(feature = "event-tree-exp")]
    struct SwitchingEventRoot {
        first: AnyElement,
        next: AnyElement,
        use_next: Cell<bool>,
    }

    #[cfg(feature = "event-tree-exp")]
    impl VisitorElement for SwitchingEventRoot {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            let child = if self.use_next.get() {
                &self.next
            } else {
                &self.first
            };
            visitor(child.as_ref());
        }

        fn debug_name(&self) -> &'static str {
            "SwitchingEventRoot"
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl EventElement for SwitchingEventRoot {
        fn event_tree_role(&self) -> EventTreeRole {
            EventTreeRole::Transparent
        }

        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            self.visit_children(visitor);
        }
    }

    #[cfg(feature = "event-tree-exp")]
    impl LayoutElement for SwitchingEventRoot {}

    #[cfg(feature = "event-tree-exp")]
    impl Drawable for SwitchingEventRoot {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    #[cfg(feature = "event-tree-exp")]
    impl Rebuildable for SwitchingEventRoot {
        fn option_any(&self) -> Option<&dyn Any> {
            Some(self)
        }
    }

    struct FocusTestElement {
        node: FocusNode,
        lifecycle: Rc<RefCell<Vec<&'static str>>>,
        autofocus: bool,
    }

    impl VisitorElement for FocusTestElement {
        fn debug_name(&self) -> &'static str {
            "FocusTestElement"
        }
    }

    impl EventElement for FocusTestElement {
        fn focus_node(&self) -> Option<&FocusNode> {
            Some(&self.node)
        }

        fn autofocus(&self) -> bool {
            self.autofocus
        }

        fn on_event(&self, event: &ElementEvent) -> EventResult {
            match event {
                ElementEvent::FocusGained => self.lifecycle.borrow_mut().push("focus"),
                ElementEvent::FocusLost => self.lifecycle.borrow_mut().push("blur"),
                _ => {}
            }
            EventResult::ignored()
        }
    }

    impl LayoutElement for FocusTestElement {}
    impl Drawable for FocusTestElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for FocusTestElement {}

    #[test]
    fn imperative_focus_is_exclusive_and_emits_each_lifecycle_event_once() {
        let first_node = FocusNode::new();
        let second_node = FocusNode::new();
        let first_lifecycle = Rc::new(RefCell::new(Vec::new()));
        let second_lifecycle = Rc::new(RefCell::new(Vec::new()));
        let root = IdentityBranch(vec![
            FocusTestElement {
                node: first_node.clone(),
                lifecycle: first_lifecycle.clone(),
                autofocus: false,
            }
            .boxed(),
            FocusTestElement {
                node: second_node.clone(),
                lifecycle: second_lifecycle.clone(),
                autofocus: false,
            }
            .boxed(),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();

        first_node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        second_node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        assert!(!first_node.has_focus());
        assert!(second_node.has_focus());
        assert_eq!(&*first_lifecycle.borrow(), &["focus", "blur"]);
        assert_eq!(&*second_lifecycle.borrow(), &["focus"]);
    }

    #[test]
    fn removing_the_focused_element_clears_focus_and_emits_one_blur() {
        let node = FocusNode::new();
        let lifecycle = Rc::new(RefCell::new(Vec::new()));
        let old = FocusTestElement {
            node: node.clone(),
            lifecycle: lifecycle.clone(),
            autofocus: false,
        }
        .boxed();
        let replacement = ReplacementLeaf.boxed();
        let mut dispatcher = EventDispatcher::new();
        node.request_focus();
        let _ = dispatcher.dispatch(old.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        reconcile_generated_tree(old.as_ref(), replacement.as_ref());

        assert!(!node.has_focus());
        assert_eq!(&*lifecycle.borrow(), &["focus", "blur"]);
    }

    #[test]
    fn first_autofocus_node_in_tree_order_wins_conflicts() {
        let first_node = FocusNode::new();
        let second_node = FocusNode::new();
        let first_lifecycle = Rc::new(RefCell::new(Vec::new()));
        let second_lifecycle = Rc::new(RefCell::new(Vec::new()));
        let root = IdentityBranch(vec![
            FocusTestElement {
                node: first_node.clone(),
                lifecycle: first_lifecycle.clone(),
                autofocus: true,
            }
            .boxed(),
            FocusTestElement {
                node: second_node.clone(),
                lifecycle: second_lifecycle.clone(),
                autofocus: true,
            }
            .boxed(),
        ])
        .boxed();

        let _ = EventDispatcher::new().dispatch(
            root.as_ref(),
            Vec2d::default(),
            &ElementEvent::Cancel,
        );

        assert!(first_node.has_focus());
        assert!(!second_node.has_focus());
        assert_eq!(&*first_lifecycle.borrow(), &["focus"]);
        assert!(second_lifecycle.borrow().is_empty());
    }

    #[test]
    fn pointer_down_outside_focusable_elements_blurs_the_owner_once() {
        let node = FocusNode::new();
        let lifecycle = Rc::new(RefCell::new(Vec::new()));
        let outside_events = Rc::new(Cell::new(0));
        let root = IdentityBranch(vec![
            FocusTestElement {
                node: node.clone(),
                lifecycle: lifecycle.clone(),
                autofocus: false,
            }
            .boxed(),
            routed_leaf(
                (
                    Vec2d { x: 20.0, y: 20.0 },
                    Vec2d { x: 30.0, y: 30.0 },
                ),
                outside_events,
                false,
                false,
            ),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 25.0, y: 25.0 },
            &ElementEvent::PointerDown(PointerInfo::mouse(
                Vec2d { x: 25.0, y: 25.0 },
                PointerButton::Primary,
            )),
        );

        assert!(!node.has_focus());
        assert_eq!(&*lifecycle.borrow(), &["focus", "blur"]);
    }

    #[test]
    fn tab_and_shift_tab_traverse_focusable_elements_in_structural_order() {
        let first_node = FocusNode::new();
        let second_node = FocusNode::new();
        let first_lifecycle = Rc::new(RefCell::new(Vec::new()));
        let second_lifecycle = Rc::new(RefCell::new(Vec::new()));
        let root = IdentityBranch(vec![
            FocusTestElement {
                node: first_node.clone(),
                lifecycle: first_lifecycle,
                autofocus: false,
            }
            .boxed(),
            FocusTestElement {
                node: second_node.clone(),
                lifecycle: second_lifecycle,
                autofocus: false,
            }
            .boxed(),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        first_node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        let forward = dispatcher.dispatch(
            root.as_ref(),
            Vec2d::default(),
            &ElementEvent::KeyInput {
                key: NamedKey::Tab,
                action: KeyAction::Pressed,
                modifiers: Modifiers::default(),
            },
        );
        let backward = dispatcher.dispatch(
            root.as_ref(),
            Vec2d::default(),
            &ElementEvent::KeyInput {
                key: NamedKey::Tab,
                action: KeyAction::Pressed,
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            },
        );

        assert!(forward.is_consumed());
        assert!(backward.is_consumed());
        assert!(first_node.has_focus());
        assert!(!second_node.has_focus());
    }

    struct FocusScopeElement {
        children: Vec<AnyElement>,
        traps: Cell<bool>,
    }

    impl VisitorElement for FocusScopeElement {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "FocusScopeElement"
        }
    }

    impl EventElement for FocusScopeElement {
        fn traps_focus(&self) -> bool {
            self.traps.get()
        }
    }

    impl LayoutElement for FocusScopeElement {}
    impl Drawable for FocusScopeElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for FocusScopeElement {
        fn option_any(&self) -> Option<&dyn Any> {
            Some(self)
        }
    }

    fn focusable(node: &FocusNode) -> AnyElement {
        FocusTestElement {
            node: node.clone(),
            lifecycle: Rc::new(RefCell::new(Vec::new())),
            autofocus: false,
        }
        .boxed()
    }

    fn tab(dispatcher: &mut EventDispatcher, root: &dyn Element) -> EventResult {
        dispatcher.dispatch(
            root,
            Vec2d::default(),
            &ElementEvent::KeyInput {
                key: NamedKey::Tab,
                action: KeyAction::Pressed,
                modifiers: Modifiers::default(),
            },
        )
    }

    #[test]
    fn tab_inside_a_trapping_scope_never_leaves_it() {
        let outside = FocusNode::new();
        let first = FocusNode::new();
        let second = FocusNode::new();
        let root = IdentityBranch(vec![
            focusable(&outside),
            FocusScopeElement {
                children: vec![focusable(&first), focusable(&second)],
                traps: Cell::new(true),
            }
            .boxed(),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        advance_element_tree_generation();

        first.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        assert!(first.has_focus());

        assert!(tab(&mut dispatcher, root.as_ref()).is_consumed());
        assert!(second.has_focus());

        // Wrapping at the end of the scope returns to its first target rather
        // than escaping into the tree behind it.
        assert!(tab(&mut dispatcher, root.as_ref()).is_consumed());
        assert!(first.has_focus());
        assert!(!outside.has_focus());
    }

    #[test]
    fn a_trapping_scope_takes_focus_away_from_the_tree_behind_it() {
        let outside = FocusNode::new();
        let inside = FocusNode::new();
        let scope = FocusScopeElement {
            children: vec![focusable(&inside)],
            traps: Cell::new(false),
        }
        .boxed();
        let root = IdentityBranch(vec![focusable(&outside), scope]).boxed();
        let mut dispatcher = EventDispatcher::new();
        advance_element_tree_generation();

        outside.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        assert!(outside.has_focus());

        set_scope_traps(root.as_ref(), true);
        advance_element_tree_generation();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        assert!(!outside.has_focus());
        assert_eq!(dispatcher.focused(), None);

        // Only the scope is reachable now.
        assert!(tab(&mut dispatcher, root.as_ref()).is_consumed());
        assert!(inside.has_focus());
    }

    #[test]
    fn leaving_a_trapping_scope_restores_the_focus_it_displaced() {
        let outside = FocusNode::new();
        let inside = FocusNode::new();
        let outside_lifecycle = Rc::new(RefCell::new(Vec::new()));
        let root = IdentityBranch(vec![
            FocusTestElement {
                node: outside.clone(),
                lifecycle: outside_lifecycle.clone(),
                autofocus: false,
            }
            .boxed(),
            FocusScopeElement {
                children: vec![focusable(&inside)],
                traps: Cell::new(false),
            }
            .boxed(),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        advance_element_tree_generation();

        outside.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        assert!(outside.has_focus());

        set_scope_traps(root.as_ref(), true);
        advance_element_tree_generation();
        inside.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        assert!(inside.has_focus());

        set_scope_traps(root.as_ref(), false);
        advance_element_tree_generation();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        assert!(outside.has_focus());
        assert!(!inside.has_focus());
        assert_eq!(&*outside_lifecycle.borrow(), &["focus", "blur", "focus"]);
    }

    #[test]
    fn a_trap_elsewhere_suspends_the_tree_and_releases_tab_to_it() {
        let node = FocusNode::new();
        let lifecycle = Rc::new(RefCell::new(Vec::new()));
        let root = IdentityBranch(vec![
            FocusTestElement {
                node: node.clone(),
                lifecycle: lifecycle.clone(),
                autofocus: false,
            }
            .boxed(),
            focusable(&FocusNode::new()),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        advance_element_tree_generation();

        node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        assert!(node.has_focus());

        let trap = FocusTrap::acquire();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        assert!(!node.has_focus());
        assert_eq!(dispatcher.focused(), None);
        assert!(
            !tab(&mut dispatcher, root.as_ref()).is_consumed(),
            "a suspended tree must pass Tab on to the trapping region"
        );
        assert_eq!(dispatcher.focused(), None);

        drop(trap);
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        assert!(node.has_focus());
        assert_eq!(&*lifecycle.borrow(), &["focus", "blur", "focus"]);
    }

    #[test]
    fn text_reaches_a_nested_dispatch_root_while_the_tree_is_suspended() {
        let node = FocusNode::new();
        let events = Rc::new(Cell::new(0));
        let outside_events = Rc::new(Cell::new(0));
        let root = IdentityBranch(vec![
            FocusKeyElement {
                node: node.clone(),
                bounds: (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
                key_events: events.clone(),
                consume_keys: Rc::new(Cell::new(true)),
            }
            .boxed(),
            routed_leaf(
                (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
                outside_events.clone(),
                false,
                false,
            ),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        advance_element_tree_generation();
        node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        let text = ElementEvent::TextInput {
            text: "a".to_string(),
            action: KeyAction::Pressed,
            modifiers: Modifiers::default(),
        };
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &text);
        assert_eq!(events.get(), 1);
        assert_eq!(outside_events.get(), 0);

        let _trap = FocusTrap::acquire();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &text);

        assert_eq!(events.get(), 1, "the suspended field must not receive text");
        assert_eq!(
            outside_events.get(),
            1,
            "text must be routed so an overlay host can hand it to its own root"
        );
    }

    #[test]
    fn a_press_outside_a_trapping_scope_does_not_take_focus_out_of_it() {
        let inside = FocusNode::new();
        let outside = FocusNode::new();
        let root = IdentityBranch(vec![
            PressableFocusElement {
                node: outside.clone(),
                bounds: (Vec2d { x: 20.0, y: 20.0 }, Vec2d { x: 30.0, y: 30.0 }),
            }
            .boxed(),
            FocusScopeElement {
                children: vec![
                    PressableFocusElement {
                        node: inside.clone(),
                        bounds: (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
                    }
                    .boxed(),
                ],
                traps: Cell::new(true),
            }
            .boxed(),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        advance_element_tree_generation();

        inside.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        assert!(inside.has_focus());

        let pos = Vec2d { x: 25.0, y: 25.0 };
        let _ = dispatcher.dispatch(
            root.as_ref(),
            pos,
            &ElementEvent::PointerDown(PointerInfo::mouse(pos, PointerButton::Primary)),
        );

        assert!(!outside.has_focus(), "a press escaped the trapping scope");
        assert!(inside.has_focus());

        // A press on nothing focusable at all leaves the scope's owner alone
        // instead of blurring it from outside.
        let empty = Vec2d { x: 50.0, y: 50.0 };
        let _ = dispatcher.dispatch(
            root.as_ref(),
            empty,
            &ElementEvent::PointerDown(PointerInfo::mouse(empty, PointerButton::Primary)),
        );

        assert!(inside.has_focus());
    }

    struct PressableFocusElement {
        node: FocusNode,
        bounds: (Vec2d, Vec2d),
    }

    impl VisitorElement for PressableFocusElement {
        fn debug_name(&self) -> &'static str {
            "PressableFocusElement"
        }
    }

    impl EventElement for PressableFocusElement {
        fn focus_node(&self) -> Option<&FocusNode> {
            Some(&self.node)
        }
    }

    impl LayoutElement for PressableFocusElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }

    impl Drawable for PressableFocusElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for PressableFocusElement {}

    /// Flips the trapping flag of the scope in `root`, as a rebuild would.
    fn set_scope_traps(root: &dyn Element, traps: bool) {
        let mut found = false;
        root.visit_children(&mut |child| {
            if let Some(scope) = child
                .option_any()
                .and_then(|any| any.downcast_ref::<FocusScopeElement>())
            {
                scope.traps.set(traps);
                found = true;
            }
        });
        assert!(found, "the tree under test has no focus scope");
    }

    struct FocusKeyElement {
        node: FocusNode,
        bounds: (Vec2d, Vec2d),
        key_events: Rc<Cell<usize>>,
        consume_keys: Rc<Cell<bool>>,
    }

    impl VisitorElement for FocusKeyElement {
        fn debug_name(&self) -> &'static str {
            "FocusKeyElement"
        }
    }

    impl EventElement for FocusKeyElement {
        fn focus_node(&self) -> Option<&FocusNode> {
            Some(&self.node)
        }

        fn on_event(&self, event: &ElementEvent) -> EventResult {
            if matches!(
                event,
                ElementEvent::KeyInput { .. }
                    | ElementEvent::CharInput { .. }
                    | ElementEvent::TextInput { .. }
                    | ElementEvent::ImePreedit { .. }
            ) {
                self.key_events.set(self.key_events.get() + 1);
                return self.consume_keys.get().into();
            }
            EventResult::ignored()
        }
    }

    impl LayoutElement for FocusKeyElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }
    impl Drawable for FocusKeyElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for FocusKeyElement {}

    #[test]
    fn named_keys_target_focus_first_then_fall_back_when_ignored() {
        let first_node = FocusNode::new();
        let first_events = Rc::new(Cell::new(0));
        let second_events = Rc::new(Cell::new(0));
        let consume_first = Rc::new(Cell::new(true));
        let root = IdentityBranch(vec![
            FocusKeyElement {
                node: first_node.clone(),
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                key_events: first_events.clone(),
                consume_keys: consume_first.clone(),
            }
            .boxed(),
            FocusKeyElement {
                node: FocusNode::new(),
                bounds: (
                    Vec2d { x: 20.0, y: 20.0 },
                    Vec2d { x: 30.0, y: 30.0 },
                ),
                key_events: second_events.clone(),
                consume_keys: Rc::new(Cell::new(true)),
            }
            .boxed(),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        first_node.request_focus();
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
        let arrow = ElementEvent::KeyInput {
            key: NamedKey::ArrowLeft,
            action: KeyAction::Pressed,
            modifiers: Modifiers::default(),
        };

        let consumed_by_focus = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 25.0, y: 25.0 },
            &arrow,
        );
        consume_first.set(false);
        let consumed_by_fallback = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 25.0, y: 25.0 },
            &arrow,
        );

        assert!(consumed_by_focus.is_consumed());
        assert!(consumed_by_fallback.is_consumed());
        assert_eq!(first_events.get(), 2);
        assert_eq!(second_events.get(), 1);
    }

    /// An element that hands focus to somebody else when pressed, the way a
    /// button focuses the field it belongs to. It is not focusable itself.
    struct FocusRequestingElement {
        requests: FocusNode,
        bounds: (Vec2d, Vec2d),
    }

    impl VisitorElement for FocusRequestingElement {
        fn debug_name(&self) -> &'static str {
            "FocusRequestingElement"
        }
    }

    impl EventElement for FocusRequestingElement {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            if matches!(event, ElementEvent::PointerDown(_)) {
                self.requests.request_focus();
                return EventResult::consumed();
            }
            EventResult::ignored()
        }
    }

    impl LayoutElement for FocusRequestingElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some(self.bounds)
        }
    }
    impl Drawable for FocusRequestingElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for FocusRequestingElement {}

    /// A request made while an event is delivered is granted by that event.
    ///
    /// Focus is resolved before an event is routed, so a handler's request
    /// arrives too late for that pass. Leaving it there would mean waiting for
    /// whatever input comes next: a mouse that moves a pixel hides the delay,
    /// but a finger that taps and lifts sends nothing more, so the field a
    /// button focused would only be focused by the *next* tap.
    #[test]
    fn focus_requested_while_an_event_is_delivered_is_granted_by_that_event() {
        let target = FocusNode::new();
        let button = Vec2d { x: 25.0, y: 25.0 };
        let root = IdentityBranch(vec![
            FocusKeyElement {
                node: target.clone(),
                bounds: (Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }),
                key_events: Rc::new(Cell::new(0)),
                consume_keys: Rc::new(Cell::new(true)),
            }
            .boxed(),
            FocusRequestingElement {
                requests: target.clone(),
                bounds: (Vec2d { x: 20.0, y: 20.0 }, Vec2d { x: 30.0, y: 30.0 }),
            }
            .boxed(),
        ])
        .boxed();
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            button,
            &ElementEvent::PointerDown(PointerInfo::mouse(button, PointerButton::Primary)),
        );

        assert!(
            target.has_focus(),
            "the press dropped focus and the handler asked for it back, so the \
             event must not end before that is settled"
        );
    }
}
