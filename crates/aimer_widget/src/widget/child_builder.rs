//! A child subtree a parent can place into the tree more than once.

use std::any::TypeId;
use std::cell::{Cell, OnceCell};
use std::rc::Rc;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_focus::FocusNode;

use crate::base::BuildContext;
use crate::components::diagnostics::ErrorWidget;
use crate::components::drawable::Drawable;
use crate::components::element::{Element, ElementId, VisitorElement};
use crate::components::event_element::{EventElement, EventResult};
use crate::components::layout_element::LayoutElement;
use crate::components::rebuildable::Rebuildable;
use crate::key::Key;
use crate::widget::{AnyWidgetExt, Widget};
use crate::{AnyElement, AnyWidget};

/// A child subtree its parent may place into the tree, any number of times.
///
/// # Why a parent needs one
///
/// A widget describes configuration and is *consumed* by the build that turns it
/// into an element, so a plain child value can serve exactly one build. That is
/// enough for a container, which is rebuilt by its own parent and therefore
/// receives a fresh child widget every time, but not for a widget that rebuilds
/// *itself*: a button rebuilds on hover and press, a scrollable on every offset
/// change, a theme on every tick of its transition. Each of those rebuilds needs
/// the child's element again, and a consumed widget cannot produce a second one.
///
/// Reproducing the widget is not available either — cloning it would need a
/// [`Clone`] bound the tree does not have, since an erased child is not `Clone`
/// — so the child is **retained** instead. The first build consumes the widget
/// and keeps the element it produced; every later build hands the tree a thin
/// proxy over that same element. A self-rebuilding parent therefore does not
/// rebuild its subtree at all, which is strictly cheaper than rebuilding it on
/// every hover.
///
/// # Identity
///
/// Reconciliation matches an old element to a new one by [`Widget::key`], and
/// diagnostics name a node by [`Widget::debug_name`]. Both are copied out of the
/// widget before it is stored, and the proxy reports the retained element's own
/// key and name, so a child reached through a slot keeps the identity it had
/// when it was reached directly.
///
/// # Invariant
///
/// One `ChildBuilder` describes **one position** in the tree. Cloning it is a
/// reference-count bump, which is what lets a state carry it across the rebuilds
/// of its parent, but placing two clones in two different positions would put
/// the same element in the tree twice and is a misuse.
///
/// # Example
///
/// ```
/// use aimer_widget::{ChildBuilder, ErrorWidget};
///
/// let child = ChildBuilder::from_widget(ErrorWidget::new("missing route"));
///
/// // The builder is a widget itself, so it can be handed to any parent, and
/// // it reports the name of the widget it was made from.
/// assert_eq!(aimer_widget::Widget::debug_name(&child), "ErrorWidget");
/// ```
pub struct ChildBuilder {
    source: Source,
    key: Option<Key>,
    debug_name: &'static str,
}

/// Where a [`ChildBuilder`] gets its subtree from.
///
/// The un-attached case is the absent handle rather than a variant beside it:
/// both attached forms are one refcounted [`Child`], so `None` lives in that
/// count's null niche and a waiting builder is exactly one word — no
/// allocation, no discriminant. A parent therefore holds one plain field, and
/// the question `None` asks is already settled by its type-state.
type Source = Option<Rc<Child>>;

/// The child a slot and all of its placements share.
struct Child {
    /// Where this child's widget comes from.
    origin: Origin,
    /// The element the first build produced, for a child that is retained.
    ///
    /// Written at most once and never replaced, which is why a [`OnceCell`] is
    /// enough: a placement can hand out a borrow of the child for as long as it
    /// holds its own reference, with no borrow flag on the hot path and no
    /// `unsafe`.
    element: OnceCell<AnyElement>,
}

/// How a [`Child`] produces its widget.
enum Origin {
    /// One widget, taken by the first build; the element is then reused.
    Once(Cell<Option<AnyWidget>>),
    /// A closure asked again on every build, because that is what a closure is
    /// for.
    Every(Box<dyn Fn() -> AnyWidget>),
}

impl Child {
    /// Produces this child's element for one placement in the tree.
    #[inline]
    fn build(self: &Rc<Self>, ctx: &BuildContext) -> AnyElement {
        match &self.origin {
            Origin::Every(build) => build().into_element(ctx),
            Origin::Once(widget) => {
                match self.element.get() {
                    None => {
                        let widget = widget
                            .take()
                            .expect("a retained child holds either its widget or its element");
                        self.element
                            .set(widget.into_element(ctx))
                            .unwrap_or_else(|_| unreachable!("the cell was empty one line above"));
                    }
                    // A later placement means the parent rebuilt itself, and a
                    // parent rebuild is exactly when the values it publishes
                    // change — a theme mid-transition, a provider's new value.
                    // The child's element survives, so its state, scroll offsets
                    // and GPU resources do, but the build closures inside it are
                    // asked again so they observe the new context.
                    Some(element) => element.mark_needs_rebuild(),
                }
                RetainedChildElement(Rc::clone(self)).boxed()
            }
        }
    }

    /// Returns the retained element if this child has been built and kept.
    #[inline]
    fn built(&self) -> Option<&AnyElement> {
        self.element.get()
    }
}

impl ChildBuilder {
    /// Stores a widget as a subtree that can be placed repeatedly.
    ///
    /// The widget is moved in, and moved out again by the first build. Attaching
    /// a child therefore costs one allocation and no copy of the widget's
    /// fields.
    ///
    /// # Example
    ///
    /// ```
    /// use aimer_widget::{ChildBuilder, ErrorWidget, Widget};
    ///
    /// let child = ChildBuilder::from_widget(ErrorWidget::new("boom"));
    ///
    /// assert_eq!(child.debug_name(), "ErrorWidget");
    /// ```
    #[inline]
    pub fn from_widget<W: Widget + 'static>(widget: W) -> Self {
        let key = widget.key();
        let debug_name = widget.debug_name();
        Self {
            source: Some(Rc::new(Child {
                origin: Origin::Once(Cell::new(Some(widget.boxed()))),
                element: OnceCell::new(),
            })),
            key,
            debug_name,
        }
    }

    /// Stores a closure that produces a fresh subtree on every call.
    ///
    /// Use this when the subtree is not a single value to keep — when it
    /// depends on data read at build time, or when the caller would otherwise
    /// have to keep a widget alive only to describe it. Unlike
    /// [`ChildBuilder::from_widget`], the subtree is rebuilt on every build,
    /// because a closure exists precisely to answer differently next time.
    ///
    /// The builder cannot know a key or a name in advance here, so it reports
    /// none and is named after itself.
    ///
    /// # Example
    ///
    /// ```
    /// use aimer_widget::{ChildBuilder, ErrorWidget, Widget};
    ///
    /// let child = ChildBuilder::new(|| ErrorWidget::new("rebuilt").boxed());
    ///
    /// assert_eq!(child.key(), None);
    /// ```
    #[inline]
    pub fn new(build: impl Fn() -> AnyWidget + 'static) -> Self {
        Self {
            source: Some(Rc::new(Child {
                origin: Origin::Every(Box::new(build)),
                element: OnceCell::new(),
            })),
            key: None,
            debug_name: "ChildBuilder",
        }
    }

    /// The placeholder a parent holds until its child is attached.
    ///
    /// A parent in this framework is completed by its child, and the builder
    /// spells that out in the type: a parent still holding
    /// [`RequiredChild`](crate::RequiredChild) is not a [`Widget`], and only
    /// attaching a child makes it one. This value fills the slot in the
    /// meantime, so a half-built parent stays a plain value instead of an
    /// `Option` that every build would have to unwrap.
    ///
    /// It allocates nothing: a builder waiting for its child holds no child
    /// yet, which matters because parents are constructed on every frame.
    ///
    /// A well-typed tree never builds it. One that somehow does gets an
    /// [`ErrorWidget`] naming the mistake rather than a panic.
    ///
    /// # Example
    ///
    /// ```
    /// use aimer_widget::{ChildBuilder, Widget};
    ///
    /// assert_eq!(ChildBuilder::required().debug_name(), "RequiredChild");
    /// ```
    #[inline]
    pub fn required() -> Self {
        Self {
            source: None,
            key: None,
            debug_name: "RequiredChild",
        }
    }

    /// Produces the element for this child's position.
    ///
    /// A stored widget is built on the first call and reused by every later one;
    /// a stored closure is called every time.
    pub fn build(&self, ctx: &BuildContext) -> AnyElement {
        match &self.source {
            Some(child) => child.build(ctx),
            None => ErrorWidget::new("a widget was built before its required child was attached")
                .to_element(ctx),
        }
    }
}

/// Cloning shares the child instead of duplicating it, which is what lets a
/// state keep it across the rebuilds of its parent.
impl Clone for ChildBuilder {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            key: self.key.clone(),
            debug_name: self.debug_name,
        }
    }
}

impl Widget for ChildBuilder {
    #[inline]
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    #[inline]
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        self.build(ctx)
    }

    #[inline]
    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}

/// The element a parent places into the tree for a retained child.
///
/// It owns no subtree of its own: every question is forwarded to the one
/// retained element, so layout, painting, event dispatch, reconciliation
/// identity and diagnostics are the child's own and not the proxy's.
struct RetainedChildElement(Rc<Child>);

impl RetainedChildElement {
    /// Returns the retained child.
    ///
    /// A placement only exists after the child was built, so the child is
    /// always there; the fallback keeps a misuse from panicking during a paint.
    #[inline]
    fn child(&self) -> Option<&dyn Element> {
        self.0.built().map(|element| element.as_ref())
    }
}

impl VisitorElement for RetainedChildElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        if let Some(child) = self.child() {
            visitor(child);
        }
    }

    fn debug_name(&self) -> &'static str {
        self.child()
            .map(VisitorElement::debug_name)
            .unwrap_or("RetainedChild")
    }

    fn element_type_id(&self) -> TypeId {
        self.child()
            .map(VisitorElement::element_type_id)
            .unwrap_or_else(|| TypeId::of::<Self>())
    }

    fn reconciliation_key(&self) -> Option<&Key> {
        self.child().and_then(VisitorElement::reconciliation_key)
    }

    fn element_id(&self) -> Option<ElementId> {
        self.child().and_then(VisitorElement::element_id)
    }

    fn set_element_id(&self, id: ElementId) {
        if let Some(child) = self.child() {
            child.set_element_id(id);
        }
    }
}

impl LayoutElement for RetainedChildElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child().and_then(LayoutElement::pos)
    }

    fn size(&self) -> Option<Size> {
        self.child().and_then(LayoutElement::size)
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child()
            .map(|child| child.layout(ctx))
            .unwrap_or_default()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child()
            .map(|child| child.computed_size(ctx))
            .unwrap_or_default()
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child()
            .map(|child| child.content_size(ctx))
            .unwrap_or_default()
    }

    fn layer(&self) -> u32 {
        self.child().map(LayoutElement::layer).unwrap_or_default()
    }

    fn flex(&self) -> Option<f32> {
        self.child().and_then(LayoutElement::flex)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child().and_then(LayoutElement::get_size_from_child)
    }

    fn invalidate_layout(&self) {
        if let Some(child) = self.child() {
            child.invalidate_layout();
        }
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child().and_then(LayoutElement::pos_start_end)
    }
}

impl EventElement for RetainedChildElement {
    fn focus_node(&self) -> Option<&FocusNode> {
        self.child().and_then(EventElement::focus_node)
    }

    fn autofocus(&self) -> bool {
        self.child().map(EventElement::autofocus).unwrap_or_default()
    }

    fn traps_focus(&self) -> bool {
        self.child()
            .map(EventElement::traps_focus)
            .unwrap_or_default()
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.child()
            .map(|child| child.on_event(event))
            .unwrap_or_default()
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        if let Some(child) = self.child() {
            visitor(child);
        }
    }

    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        if let Some(child) = self.child() {
            visitor(child);
        }
    }
}

impl Drawable for RetainedChildElement {
    fn draw(&self, ctx: &BuildContext) {
        if let Some(child) = self.child() {
            child.draw(ctx);
        }
    }
}

impl Rebuildable for RetainedChildElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        if let Some(child) = self.child() {
            child.rebuild_if_dirty(ctx);
        }
    }

    fn mark_needs_rebuild(&self) {
        if let Some(child) = self.child() {
            child.mark_needs_rebuild();
        }
    }

    /// Adopts nothing.
    ///
    /// The element this one replaces is a placement of the *same* retained
    /// child, so there is no state to move: it never left.
    fn adopt_runtime_state_from(&self, _old: &dyn Element) {}
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_attribute::size::ResolvedSize;

    use super::*;
    use crate::base::WindowHandle;

    struct Probe {
        builds: Rc<Cell<usize>>,
    }

    impl Widget for Probe {
        fn to_element(self, ctx: &BuildContext) -> AnyElement {
            self.builds.set(self.builds.get() + 1);
            ErrorWidget::new("probe").to_element(ctx)
        }

        fn key(&self) -> Option<Key> {
            Some(Key::Static("probe"))
        }

        fn debug_name(&self) -> &'static str {
            "Probe"
        }
    }

    fn context() -> BuildContext<'static> {
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        BuildContext::new(
            canvas,
            ResolvedSize::default(),
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        )
    }

    #[tokio::test]
    async fn a_stored_widget_is_built_once_however_often_it_is_placed() {
        let builds = Rc::new(Cell::new(0));
        let child = ChildBuilder::from_widget(Probe {
            builds: Rc::clone(&builds),
        });
        let ctx = context();

        for _ in 0..8 {
            drop(child.build(&ctx));
        }

        assert_eq!(
            builds.get(),
            1,
            "a parent rebuilding itself must not rebuild its child subtree"
        );
    }

    #[tokio::test]
    async fn every_placement_reaches_the_same_element() {
        let child = ChildBuilder::from_widget(Probe {
            builds: Rc::new(Cell::new(0)),
        });
        let ctx = context();

        let first = child.build(&ctx);
        let second = child.build(&ctx);

        let mut addresses = Vec::new();
        for placement in [&first, &second] {
            placement.visit_children(&mut |retained| {
                addresses.push(retained as *const dyn Element as *const ());
            });
        }

        assert_eq!(addresses.len(), 2);
        assert_eq!(
            addresses[0], addresses[1],
            "every placement must reach the element the first build produced"
        );
    }

    #[tokio::test]
    async fn a_placement_keeps_the_identity_of_the_retained_child() {
        let child = ChildBuilder::from_widget(Probe {
            builds: Rc::new(Cell::new(0)),
        });
        let ctx = context();

        let placement = child.build(&ctx);

        assert_eq!(
            placement.debug_name(),
            "ErrorWidget",
            "diagnostics must name the child, not its holder"
        );
        assert_eq!(
            child.key(),
            Some(Key::Static("probe")),
            "reconciliation matches on the key of the widget, not of its holder"
        );
        assert_eq!(child.debug_name(), "Probe");
    }

    #[tokio::test]
    async fn a_builder_is_a_widget_a_parent_can_take_as_its_child() {
        let builds = Rc::new(Cell::new(0));
        let child = ChildBuilder::from_widget(Probe {
            builds: Rc::clone(&builds),
        });
        let ctx = context();

        drop(Widget::to_element(child, &ctx));

        assert_eq!(builds.get(), 1);
    }

    #[tokio::test]
    async fn a_clone_places_the_same_child() {
        let builds = Rc::new(Cell::new(0));
        let child = ChildBuilder::from_widget(Probe {
            builds: Rc::clone(&builds),
        });
        let carried = child.clone();
        let ctx = context();

        drop(child.build(&ctx));
        drop(carried.build(&ctx));

        assert_eq!(
            builds.get(),
            1,
            "a clone shares the child instead of building another one"
        );
    }

    #[test]
    fn waiting_for_a_child_costs_no_space_and_no_allocation() {
        assert_eq!(
            size_of::<Source>(),
            size_of::<usize>(),
            "an un-attached child has to live in the null niche of the count"
        );
        assert!(ChildBuilder::required().source.is_none());
    }

    #[tokio::test]
    async fn an_unattached_child_reports_the_mistake_instead_of_panicking() {
        let ctx = context();

        let element = ChildBuilder::required().build(&ctx);

        assert_eq!(element.debug_name(), "ErrorWidget");
    }

    #[tokio::test]
    async fn a_closure_produces_a_fresh_widget_for_every_build() {
        let builds = Rc::new(Cell::new(0));
        let child = ChildBuilder::new({
            let builds = Rc::clone(&builds);
            move || {
                Probe {
                    builds: Rc::clone(&builds),
                }
                .boxed()
            }
        });
        let ctx = context();

        drop(child.build(&ctx));
        drop(child.build(&ctx));

        assert_eq!(builds.get(), 2);
        assert_eq!(
            child.debug_name(),
            "ChildBuilder",
            "a closure has no widget to take a name from until it runs"
        );
    }
}
