use std::any::TypeId;
use std::cell::Cell;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use smallvec::SmallVec;
use hashbrown::{HashMap, HashSet};

use crate::base::*;
use crate::components::event_element::{CaptureRequest, EventElement, EventResult, PointerKey};
use crate::components::layout_element::LayoutElement;
use crate::components::rebuildable::Rebuildable;
pub(crate) use crate::components::visitor_element::VisitorElement;
use crate::{AnyElement, Drawable, Key};

type EventChildren<'a> = SmallVec<[&'a dyn Element; 32]>;

#[cfg(test)]
thread_local! {
    static ROUTED_EVENT_SCRATCH_CONSTRUCTIONS: Cell<usize> = const { Cell::new(0) };
}

fn new_routed_event_scratch<'a>() -> EventChildren<'a> {
    #[cfg(test)]
    ROUTED_EVENT_SCRATCH_CONSTRUCTIONS.with(|count| count.set(count.get() + 1));
    EventChildren::new()
}

static NEXT_ELEMENT_ID: AtomicU64 = AtomicU64::new(1);
static ELEMENT_TREE_GENERATION: AtomicU64 = AtomicU64::new(0);

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

struct ElementNode<E> {
    id: Cell<ElementId>,
    element: E,
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
        AnyElement::new_projected(
            ElementNode {
                id: Cell::new(ElementId::next()),
                element: self,
            },
            project_element,
            project_element_mut,
        )
    }
}

fn project_element<E: Element + 'static>(value: &ElementNode<E>) -> &(dyn Element + 'static) {
    value
}

fn project_element_mut<E: Element + 'static>(
    value: &mut ElementNode<E>,
) -> &mut (dyn Element + 'static) {
    value
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

    fn get_size_from_child(&self) -> Option<Size> {
        self.element.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.element.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.element.pos_start_end()
    }
}

impl<E: Element + 'static> Rebuildable for ElementNode<E> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.element.rebuild_if_dirty(ctx);
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

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.element.with_rebuild_context(ctx, callback);
    }

    fn mark_needs_rebuild(&self) {
        self.element.mark_needs_rebuild();
    }
}

impl<E: Element + 'static> EventElement for ElementNode<E> {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.element.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.element.event_children(visitor);
    }
}

impl<E: Element + 'static> Drawable for ElementNode<E> {
    fn draw(&self, ctx: &BuildContext) {
        self.element.draw(ctx);
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

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }
}

impl Rebuildable for AnyElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref().rebuild_if_dirty(ctx)
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

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.as_ref().with_rebuild_context(ctx, callback)
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref().mark_needs_rebuild()
    }
}

impl EventElement for AnyElement {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.as_ref().on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().event_children(visitor)
    }
}

impl Drawable for AnyElement {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
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

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }
}

impl Rebuildable for Box<dyn Element> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref().rebuild_if_dirty(ctx)
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

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.as_ref().with_rebuild_context(ctx, callback)
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref().mark_needs_rebuild()
    }
}

impl EventElement for Box<dyn Element> {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.as_ref().on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().event_children(visitor)
    }
}

impl Drawable for Box<dyn Element> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
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

fn advance_element_tree_generation() {
    ELEMENT_TREE_GENERATION
        .fetch_update(Ordering::Release, Ordering::Relaxed, |generation| {
            generation.checked_add(1)
        })
        .expect("exhausted all element-tree generations");
}

fn structural_children(element: &dyn Element) -> SmallVec<[&dyn Element; 8]> {
    let mut children: SmallVec<[&dyn Element; 8]> = SmallVec::new();
    element.event_children(&mut |child| children.push(child));
    element.visit_children(&mut |child| {
        if !children
            .iter()
            .any(|existing| std::ptr::eq(*existing, child))
        {
            children.push(child);
        }
    });
    children
}

fn identities_are_compatible(old: &dyn Element, new: &dyn Element) -> bool {
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
pub(crate) fn reconcile_element_identities(old: &dyn Element, new: &dyn Element) {
    if !identities_are_compatible(old, new) {
        return;
    }

    if let Some(old_id) = old.element_id() {
        new.set_element_id(old_id);
    }

    let old_children = structural_children(old);
    let new_children = structural_children(new);
    let mut matched = vec![false; old_children.len()];

    for (new_index, new_child) in new_children.iter().copied().enumerate() {
        let old_index = if let Some(new_key) = new_child.reconciliation_key() {
            old_children
                .iter()
                .enumerate()
                .position(|(index, old_child)| {
                    !matched[index]
                        && old_child.reconciliation_key() == Some(new_key)
                        && identities_are_compatible(*old_child, new_child)
                })
        } else {
            old_children.get(new_index).and_then(|old_child| {
                (!matched[new_index]
                    && old_child.reconciliation_key().is_none()
                    && identities_are_compatible(*old_child, new_child))
                .then_some(new_index)
            })
        };

        if let Some(old_index) = old_index {
            matched[old_index] = true;
            reconcile_element_identities(old_children[old_index], new_child);
        }
    }
}

/// Reconciles identities for a generated subtree and invalidates structural
/// path indexes.
pub(crate) fn reconcile_generated_tree(old: &dyn Element, new: &dyn Element) {
    reconcile_element_identities(old, new);
    advance_element_tree_generation();
}

/// A root-relative sequence of structural child indexes.
///
/// Paths contain no references or addresses, so they remain safe when inline
/// [`AnyElement`] owners move. They are rebuilt after structural generation
/// changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElementPath(Box<[usize]>);

/// Routes pointer events and persists capture ownership across event calls.
///
/// Capture lookup is an average `O(1)` hash-map operation. The saved path is
/// then resolved from the current root, avoiding a full-tree capture scan.
pub struct EventDispatcher {
    captures: HashMap<PointerKey, ElementId>,
    paths: HashMap<ElementId, ElementPath>,
    indexed_generation: u64,
    indexed_root: Option<ElementId>,
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
            paths: HashMap::new(),
            indexed_generation: u64::MAX,
            indexed_root: None,
        }
    }

    /// Dispatches one event using persistent capture state.
    ///
    /// Uncaptured events use normal hit testing. A captured move, exit, or up
    /// resolves only the saved root-to-owner path. Pointer-up releases its
    /// capture after delivery, and cancellation is delivered once to every
    /// distinct captured owner before all captures are cleared.
    pub fn dispatch(
        &mut self,
        root: &dyn Element,
        pos: Vec2d,
        event: &ElementEvent,
    ) -> EventResult {
        let pointer = event_pointer_key(event);
        let routes_to_capture = matches!(
            event,
            ElementEvent::PointerMove(_, _, _)
                | ElementEvent::PointerUp(_, _, _)
                | ElementEvent::PointerExited(_, _)
        );
        let was_captured = routes_to_capture
            && pointer.is_some_and(|pointer| self.captures.contains_key(&pointer));

        self.synchronize_paths(root);

        if matches!(event, ElementEvent::Cancel) {
            return self.cancel_captures(root, event).without_capture_request();
        }

        if routes_to_capture && let Some(pointer) = pointer {
            if was_captured {
                return self.dispatch_captured(root, pointer, event);
            }
        }

        let outcome = dispatch_routed_event(root, pos, event);
        self.apply_capture_request(outcome.result.capture_request(), outcome.capture_owner);
        outcome.result.without_capture_request()
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
        let Some(path) = self.paths.get(&owner) else {
            return EventResult::ignored();
        };
        let Some(target) = resolve_element_path(root, &path.0) else {
            return EventResult::ignored();
        };
        if target.element_id() != Some(owner) {
            return EventResult::ignored();
        }
        target
            .on_event(&ElementEvent::Cancel)
            .without_capture_request()
    }

    fn synchronize_paths(&mut self, root: &dyn Element) {
        let generation = element_tree_generation();
        let root_id = root.element_id();
        if self.indexed_generation == generation && self.indexed_root == root_id {
            return;
        }

        self.paths.clear();
        let mut path = Vec::new();
        index_element_paths(root, &mut path, &mut self.paths);
        self.captures
            .retain(|_, owner| self.paths.contains_key(owner));
        self.indexed_generation = generation;
        self.indexed_root = root_id;
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
        let Some(path) = self.paths.get(&owner) else {
            self.captures.remove(&pointer);
            return EventResult::ignored();
        };
        let Some(target) = resolve_element_path(root, &path.0) else {
            self.captures.remove(&pointer);
            return EventResult::ignored();
        };
        if target.element_id() != Some(owner) {
            self.captures.remove(&pointer);
            return EventResult::ignored();
        }

        let result = target.on_event(event);
        self.apply_capture_request(result.capture_request(), Some(owner));
        if matches!(event, ElementEvent::PointerUp(_, _, _)) {
            self.captures.remove(&pointer);
        }
        result.without_capture_request()
    }

    fn cancel_captures(&mut self, root: &dyn Element, event: &ElementEvent) -> EventResult {
        let owners: HashSet<ElementId> = self.captures.values().copied().collect();
        let mut result = EventResult::ignored();
        for owner in owners {
            let Some(path) = self.paths.get(&owner) else {
                continue;
            };
            let Some(target) = resolve_element_path(root, &path.0) else {
                continue;
            };
            if target.element_id() == Some(owner) {
                result = result.merge(target.on_event(event));
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
}

fn event_pointer_key(event: &ElementEvent) -> Option<PointerKey> {
    match event {
        ElementEvent::PointerDown(_, source, id)
        | ElementEvent::PointerUp(_, source, id)
        | ElementEvent::PointerMove(_, source, id)
        | ElementEvent::PointerExited(source, id) => Some(PointerKey::new(*source, *id)),
        _ => None,
    }
}

fn index_element_paths(
    element: &dyn Element,
    path: &mut Vec<usize>,
    paths: &mut HashMap<ElementId, ElementPath>,
) {
    if let Some(id) = element.element_id() {
        paths.insert(id, ElementPath(path.clone().into_boxed_slice()));
    }
    for (index, child) in structural_children(element).iter().copied().enumerate() {
        path.push(index);
        index_element_paths(child, path, paths);
        path.pop();
    }
}

fn resolve_element_path<'a>(root: &'a dyn Element, path: &[usize]) -> Option<&'a dyn Element> {
    let mut current = root;
    for index in path {
        current = {
            let children = structural_children(current);
            *children.get(*index)?
        };
    }
    Some(current)
}

fn dispatch_routed_event(
    root: &dyn Element,
    pos: Vec2d,
    event: &ElementEvent,
) -> RoutedEventResult {
    let mut children = new_routed_event_scratch();
    dispatch_routed_event_inner(root, pos, event, &mut children)
}

fn dispatch_routed_event_inner<'a>(
    root: &'a dyn Element,
    pos: Vec2d,
    event: &ElementEvent,
    children: &mut EventChildren<'a>,
) -> RoutedEventResult {
    let mut result = EventResult::ignored();
    let mut capture_owner = None;
    let start = children.len();
    root.event_children(&mut |child| children.push(child));

    while children.len() > start {
        let child = children
            .pop()
            .expect("routed event scratch contains an element beyond its entry length");
        let child_outcome = dispatch_routed_event_inner(child, pos, event, children);
        result = result.merge(child_outcome.result);
        if capture_owner.is_none() {
            capture_owner = child_outcome.capture_owner.or_else(|| {
                (!matches!(child_outcome.result.capture_request(), CaptureRequest::None))
                    .then(|| root.element_id())
                    .flatten()
            });
        }
        if child_outcome.result.is_consumed() {
            children.truncate(start);
            return RoutedEventResult {
                result,
                capture_owner,
            };
        }
    }
    children.truncate(start);

    let bounds = root.pos_start_end();
    let inside = bounds.is_none_or(|(start, end)| {
        pos.x >= start.x && pos.x <= end.x && pos.y >= start.y && pos.y <= end.y
    });
    if inside {
        let own_result = root.on_event(event);
        if capture_owner.is_none() && !matches!(own_result.capture_request(), CaptureRequest::None)
        {
            capture_owner = root.element_id();
        }
        result = result.merge(own_result);
    }

    RoutedEventResult {
        result,
        capture_owner,
    }
}

/// Perform a hit-test on the element tree and dispatch the event to the deepest
/// hit element. Returns the effects produced by the target element.
pub fn dispatch_event(root: &dyn Element, pos: Vec2d, event: &ElementEvent) -> EventResult {
    let mut children = EventChildren::new();
    dispatch_event_inner(root, pos, event, &mut children)
}

fn dispatch_event_inner<'a>(
    root: &'a dyn Element,
    pos: Vec2d,
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
        let child_result = dispatch_event_inner(child, pos, event, children);
        result = result.merge(child_result);
        if child_result.is_consumed() {
            children.truncate(start);
            return result;
        }
    }
    children.truncate(start);

    // Check if pos is inside this element's bounds
    let bounds = root.pos_start_end();
    if let Some((start, end)) = bounds {
        let inside = pos.x >= start.x && pos.x <= end.x && pos.y >= start.y && pos.y <= end.y;
        if inside {
            return result.merge(root.on_event(event));
        }
    }

    // If the element has no position info, still try to dispatch the event.
    // This allows elements like Button (which don't track absolute position)
    // to receive events when reached through the tree traversal.
    if bounds.is_none() {
        return result.merge(root.on_event(event));
    }

    result
}

/// Broadcast an event to every element in the tree, regardless of hit-testing.
/// Returns the combined effects produced by every element.
pub fn broadcast_event(root: &dyn Element, event: &ElementEvent) -> EventResult {
    let mut children = EventChildren::new();
    broadcast_event_inner(root, event, &mut children)
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

    result.merge(root.on_event(event))
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_events::pointer::PointerSource;
    use aimer_rubick::INLINE_CAPACITY;

    use super::*;
    use crate::Key;

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
                ElementEvent::PointerDown(_, source, pointer) if self.capture_on_down => {
                    EventResult::consumed().with_pointer_capture(PointerKey::new(*source, *pointer))
                }
                ElementEvent::PointerMove(_, source, pointer) if self.release_on_move => {
                    EventResult::consumed().with_pointer_release(PointerKey::new(*source, *pointer))
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

    #[test]
    fn uncaptured_routing_constructs_one_scratch_buffer_per_dispatch() {
        let events = Rc::new(Cell::new(0));
        let leaf = routed_leaf(
            (Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }),
            events.clone(),
            false,
            false,
        );
        let branch = RoutedElement {
            children: vec![leaf],
            bounds: None,
            events: events.clone(),
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();
        let root = RoutedElement {
            children: vec![branch],
            bounds: None,
            events,
            capture_on_down: false,
            release_on_move: false,
        }
        .boxed();
        ROUTED_EVENT_SCRATCH_CONSTRUCTIONS.with(|count| count.set(0));

        let _ = EventDispatcher::new().dispatch(
            root.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerMove(Vec2d { x: 5.0, y: 5.0 }, PointerSource::Mouse, 0),
        );

        ROUTED_EVENT_SCRATCH_CONSTRUCTIONS.with(|count| assert_eq!(count.get(), 1));
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
            &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, touch.source, touch.id),
        );
        target_events.set(0);
        unrelated_events.set(0);

        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 500.0, y: 500.0 },
            &ElementEvent::PointerMove(Vec2d { x: 500.0, y: 500.0 }, touch.source, touch.id),
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
            &ElementEvent::PointerDown(
                Vec2d { x: 5.0, y: 5.0 },
                mouse_pointer.source,
                mouse_pointer.id,
            ),
        );
        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 25.0, y: 25.0 },
            &ElementEvent::PointerDown(
                Vec2d { x: 25.0, y: 25.0 },
                touch_pointer.source,
                touch_pointer.id,
            ),
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
            &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
        );
        let _ = dispatcher.dispatch(
            target.as_ref(),
            Vec2d { x: 50.0, y: 50.0 },
            &ElementEvent::PointerMove(Vec2d { x: 50.0, y: 50.0 }, pointer.source, pointer.id),
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
            &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
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
            &ElementEvent::PointerMove(Vec2d { x: 50.0, y: 50.0 }, pointer.source, pointer.id),
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
            &ElementEvent::PointerDown(Vec2d::default(), pointer.source, pointer.id),
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
                &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
            );
        }
        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 50.0, y: 50.0 },
            &ElementEvent::PointerUp(Vec2d { x: 50.0, y: 50.0 }, mouse.source, mouse.id),
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
            &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
        );

        let replacement = ReplacementLeaf.boxed();
        reconcile_generated_tree(old.as_ref(), replacement.as_ref());
        let result = dispatcher.dispatch(
            replacement.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerMove(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
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
            &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
        );
        let owner = dispatcher
            .captured_owner(pointer)
            .expect("pointer must be captured after down");
        dispatcher
            .paths
            .insert(owner, ElementPath(vec![usize::MAX].into_boxed_slice()));
        events.set(0);

        let result = dispatcher.dispatch(
            target.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerMove(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
        );

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
                ElementEvent::PointerDown(_, source, 7) => {
                    EventResult::consumed().with_pointer_capture(PointerKey::new(*source, 7))
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
        children: Vec<TreeElement>,
        events: Cell<usize>,
    }

    impl VisitorElement for TreeElement {
        fn debug_name(&self) -> &'static str {
            "TreeElement"
        }
    }

    impl EventElement for TreeElement {
        fn on_event(&self, _event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            EventResult::consumed()
        }

        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child);
            }
        }
    }

    impl LayoutElement for TreeElement {}

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
        use aimer_events::pointer::PointerSource;

        let events = Rc::new(Cell::new(0));
        let element = CapturingElement {
            events: Cell::new(0),
        }
        .boxed();
        let mut dispatcher = EventDispatcher::new();
        let _ = dispatcher.dispatch(
            element.as_ref(),
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, PointerSource::Touch, 7),
        );
        let event = ElementEvent::PointerMove(Vec2d { x: 50.0, y: 50.0 }, PointerSource::Touch, 7);

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
            &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
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
            &ElementEvent::PointerDown(Vec2d { x: 5.0, y: 5.0 }, pointer.source, pointer.id),
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
        let element = TreeElement {
            children: vec![TreeElement {
                children: vec![TreeElement {
                    children: Vec::new(),
                    events: Cell::new(0),
                }],
                events: Cell::new(0),
            }],
            events: Cell::new(0),
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
    }
}
