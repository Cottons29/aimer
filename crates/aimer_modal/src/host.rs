use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use aimer_animation::AnimInstant;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_events::window::request_animation_frame;
use aimer_macro::Rebuildable;
use aimer_widget::base::BuildContext;
use aimer_widget::focus::FocusTrap;
use aimer_widget::{
    AnyElement, Drawable, Element, EventDispatcher, EventElement, EventResult, LayoutElement,
    PointerKey, RequiredChild, VisitorElement, Widget, broadcast_event, dispatch_focused_event,
};

use crate::ModalAnimation;

type EntryBuilder =
    Box<dyn FnOnce(&BuildContext, ModalId, Rc<RefCell<ModalTimeline>>) -> AnyElement>;

thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
    static COMMANDS: RefCell<VecDeque<ModalCommand>> = const { RefCell::new(VecDeque::new()) };
    static ENTRIES: RefCell<Vec<HostedModal>> = const { RefCell::new(Vec::new()) };
    static LAYERS: RefCell<Vec<HostedLayer>> = const { RefCell::new(Vec::new()) };
}

/// A painter installed above every modal, receiving no events.
///
/// A modal is a *mode*: presenting one deliberately cancels the gestures
/// underneath it and puts a barrier between the user and the rest of the
/// application. Some overlays are the opposite of that — drag feedback follows
/// a gesture that must keep running, and must never intercept the pointer it is
/// chasing. Those install a layer instead: it paints last, above the modals,
/// and that is all it does.
///
/// The painter returns whether it should stay installed, so an overlay that
/// finishes an animation can retire itself from inside the frame that finished
/// it.
///
/// # Examples
///
/// ```no_run
/// use std::rc::Rc;
///
/// use aimer_modal::OverlayLayer;
///
/// // Paints for exactly one frame, then removes itself.
/// let handle = OverlayLayer::install(Rc::new(|_ctx| false));
/// handle.remove();
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct OverlayLayer;

/// Identifies an installed [`OverlayLayer`] so it can be taken down again.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OverlayLayerHandle(u64);

/// Paints one frame of an overlay layer, returning whether to keep it.
pub type OverlayPainter = Rc<dyn Fn(&BuildContext) -> bool>;

struct HostedLayer {
    id: OverlayLayerHandle,
    paint: OverlayPainter,
}

impl OverlayLayer {
    /// Installs `paint` above every modal and returns its handle.
    pub fn install(paint: OverlayPainter) -> OverlayLayerHandle {
        let id = NEXT_ID.with(|next_id| {
            let id = OverlayLayerHandle(next_id.get());
            next_id.set(next_id.get().wrapping_add(1).max(1));
            id
        });
        LAYERS.with_borrow_mut(|layers| layers.push(HostedLayer { id, paint }));
        request_animation_frame();
        id
    }

    /// Returns whether any layer is installed.
    pub fn is_installed() -> bool {
        LAYERS.with_borrow(|layers| !layers.is_empty())
    }
}

impl OverlayLayerHandle {
    /// Takes the layer down. Repeated calls are harmless.
    pub fn remove(self) {
        LAYERS.with_borrow_mut(|layers| layers.retain(|layer| layer.id != self));
        request_animation_frame();
    }
}

/// Paints the installed layers, dropping the ones that asked to retire.
///
/// The list is taken out of the slot for the duration of the walk, so a painter
/// is free to install or remove a layer while it runs.
fn draw_layers(ctx: &BuildContext) {
    let mut layers = LAYERS.with_borrow_mut(std::mem::take);
    if layers.is_empty() {
        return;
    }
    layers.retain(|layer| (layer.paint)(ctx));
    LAYERS.with_borrow_mut(|installed| {
        layers.append(installed);
        *installed = layers;
    });
}

/// Stable identity assigned to a presented modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModalId(u64);

/// A handle that can dismiss the modal returned by [`crate::Modal::show`].
#[derive(Clone)]
pub struct ModalHandle {
    id: ModalId,
    dismissed: Rc<Cell<bool>>,
}

impl ModalHandle {
    /// Returns this modal's stable identity.
    pub fn id(&self) -> ModalId {
        self.id
    }

    /// Begins dismissal. Repeated calls are harmless and return `false`.
    pub fn dismiss(&self) -> bool {
        if self.dismissed.replace(true) {
            return false;
        }
        enqueue(ModalCommand::Dismiss(self.id));
        true
    }

    /// Whether this modal is still presented.
    ///
    /// A modal is not only closed by its owner: the barrier, `Escape` and
    /// [`ModalController::dismiss_top`] all close it by id, leaving the handle
    /// none the wiser. An owner that acts on "my panel is open" — keeping a
    /// selection alive for it, say — must ask the registry rather than trust
    /// its own last request.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn show() -> aimer_modal::ModalHandle { unimplemented!() }
    /// let handle = show();
    /// assert!(handle.is_showing());
    ///
    /// handle.dismiss();
    /// assert!(!handle.is_showing());
    /// ```
    pub fn is_showing(&self) -> bool {
        !self.dismissed.get() && is_presented(self.id)
    }
}

/// Whether `id` is presented, or on its way to being presented.
///
/// A queued `Show` counts, because a modal asked for during a frame is only
/// built by the next one; a queued `Dismiss` does not, so an owner learns of a
/// closure in the same frame it happened rather than one later.
fn is_presented(id: ModalId) -> bool {
    let closing = COMMANDS.with_borrow(|commands| {
        commands
            .iter()
            .any(|command| matches!(command, ModalCommand::Dismiss(dismissed) if *dismissed == id))
    });
    if closing {
        return false;
    }
    let queued = COMMANDS.with_borrow(|commands| {
        commands
            .iter()
            .any(|command| matches!(command, ModalCommand::Show { id: shown, .. } if *shown == id))
    });
    queued
        || ENTRIES.with_borrow(|entries| {
            entries
                .iter()
                .any(|entry| entry.id == id && !entry.timeline.borrow().is_closing())
        })
}

/// Access to the application-wide modal overlay.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModalController;

impl ModalController {
    /// Begins dismissal of the topmost modal, if one exists.
    pub fn dismiss_top() -> bool {
        let has_modal = ENTRIES.with(|entries| !entries.borrow().is_empty())
            || COMMANDS.with(|commands| {
                commands
                    .borrow()
                    .iter()
                    .any(|command| matches!(command, ModalCommand::Show { .. }))
            });
        if has_modal {
            enqueue(ModalCommand::DismissTop);
        }
        has_modal
    }

    /// Returns whether a modal is active or waiting for the first host frame.
    pub fn is_showing() -> bool {
        !ENTRIES.with(|entries| entries.borrow().is_empty())
            || COMMANDS.with(|commands| {
                commands
                    .borrow()
                    .iter()
                    .any(|command| matches!(command, ModalCommand::Show { .. }))
            })
    }
}

/// Root overlay that paints application modals above its child.
///
/// `AimerApp` installs this host automatically. It remains public for embedded
/// render roots and tests that construct widget trees without `AimerApp`.
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(id = "aimer_modal::ModalHost", schema_only)]
pub struct ModalHost<W = RequiredChild> {
    #[portable_child]
    child: W,
}

impl Default for ModalHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalHost {
    /// Creates an incomplete host builder.
    pub fn new() -> Self {
        Self {
            child: RequiredChild,
        }
    }

    /// Attaches the application root and completes the host.
    pub fn child<W: Widget>(self, child: W) -> ModalHost<W> {
        ModalHost { child }
    }
}

impl<W: Widget + 'static> Widget for ModalHost<W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        RawModalHost {
            child: self.child.to_element(ctx),
            overlay: RawModalOverlay::default(),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "ModalHost"
    }
}

#[derive(Rebuildable)]
struct RawModalHost {
    child: AnyElement,
    overlay: RawModalOverlay,
}

impl Drop for RawModalHost {
    fn drop(&mut self) {
        clear_registry();
    }
}

impl Drawable for RawModalHost {
    fn draw(&self, ctx: &BuildContext) {
        if self.overlay.prepare(ctx) {
            let _ = broadcast_event(self.child.as_ref(), &ElementEvent::Cancel);
        }
        self.child.draw(ctx);
        self.overlay.draw_entries(ctx);
    }
}

impl EventElement for RawModalHost {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.overlay.on_event(event)
    }
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
        visitor(&self.overlay);
    }
}

impl LayoutElement for RawModalHost {
    fn size(&self) -> Option<Size> {
        None
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ctx.parent_size
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ctx.parent_size
    }
}

impl VisitorElement for RawModalHost {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
        visitor(&self.overlay);
    }

    fn debug_name(&self) -> &'static str {
        "ModalHost"
    }
}

#[derive(Default, Rebuildable)]
struct RawModalOverlay {
    promoted_captures: RefCell<HashSet<PointerKey>>,
}

impl RawModalOverlay {
    fn prepare(&self, ctx: &BuildContext) -> bool {
        process_commands(ctx)
    }

    fn draw_entries(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        ENTRIES.with(|entries| {
            let mut entries = entries.borrow_mut();
            for entry in entries.iter() {
                entry.timeline.borrow_mut().tick(now, entry.animation);
                entry.element.draw(ctx);
            }
            entries.retain(|entry| {
                let retain = !entry.timeline.borrow().finished();
                if !retain {
                    cancel_hosted_entry(entry);
                }
                retain
            });
        });
        draw_layers(ctx);
    }
}

impl Drawable for RawModalOverlay {
    fn draw(&self, ctx: &BuildContext) {
        self.prepare(ctx);
        self.draw_entries(ctx);
    }
}

impl EventElement for RawModalOverlay {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        ENTRIES.with(|entries| {
            let entries = entries.borrow();
            if matches!(event, ElementEvent::Scroll { .. }) {
                // Scroll events carry no pointer position. Route them to the
                // topmost modal directly so an anchored wheel can consume a
                // trackpad or mouse-wheel frame instead of the page beneath
                // it. Pointer events still use normal hit testing/capture.
                return entries
                    .last()
                    .map_or_else(EventResult::ignored, |entry| {
                        dispatch_focused_event(entry.element.as_ref(), event)
                    });
            }
            if matches!(
                event,
                ElementEvent::KeyInput {
                    key: aimer_events::element::NamedKey::Escape,
                    action: aimer_events::element::KeyAction::Pressed,
                    ..
                }
            ) {
                return entries
                    .last()
                    .map_or_else(EventResult::ignored, |entry| entry.element.on_event(event));
            }
            let pointer = event_pointer_key(event);
            if let Some(pointer) = pointer
                && let Some(entry) = entries
                    .iter()
                    .rev()
                    .find(|entry| entry.dispatcher.borrow().is_captured(pointer))
            {
                let pos = event.get_pointer_pos().unwrap_or_default();
                let result = dispatch_hosted_event(entry, pos, event);
                self.track_promoted_capture(result, pointer);
                return result;
            }
            if let Some(pointer) = pointer
                && self.promoted_captures.borrow_mut().remove(&pointer)
            {
                return EventResult::ignored().with_pointer_release(pointer);
            }
            let mut result = EventResult::ignored();
            for entry in entries.iter().rev() {
                let pos = event.get_pointer_pos().unwrap_or_default();
                let entry_result = dispatch_hosted_event(entry, pos, event);
                if let Some(pointer) = pointer {
                    self.track_promoted_capture(entry_result, pointer);
                }
                result = result.merge(entry_result);
                if entry_result.is_consumed() {
                    return result;
                }
            }
            result.merge(EventResult::from(!entries.is_empty()))
        })
    }
}

impl RawModalOverlay {
    fn track_promoted_capture(&self, result: EventResult, pointer: PointerKey) {
        match result.capture_request() {
            aimer_widget::CaptureRequest::Capture(captured) if captured == pointer => {
                self.promoted_captures.borrow_mut().insert(pointer);
            }
            aimer_widget::CaptureRequest::Release(released) if released == pointer => {
                self.promoted_captures.borrow_mut().remove(&pointer);
            }
            _ => {}
        }
    }
}

impl LayoutElement for RawModalOverlay {
    fn size(&self) -> Option<Size> {
        None
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ctx.parent_size
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ctx.parent_size
    }
}

impl VisitorElement for RawModalOverlay {
    fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

    fn debug_name(&self) -> &'static str {
        "ModalOverlay"
    }
}

enum ModalCommand {
    Show {
        id: ModalId,
        animation: Option<ModalAnimation>,
        build: EntryBuilder,
    },
    Dismiss(ModalId),
    DismissTop,
}

struct HostedModal {
    id: ModalId,
    element: AnyElement,
    animation: Option<ModalAnimation>,
    timeline: Rc<RefCell<ModalTimeline>>,
    dispatcher: RefCell<EventDispatcher>,
    focus_trap: RefCell<Option<FocusTrap>>,
}

impl HostedModal {
    /// Stops confining keyboard focus to this entry.
    ///
    /// Called when dismissal begins rather than when the entry is finally
    /// dropped: an exit animation is the modal leaving, so the application
    /// underneath gets its focus owner back while the content fades out instead
    /// of a few hundred milliseconds later. Releasing twice is harmless, and the
    /// guard is dropped with the entry in any case, so an entry can never leave
    /// focus confined to itself after it is gone.
    fn release_focus_trap(&self) {
        drop(self.focus_trap.borrow_mut().take());
    }
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

fn dispatch_hosted_event(
    entry: &HostedModal,
    pos: aimer_attribute::Vec2d,
    event: &ElementEvent,
) -> EventResult {
    let pointer = event_pointer_key(event);
    let was_captured =
        pointer.is_some_and(|pointer| entry.dispatcher.borrow().is_captured(pointer));
    let result = entry
        .dispatcher
        .borrow_mut()
        .dispatch(entry.element.as_ref(), pos, event);
    let is_captured = pointer.is_some_and(|pointer| entry.dispatcher.borrow().is_captured(pointer));
    match (pointer, was_captured, is_captured) {
        (Some(pointer), false, true) => result.with_pointer_capture(pointer),
        (Some(pointer), true, false) => result.with_pointer_release(pointer),
        _ => result,
    }
}

fn cancel_hosted_entry(entry: &HostedModal) {
    let _ = broadcast_event(entry.element.as_ref(), &ElementEvent::Cancel);
    entry.dispatcher.borrow_mut().clear_captures();
}

pub(crate) struct ModalTimeline {
    progress: f32,
    phase: TimelinePhase,
}

enum TimelinePhase {
    Entering {
        started: Option<AnimInstant>,
    },
    Shown,
    Exiting {
        started: Option<AnimInstant>,
        from: f32,
    },
    Finished,
}

impl ModalTimeline {
    fn new(animated: bool) -> Self {
        Self {
            progress: if animated { 0.0 } else { 1.0 },
            phase: if animated {
                TimelinePhase::Entering { started: None }
            } else {
                TimelinePhase::Shown
            },
        }
    }

    pub(crate) fn new_static() -> Self {
        Self::new(false)
    }

    pub(crate) fn progress(&self) -> f32 {
        self.progress
    }

    fn begin_exit(&mut self, animated: bool) {
        if matches!(
            self.phase,
            TimelinePhase::Exiting { .. } | TimelinePhase::Finished
        ) {
            return;
        }
        if animated {
            self.phase = TimelinePhase::Exiting {
                started: None,
                from: self.progress,
            };
        } else {
            self.progress = 0.0;
            self.phase = TimelinePhase::Finished;
        }
        request_animation_frame();
    }

    fn tick(&mut self, now: AnimInstant, animation: Option<ModalAnimation>) {
        let Some(animation) = animation else {
            return;
        };
        match &mut self.phase {
            TimelinePhase::Entering { started } => {
                let start = *started.get_or_insert(now);
                let t = duration_progress(now, start, animation.enter_duration);
                self.progress = animation.enter_curve.transform(t);
                if t >= 1.0 {
                    self.progress = 1.0;
                    self.phase = TimelinePhase::Shown;
                } else {
                    request_animation_frame();
                }
            }
            TimelinePhase::Exiting { started, from } => {
                let start = *started.get_or_insert(now);
                let t = duration_progress(now, start, animation.exit_duration);
                self.progress = *from * (1.0 - animation.exit_curve.transform(t));
                if t >= 1.0 {
                    self.progress = 0.0;
                    self.phase = TimelinePhase::Finished;
                } else {
                    request_animation_frame();
                }
            }
            TimelinePhase::Shown | TimelinePhase::Finished => {}
        }
    }

    fn finished(&self) -> bool {
        matches!(self.phase, TimelinePhase::Finished)
    }

    /// Whether the modal is on its way out, animating or already gone.
    fn is_closing(&self) -> bool {
        matches!(
            self.phase,
            TimelinePhase::Exiting { .. } | TimelinePhase::Finished
        )
    }
}

fn duration_progress(now: AnimInstant, start: AnimInstant, duration: std::time::Duration) -> f32 {
    if duration.is_zero() {
        1.0
    } else {
        (now.duration_since(start).as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
    }
}

fn process_commands(ctx: &BuildContext) -> bool {
    let commands = COMMANDS.with(|commands| std::mem::take(&mut *commands.borrow_mut()));
    let mut opened = false;
    ENTRIES.with(|entries| {
        let mut entries = entries.borrow_mut();
        for command in commands {
            match command {
                ModalCommand::Show {
                    id,
                    animation,
                    build,
                } => {
                    for entry in entries.iter() {
                        cancel_hosted_entry(entry);
                    }
                    let timeline = Rc::new(RefCell::new(ModalTimeline::new(animation.is_some())));
                    let element = build(ctx, id, timeline.clone());
                    // A modal is a mode: while it is presented it owns the
                    // keyboard, and the tree it covers — which dispatches
                    // separately, and cannot see this content at all — owns
                    // nothing until the trap is released.
                    let focus_trap = FocusTrap::acquire();
                    let dispatcher = EventDispatcher::new().with_focus_trap(focus_trap.id());
                    entries.push(HostedModal {
                        id,
                        element,
                        animation,
                        timeline,
                        dispatcher: RefCell::new(dispatcher),
                        focus_trap: RefCell::new(Some(focus_trap)),
                    });
                    opened = true;
                }
                ModalCommand::Dismiss(id) => {
                    if let Some(entry) = entries.iter().find(|entry| entry.id == id) {
                        cancel_hosted_entry(entry);
                        entry.release_focus_trap();
                        entry
                            .timeline
                            .borrow_mut()
                            .begin_exit(entry.animation.is_some());
                    }
                }
                ModalCommand::DismissTop => {
                    if let Some(entry) = entries.last() {
                        cancel_hosted_entry(entry);
                        entry.release_focus_trap();
                        entry
                            .timeline
                            .borrow_mut()
                            .begin_exit(entry.animation.is_some());
                    }
                }
            }
        }
    });
    opened
}

fn enqueue(command: ModalCommand) {
    COMMANDS.with(|commands| commands.borrow_mut().push_back(command));
    request_animation_frame();
}

fn clear_registry() {
    COMMANDS.with(|commands| commands.borrow_mut().clear());
    ENTRIES.with(|entries| entries.borrow_mut().clear());
    LAYERS.with(|layers| layers.borrow_mut().clear());
}

pub(crate) fn show(animation: Option<ModalAnimation>, build: EntryBuilder) -> ModalHandle {
    let id = NEXT_ID.with(|next_id| {
        let id = ModalId(next_id.get());
        next_id.set(next_id.get().wrapping_add(1).max(1));
        id
    });
    let dismissed = Rc::new(Cell::new(false));
    enqueue(ModalCommand::Show {
        id,
        animation,
        build,
    });
    ModalHandle { id, dismissed }
}

pub(crate) fn dismiss(id: ModalId) {
    enqueue(ModalCommand::Dismiss(id));
}

#[cfg(test)]
pub(crate) fn reset_registry_for_test() {
    clear_registry();
    NEXT_ID.with(|next_id| next_id.set(1));
}

#[cfg(test)]
pub(crate) fn pending_command_count_for_test() -> usize {
    COMMANDS.with(|commands| commands.borrow().len())
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Duration;

    use aimer_animation::{AnimInstant, Curve};
    use aimer_attribute::Vec2d;
    use aimer_events::element::ElementEvent;
    use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};
    use aimer_widget::base::BuildContext;
    use aimer_widget::focus::{FocusNode, FocusTrap, active_focus_trap};
    use aimer_widget::{
        CaptureRequest, Drawable, Element, EventDispatcher, EventElement, EventResult,
        LayoutElement, PointerKey, Rebuildable, VisitorElement,
    };

    use super::{HostedModal, ModalId, ModalTimeline, dispatch_hosted_event};
    use crate::ModalAnimation;

    /// Builds an entry that confines focus, as `process_commands` does.
    fn trapping_entry(element: aimer_widget::AnyElement) -> HostedModal {
        let focus_trap = FocusTrap::acquire();
        let dispatcher = EventDispatcher::new().with_focus_trap(focus_trap.id());
        HostedModal {
            id: ModalId(1),
            element,
            animation: None,
            timeline: Rc::new(RefCell::new(ModalTimeline::new(false))),
            dispatcher: RefCell::new(dispatcher),
            focus_trap: RefCell::new(Some(focus_trap)),
        }
    }

    struct FocusableModalContent {
        node: FocusNode,
    }

    impl VisitorElement for FocusableModalContent {
        fn debug_name(&self) -> &'static str {
            "FocusableModalContent"
        }
    }

    impl EventElement for FocusableModalContent {
        fn focus_node(&self) -> Option<&FocusNode> {
            Some(&self.node)
        }
    }

    impl LayoutElement for FocusableModalContent {}
    impl Drawable for FocusableModalContent {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for FocusableModalContent {}

    #[test]
    fn a_hosted_entry_confines_focus_until_it_is_dismissed() {
        let entry = trapping_entry(CapturingModalElement {
            events: Rc::new(Cell::new(0)),
        }
        .boxed());

        assert_eq!(
            active_focus_trap(),
            entry.dispatcher.borrow().focus_trap(),
            "the entry dispatches inside the region it traps"
        );

        entry.release_focus_trap();
        assert_eq!(active_focus_trap(), None);

        entry.release_focus_trap();
        assert_eq!(active_focus_trap(), None, "releasing twice is harmless");
    }

    #[test]
    fn a_dropped_entry_stops_confining_focus() {
        let entry = trapping_entry(
            CapturingModalElement {
                events: Rc::new(Cell::new(0)),
            }
            .boxed(),
        );

        drop(entry);

        assert_eq!(active_focus_trap(), None);
    }

    #[test]
    fn content_of_the_trapping_entry_still_takes_focus() {
        let node = FocusNode::new();
        let entry = trapping_entry(FocusableModalContent { node: node.clone() }.boxed());

        node.request_focus();
        let _ = dispatch_hosted_event(&entry, Vec2d::default(), &ElementEvent::Cancel);

        assert!(
            node.has_focus(),
            "a modal confines focus to itself, not away from itself"
        );
        assert!(entry.dispatcher.borrow().focused().is_some());
    }

    struct CapturingModalElement {
        events: Rc<Cell<usize>>,
    }

    impl VisitorElement for CapturingModalElement {
        fn debug_name(&self) -> &'static str {
            "CapturingModalElement"
        }
    }

    impl EventElement for CapturingModalElement {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            match event {
                ElementEvent::PointerDown(pointer) => EventResult::consumed()
                    .with_pointer_capture(PointerKey::new(pointer.source, pointer.id)),
                ElementEvent::PointerUp(pointer) => EventResult::consumed()
                    .with_pointer_release(PointerKey::new(pointer.source, pointer.id)),
                _ => EventResult::consumed(),
            }
        }
    }

    impl LayoutElement for CapturingModalElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((Vec2d::default(), Vec2d { x: 10.0, y: 10.0 }))
        }
    }

    impl Drawable for CapturingModalElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for CapturingModalElement {}

    #[test]
    fn hosted_modal_routes_capture_outside_until_up() {
        let events = Rc::new(Cell::new(0));
        let entry = trapping_entry(
            CapturingModalElement {
                events: events.clone(),
            }
            .boxed(),
        );
        let pointer = PointerKey::new(PointerSource::Touch, 4);
        let down = dispatch_hosted_event(
            &entry,
            Vec2d { x: 5.0, y: 5.0 },
            &ElementEvent::PointerDown(PointerInfo::new(
                Vec2d { x: 5.0, y: 5.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );
        assert_eq!(down.capture_request(), CaptureRequest::Capture(pointer));

        let _ = dispatch_hosted_event(
            &entry,
            Vec2d { x: 50.0, y: 50.0 },
            &ElementEvent::PointerMove(PointerInfo::new(
                Vec2d { x: 50.0, y: 50.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );
        let up = dispatch_hosted_event(
            &entry,
            Vec2d { x: 50.0, y: 50.0 },
            &ElementEvent::PointerUp(PointerInfo::new(
                Vec2d { x: 50.0, y: 50.0 },
                pointer.source,
                pointer.id,
                PointerButton::Primary,
            )),
        );

        assert_eq!(events.get(), 3);
        assert_eq!(up.capture_request(), CaptureRequest::Release(pointer));
        assert_eq!(entry.dispatcher.borrow().capture_count(), 0);
    }

    #[test]
    fn a_handle_reports_its_modal_showing_until_it_is_dismissed() {
        super::reset_registry_for_test();
        let handle = super::show(None, Box::new(|_ctx, _id, _timeline| unreachable!()));

        assert!(
            handle.is_showing(),
            "a modal is showing from the moment it is asked for"
        );

        assert!(handle.dismiss());
        assert!(!handle.is_showing());
    }

    #[test]
    fn a_handle_reports_a_modal_dismissed_by_someone_else() {
        super::reset_registry_for_test();
        let handle = super::show(None, Box::new(|_ctx, _id, _timeline| unreachable!()));

        // What the barrier and `Escape` do: dismiss by id, without the handle.
        super::dismiss(handle.id());

        assert!(
            !handle.is_showing(),
            "the owner must learn its modal was closed for it"
        );
    }

    #[test]
    fn timeline_reverses_from_visible_progress_without_a_jump() {
        let animation = ModalAnimation::new()
            .enter_duration(Duration::from_millis(100))
            .exit_duration(Duration::from_millis(100))
            .enter_curve(Curve::Linear)
            .exit_curve(Curve::Linear);
        let start = AnimInstant::now();
        let mut timeline = ModalTimeline::new(true);

        timeline.tick(start, Some(animation));
        timeline.tick(start + Duration::from_millis(50), Some(animation));
        assert!((timeline.progress() - 0.5).abs() < 0.01);

        timeline.begin_exit(true);
        timeline.tick(start + Duration::from_millis(50), Some(animation));
        assert!((timeline.progress() - 0.5).abs() < 0.01);

        timeline.tick(start + Duration::from_millis(100), Some(animation));
        assert!((timeline.progress() - 0.25).abs() < 0.01);
    }

    #[test]
    fn zero_duration_timeline_reaches_both_endpoints() {
        let animation = ModalAnimation::new()
            .enter_duration(Duration::ZERO)
            .exit_duration(Duration::ZERO);
        let now = AnimInstant::now();
        let mut timeline = ModalTimeline::new(true);

        timeline.tick(now, Some(animation));
        assert_eq!(timeline.progress(), 1.0);

        timeline.begin_exit(true);
        timeline.tick(now, Some(animation));
        assert_eq!(timeline.progress(), 0.0);
        assert!(timeline.finished());
    }
}
