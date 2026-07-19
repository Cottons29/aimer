use std::any::type_name;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::{Rc, Weak};

use aimer_widget::base::{BuildConsumer, BuildContext, ResolvedSize, Size, Vec2d, WindowHandle};
use aimer_widget::{
    AnyWidget, Drawable, Element, EventElement, LayoutElement, Rebuildable, RequiredChild, State,
    StateUpdater, StatefulElement, StatefulWidget, VisitorElement, Widget,
};

struct Subscriber<T> {
    consumer: Weak<BuildConsumer>,
    should_notify: Box<dyn FnMut(&T) -> bool>,
}

struct ProviderStore<T> {
    value: RefCell<Rc<T>>,
    subscribers: RefCell<HashMap<u64, Subscriber<T>>>,
    next_subscriber: Cell<u64>,
}

/// An immutable, cheaply cloneable version of a provided value.
///
/// A snapshot dereferences to `T`, so fields and methods can be accessed directly. It keeps the
/// version observed at read time alive even after the provider is updated.
pub struct Snapshot<T>(Rc<T>);

impl<T> Clone for Snapshot<T> {
    fn clone(&self) -> Self {
        Self(
            self.0
                .clone(),
        )
    }
}

impl<T> Deref for Snapshot<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
    }
}

/// A handle for reading and updating one provided value outside a widget build.
///
/// Cloning a handle keeps access to the same provider store. Reading creates a cheap [`Snapshot`],
/// while updating uses copy-on-write to preserve snapshots that are still alive.
pub struct ProviderHandle<T>(Rc<ProviderStore<T>>);

impl<T> Clone for ProviderHandle<T> {
    fn clone(&self) -> Self {
        Self(
            self.0
                .clone(),
        )
    }
}

impl<T: 'static> ProviderHandle<T> {
    /// Creates a handle containing `value` for use by a provider scope.
    pub fn new(value: T) -> Self {
        Self(Rc::new(ProviderStore {
            value: RefCell::new(Rc::new(value)),
            subscribers: RefCell::new(HashMap::new()),
            next_subscriber: Cell::new(0),
        }))
    }

    /// Returns an immutable snapshot of the current value without subscribing a widget.
    pub fn read(&self) -> Snapshot<T> {
        Snapshot(
            self.0
                .value
                .borrow()
                .clone(),
        )
    }

    /// Returns the nearest handle for `T`, or `None` when no matching provider is in scope.
    pub fn try_of(context: &BuildContext) -> Option<Self> {
        context
            .get_state::<Provided<T>>()
            .map(|provided| {
                provided
                    .0
                    .clone()
            })
    }

    /// Returns the nearest handle for `T`.
    ///
    /// # Panics
    ///
    /// Panics when no matching provider is in the current widget scope.
    pub fn of(context: &BuildContext) -> Self {
        Self::try_of(context).unwrap_or_else(|| {
            panic!("No provider for `{}` found in the current widget scope", type_name::<T>())
        })
    }

    /// Mutates the current value and notifies subscribed widgets.
    ///
    /// The value is cloned only when an existing [`Snapshot`] still shares the previous version.
    pub fn update(&self, mutation: impl FnOnce(&mut T))
    where
        T: Clone,
    {
        {
            let mut value = self
                .0
                .value
                .try_borrow_mut()
                .expect("provider value is already borrowed during an update");
            mutation(Rc::make_mut(&mut value));
        }
        self.notify();
    }

    /// Applies an action through a reducer and notifies subscribed widgets.
    pub fn dispatch<A>(&self, action: A, reducer: impl FnOnce(&mut T, A))
    where
        T: Clone,
    {
        self.update(|value| reducer(value, action));
    }

    fn notify(&self) {
        let value = self
            .0
            .value
            .borrow();
        self.0
            .subscribers
            .borrow_mut()
            .retain(|_, subscriber| {
                let Some(consumer) = subscriber
                    .consumer
                    .upgrade()
                else {
                    return false;
                };
                if (subscriber.should_notify)(value.as_ref()) {
                    consumer.mark_needs_rebuild();
                }
                true
            });
    }

    fn add_subscriber(
        &self,
        consumer: &Rc<BuildConsumer>,
        window: &WindowHandle,
        should_notify: impl FnMut(&T) -> bool + 'static,
    ) {
        let id = self
            .0
            .next_subscriber
            .get();
        self.0
            .next_subscriber
            .set(id.wrapping_add(1));
        let window = window.clone();
        let mut should_notify = should_notify;
        self.0
            .subscribers
            .borrow_mut()
            .insert(
                id,
                Subscriber {
                    consumer: Rc::downgrade(consumer),
                    should_notify: Box::new(move |value| {
                        let notify = should_notify(value);
                        if notify {
                            window.request_redraw();
                        }
                        notify
                    }),
                },
            );
        let store = Rc::downgrade(&self.0);
        consumer.add_cleanup(move || {
            if let Some(store) = store.upgrade() {
                store
                    .subscribers
                    .borrow_mut()
                    .remove(&id);
            }
        });
    }

    fn subscribe_watch(&self, consumer: &Rc<BuildConsumer>, window: &WindowHandle) {
        let identity = Rc::as_ptr(&self.0) as usize;
        if consumer.register_dependency(identity) {
            self.add_subscriber(consumer, window, |_| true);
        }
    }

    fn subscribe_selector<R: PartialEq + 'static>(
        &self,
        consumer: &Rc<BuildConsumer>,
        window: &WindowHandle,
        selector: impl Fn(&T) -> R + 'static,
    ) {
        let mut selected = selector(
            self.0
                .value
                .borrow()
                .as_ref(),
        );
        self.add_subscriber(consumer, window, move |value| {
            let next = selector(value);
            if next == selected {
                false
            } else {
                selected = next;
                true
            }
        });
    }

    #[cfg(test)]
    fn subscriber_count(&self) -> usize {
        self.0
            .subscribers
            .borrow()
            .len()
    }
}

struct Provided<T>(ProviderHandle<T>);

impl<T> Clone for Provided<T> {
    fn clone(&self) -> Self {
        Self(
            self.0
                .clone(),
        )
    }
}

struct StoreDispatcher<A>(Rc<dyn Fn(A)>);
type StoreReducer<T, A> = dyn Fn(&mut T, A);

impl<A> Clone for StoreDispatcher<A> {
    fn clone(&self) -> Self {
        Self(
            self.0
                .clone(),
        )
    }
}

/// Accesses providers from a [`BuildContext`].
///
/// [`read`](ProviderContext::read) observes a value without registering a dependency, while
/// [`watch`](ProviderContext::watch) rebuilds the current widget after any update. Use
/// [`select`](ProviderContext::select) when only a projection should trigger rebuilding.
pub trait ProviderContext {
    /// Returns a snapshot of the nearest provided `T`, without subscribing, if one exists.
    fn try_read<T: 'static>(&self) -> Option<Snapshot<T>>;

    /// Returns a snapshot of the nearest provided `T` without subscribing.
    ///
    /// # Panics
    ///
    /// Panics when no matching provider is in the current widget scope.
    fn read<T: 'static>(&self) -> Snapshot<T>;

    /// Returns and subscribes to the nearest provided `T`, if one exists.
    ///
    /// # Panics
    ///
    /// Panics when called outside a widget build while a matching provider exists.
    fn try_watch<T: 'static>(&self) -> Option<Snapshot<T>>;

    /// Returns and subscribes to the nearest provided `T`.
    ///
    /// # Panics
    ///
    /// Panics when no matching provider exists or when called outside a widget build.
    fn watch<T: 'static>(&self) -> Snapshot<T>;

    /// Returns a projection and rebuilds the current widget only when that projection changes.
    fn select<T: 'static, R: PartialEq + 'static>(&self, selector: impl Fn(&T) -> R + 'static)
    -> R;

    /// Mutates the nearest provided `T` and notifies its subscribers.
    fn update<T: Clone + 'static>(&self, mutation: impl FnOnce(&mut T));

    /// Dispatches an action to the nearest store accepting `A`.
    fn dispatch<A: 'static>(&self, action: A);
}

impl ProviderContext for BuildContext<'_> {
    fn try_read<T: 'static>(&self) -> Option<Snapshot<T>> {
        self.get_state::<Provided<T>>()
            .map(|provided| {
                provided
                    .0
                    .read()
            })
    }

    fn read<T: 'static>(&self) -> Snapshot<T> {
        self.try_read::<T>()
            .unwrap_or_else(|| {
                panic!("No provider for `{}` found in the current widget scope", type_name::<T>())
            })
    }

    fn try_watch<T: 'static>(&self) -> Option<Snapshot<T>> {
        let provided = self.get_state::<Provided<T>>()?;
        let consumer = self
            .current_build_consumer()
            .unwrap_or_else(|| {
                panic!("watch::<{}>() must be called while building a widget", type_name::<T>())
            });
        provided
            .0
            .subscribe_watch(&consumer, &self.window);
        Some(
            provided
                .0
                .read(),
        )
    }

    fn watch<T: 'static>(&self) -> Snapshot<T> {
        self.try_watch::<T>()
            .unwrap_or_else(|| {
                panic!("No provider for `{}` found in the current widget scope", type_name::<T>())
            })
    }

    fn select<T: 'static, R: PartialEq + 'static>(
        &self,
        selector: impl Fn(&T) -> R + 'static,
    ) -> R {
        let provided = self
            .get_state::<Provided<T>>()
            .unwrap_or_else(|| {
                panic!("No provider for `{}` found in the current widget scope", type_name::<T>())
            });
        let consumer = self
            .current_build_consumer()
            .unwrap_or_else(|| {
                panic!("select::<{}>() must be called while building a widget", type_name::<T>())
            });
        let selected = selector(
            &provided
                .0
                .read(),
        );
        provided
            .0
            .subscribe_selector(&consumer, &self.window, selector);
        selected
    }

    fn update<T: Clone + 'static>(&self, mutation: impl FnOnce(&mut T)) {
        let provided = self
            .get_state::<Provided<T>>()
            .unwrap_or_else(|| {
                panic!("No provider for `{}` found in the current widget scope", type_name::<T>())
            });
        provided
            .0
            .update(mutation);
    }

    fn dispatch<A: 'static>(&self, action: A) {
        let dispatcher = self
            .get_state::<StoreDispatcher<A>>()
            .unwrap_or_else(|| {
                panic!(
                    "No store accepting `{}` found in the current widget scope",
                    type_name::<A>()
                )
            });
        (dispatcher.0)(action);
    }
}

/// Provides one state value to a descendant widget subtree.
///
/// Construct a provider with [`Provider::new`], configure its initializer with
/// [`Provider::create`], and attach the subtree with [`Provider::child`]. Descendants access the
/// value through [`ProviderContext`] or [`ProviderHandle`].
pub struct Provider<T, W = RequiredChild> {
    create: Option<Rc<dyn Fn() -> T>>,
    handle: Option<ProviderHandle<T>>,
    child: Rc<W>,
}

/// An alias for [`Provider`] used when the provided value is updated directly.
pub type NotifierProvider<T, W = RequiredChild> = Provider<T, W>;

impl<T> Provider<T> {
    /// Creates an unconfigured provider.
    pub fn new() -> Self {
        Self { create: None, handle: None, child: Rc::new(RequiredChild) }
    }
}

impl<T> Default for Provider<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, W> Provider<T, W> {
    /// Sets the initializer called once when the provider state is created.
    pub fn create(mut self, create: impl Fn() -> T + 'static) -> Self {
        self.create = Some(Rc::new(create));
        self
    }

    /// Uses an existing handle instead of creating a new provider value.
    pub fn handle(mut self, handle: ProviderHandle<T>) -> Self {
        self.handle = Some(handle);
        self
    }

    /// Attaches the descendant widget subtree and produces a valid widget.
    pub fn child<C: Widget>(self, child: C) -> Provider<T, C> {
        Provider { create: self.create, handle: self.handle, child: Rc::new(child) }
    }

    /// Attaches the descendant subtree and type-erases the completed provider widget.
    ///
    /// This is equivalent to calling [`Provider::child`] followed by [`Widget::boxed`]. Use it
    /// when different code paths must return one [`AnyWidget`] type.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget
    where
        T: 'static,
    {
        self.child(child).boxed()
    }
}

#[doc(hidden)]
pub struct ProviderState<T, W> {
    handle: ProviderHandle<T>,
    child: Rc<W>,
}

impl<T: 'static, W: Widget + 'static> StatefulWidget for Provider<T, W> {
    type State = ProviderState<T, W>;

    fn create_state(&self) -> Self::State {
        let handle = self
            .handle
            .clone()
            .unwrap_or_else(|| {
                let create = self
                    .create
                    .as_ref()
                    .unwrap_or_else(|| {
                        panic!(
                            "Provider::<{}>::create or handle must be called before child",
                            type_name::<T>()
                        )
                    });
                ProviderHandle::new(create())
            });
        ProviderState {
            handle,
            child: self
                .child
                .clone(),
        }
    }
}

impl<T: 'static, W: Widget + 'static> State<Provider<T, W>> for ProviderState<T, W> {
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: &Self) {
        self.child = new
            .child
            .clone();
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        ProviderScope {
            handle: self
                .handle
                .clone(),
            child: self
                .child
                .clone(),
        }
    }
}

impl<T: 'static, W: Widget + 'static> Widget for Provider<T, W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new_with_name(self, ctx, type_name::<Provider<T>>(), None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        type_name::<Provider<T>>()
    }
}

struct ProviderScope<T, W> {
    handle: ProviderHandle<T>,
    child: Rc<W>,
}

impl<T: 'static, W: Widget + 'static> Widget for ProviderScope<T, W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = ctx.with_state(
            Provided(
                self.handle
                    .clone(),
            ),
            |ctx| {
                self.child
                    .to_element(ctx)
            },
        );
        Box::new(ProviderElement {
            handle: self
                .handle
                .clone(),
            child,
        })
    }
}

struct ProviderElement<T> {
    handle: ProviderHandle<T>,
    child: Box<dyn Element>,
}

impl<T: 'static> ProviderElement<T> {
    fn scoped<R>(&self, ctx: &BuildContext, callback: impl FnOnce(&BuildContext) -> R) -> R {
        ctx.with_state(
            Provided(
                self.handle
                    .clone(),
            ),
            callback,
        )
    }
}

impl<T: 'static> VisitorElement for ProviderElement<T> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(
            self.child
                .as_ref(),
        );
    }

    fn debug_name(&self) -> &'static str {
        "ProviderScope"
    }
}

impl<T: 'static> Drawable for ProviderElement<T> {
    fn draw(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| {
            self.child
                .draw(ctx)
        });
    }
}

impl<T: 'static> LayoutElement for ProviderElement<T> {
    fn pos(&self) -> Option<Vec2d> {
        self.child
            .pos()
    }
    fn size(&self) -> Option<Size> {
        self.child
            .size()
    }
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| {
            self.child
                .layout(ctx)
        })
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| {
            self.child
                .computed_size(ctx)
        })
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| {
            self.child
                .content_size(ctx)
        })
    }
    fn layer(&self) -> u32 {
        self.child
            .layer()
    }
    fn flex(&self) -> Option<f32> {
        self.child
            .flex()
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.child
            .get_size_from_child()
    }
    fn invalidate_layout(&self) {
        self.child
            .invalidate_layout();
    }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child
            .pos_start_end()
    }
}

impl<T: 'static> EventElement for ProviderElement<T> {}

impl<T: 'static> Rebuildable for ProviderElement<T> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| {
            self.child
                .rebuild_if_dirty(ctx)
        });
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.scoped(ctx, callback);
    }

    fn is_carry_state(&self) -> bool {
        true
    }

    fn mark_needs_rebuild(&self) {
        self.child
            .mark_needs_rebuild();
    }
}

/// Provides reducer-managed state to a descendant widget subtree.
///
/// Descendants read or watch `T` through [`ProviderContext`] and send `A` values with
/// [`ProviderContext::dispatch`].
pub struct StoreProvider<T, A, W = RequiredChild> {
    create: Option<Rc<dyn Fn() -> T>>,
    reducer: Option<Rc<StoreReducer<T, A>>>,
    child: Rc<W>,
}

impl<T, A> StoreProvider<T, A> {
    /// Creates an unconfigured store provider.
    pub fn new() -> Self {
        Self { create: None, reducer: None, child: Rc::new(RequiredChild) }
    }
}

impl<T, A> Default for StoreProvider<T, A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, A, W> StoreProvider<T, A, W> {
    /// Sets the initializer called once when the store state is created.
    pub fn create(mut self, create: impl Fn() -> T + 'static) -> Self {
        self.create = Some(Rc::new(create));
        self
    }
    /// Sets the reducer used to apply dispatched actions.
    pub fn reducer(mut self, reducer: impl Fn(&mut T, A) + 'static) -> Self {
        self.reducer = Some(Rc::new(reducer));
        self
    }
    /// Attaches the descendant widget subtree and produces a valid widget.
    pub fn child<C: Widget>(self, child: C) -> StoreProvider<T, A, C> {
        StoreProvider { create: self.create, reducer: self.reducer, child: Rc::new(child) }
    }

    /// Attaches the descendant subtree and type-erases the completed store provider widget.
    ///
    /// This is equivalent to calling [`StoreProvider::child`] followed by [`Widget::boxed`]. Use
    /// it when different code paths must return one [`AnyWidget`] type.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget
    where
        T: Clone + 'static,
        A: 'static,
    {
        self.child(child).boxed()
    }
}

#[doc(hidden)]
pub struct StoreState<T, A, W> {
    handle: ProviderHandle<T>,
    reducer: Rc<StoreReducer<T, A>>,
    child: Rc<W>,
}

impl<T: Clone + 'static, A: 'static, W: Widget + 'static> StatefulWidget
    for StoreProvider<T, A, W>
{
    type State = StoreState<T, A, W>;
    fn create_state(&self) -> Self::State {
        let create = self
            .create
            .as_ref()
            .unwrap_or_else(|| {
                panic!("StoreProvider::<{}>::create must be called before child", type_name::<T>())
            });
        let reducer = self
            .reducer
            .as_ref()
            .unwrap_or_else(|| {
                panic!("StoreProvider::<{}>::reducer must be called before child", type_name::<T>())
            });
        StoreState {
            handle: ProviderHandle::new(create()),
            reducer: reducer.clone(),
            child: self
                .child
                .clone(),
        }
    }
}

impl<T: Clone + 'static, A: 'static, W: Widget + 'static> State<StoreProvider<T, A, W>>
    for StoreState<T, A, W>
{
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}
    fn adopt_config_from(&mut self, new: &Self) {
        self.reducer = new
            .reducer
            .clone();
        self.child = new
            .child
            .clone();
    }
    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        StoreScope {
            handle: self
                .handle
                .clone(),
            reducer: self
                .reducer
                .clone(),
            child: self
                .child
                .clone(),
        }
    }
}

impl<T: Clone + 'static, A: 'static, W: Widget + 'static> Widget for StoreProvider<T, A, W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new_with_name(self, ctx, type_name::<StoreProvider<T, A>>(), None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        type_name::<StoreProvider<T, A>>()
    }
}

struct StoreScope<T, A, W> {
    handle: ProviderHandle<T>,
    reducer: Rc<StoreReducer<T, A>>,
    child: Rc<W>,
}

impl<T: Clone + 'static, A: 'static, W: Widget + 'static> Widget for StoreScope<T, A, W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let handle = self
            .handle
            .clone();
        let reducer = self
            .reducer
            .clone();
        let dispatcher = StoreDispatcher(Rc::new(move |action| {
            let reducer = reducer.clone();
            handle.update(|state| reducer(state, action));
        }));
        let child = ctx.with_state(
            Provided(
                self.handle
                    .clone(),
            ),
            |ctx| {
                ctx.with_state(dispatcher.clone(), |ctx| {
                    self.child
                        .to_element(ctx)
                })
            },
        );
        Box::new(StoreElement {
            handle: self
                .handle
                .clone(),
            dispatcher,
            child,
            marker: PhantomData,
        })
    }
}

struct StoreElement<T, A> {
    handle: ProviderHandle<T>,
    dispatcher: StoreDispatcher<A>,
    child: Box<dyn Element>,
    marker: PhantomData<A>,
}

impl<T: 'static, A: 'static> StoreElement<T, A> {
    fn scoped<R>(&self, ctx: &BuildContext, callback: impl FnOnce(&BuildContext) -> R) -> R {
        ctx.with_state(
            Provided(
                self.handle
                    .clone(),
            ),
            |ctx| {
                ctx.with_state(
                    self.dispatcher
                        .clone(),
                    callback,
                )
            },
        )
    }
}

impl<T: 'static, A: 'static> VisitorElement for StoreElement<T, A> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(
            self.child
                .as_ref(),
        );
    }
    fn debug_name(&self) -> &'static str {
        "StoreProviderScope"
    }
}
impl<T: 'static, A: 'static> Drawable for StoreElement<T, A> {
    fn draw(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| {
            self.child
                .draw(ctx)
        });
    }
}
impl<T: 'static, A: 'static> LayoutElement for StoreElement<T, A> {
    fn pos(&self) -> Option<Vec2d> {
        self.child
            .pos()
    }
    fn size(&self) -> Option<Size> {
        self.child
            .size()
    }
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| {
            self.child
                .layout(ctx)
        })
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| {
            self.child
                .computed_size(ctx)
        })
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| {
            self.child
                .content_size(ctx)
        })
    }
    fn layer(&self) -> u32 {
        self.child
            .layer()
    }
    fn flex(&self) -> Option<f32> {
        self.child
            .flex()
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.child
            .get_size_from_child()
    }
    fn invalidate_layout(&self) {
        self.child
            .invalidate_layout();
    }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child
            .pos_start_end()
    }
}
impl<T: 'static, A: 'static> EventElement for StoreElement<T, A> {}
impl<T: 'static, A: 'static> Rebuildable for StoreElement<T, A> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| {
            self.child
                .rebuild_if_dirty(ctx)
        });
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.scoped(ctx, callback);
    }

    fn is_carry_state(&self) -> bool {
        true
    }

    fn mark_needs_rebuild(&self) {
        self.child
            .mark_needs_rebuild();
    }
}

#[cfg(test)]
mod tests {
    use std::any::{Any, TypeId};
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::RwLock;

    use aimer_widget::base::{BuildConsumer, BuildContext, WindowHandle};
    use aimer_widget::{
        Drawable, EventElement, LayoutElement, Rebuildable, State, StateUpdater, StatefulElement,
        StatefulWidget, VisitorElement, Widget,
    };

    use super::*;

    #[derive(Clone, Debug, Default, PartialEq)]
    struct Counter {
        count: usize,
        label: &'static str,
    }

    struct NonClone {
        value: usize,
    }

    struct Leaf;

    impl VisitorElement for Leaf {
        fn debug_name(&self) -> &'static str {
            "Leaf"
        }
    }
    impl Drawable for Leaf {
        fn draw(&self, _context: &BuildContext) {}
    }
    impl EventElement for Leaf {}
    impl LayoutElement for Leaf {}
    impl Rebuildable for Leaf {}

    struct ReadingWidget {
        observed: Rc<Cell<usize>>,
    }

    impl Widget for ReadingWidget {
        fn to_element(&self, context: &BuildContext) -> Box<dyn Element> {
            self.observed
                .set(ProviderContext::read::<Counter>(context).count);
            Box::new(Leaf)
        }
    }

    struct LeafWidget;

    impl Widget for LeafWidget {
        fn to_element(&self, _context: &BuildContext) -> Box<dyn Element> {
            Box::new(Leaf)
        }
    }

    struct WatchingWidget {
        builds: Rc<Cell<usize>>,
        handle: Rc<RefCell<Option<ProviderHandle<Counter>>>>,
        select_count: bool,
    }

    struct WatchingState {
        builds: Rc<Cell<usize>>,
        handle: Rc<RefCell<Option<ProviderHandle<Counter>>>>,
        select_count: bool,
    }

    struct MultiSelectorWidget {
        builds: Rc<Cell<usize>>,
        handle: Rc<RefCell<Option<ProviderHandle<Counter>>>>,
    }

    struct MultiSelectorState {
        builds: Rc<Cell<usize>>,
        handle: Rc<RefCell<Option<ProviderHandle<Counter>>>>,
    }

    impl StatefulWidget for WatchingWidget {
        type State = WatchingState;

        fn create_state(&self) -> Self::State {
            WatchingState {
                builds: self
                    .builds
                    .clone(),
                handle: self
                    .handle
                    .clone(),
                select_count: self.select_count,
            }
        }
    }

    impl State<WatchingWidget> for WatchingState {
        fn init_state(&mut self, _updater: StateUpdater<Self>) {}

        fn build(&self, context: &BuildContext) -> impl Widget {
            self.builds
                .set(
                    self.builds
                        .get()
                        + 1,
                );
            if self.select_count {
                ProviderContext::select::<Counter, usize>(context, |counter| counter.count);
            } else {
                ProviderContext::watch::<Counter>(context);
            }
            *self
                .handle
                .borrow_mut() = Some(ProviderHandle::of(context));
            LeafWidget
        }
    }

    impl Widget for WatchingWidget {
        fn to_element(&self, context: &BuildContext) -> Box<dyn Element> {
            StatefulElement::new_with_name(self, context, "WatchingWidget", None)
                .0
                .boxed()
        }
    }

    impl StatefulWidget for MultiSelectorWidget {
        type State = MultiSelectorState;

        fn create_state(&self) -> Self::State {
            MultiSelectorState {
                builds: self
                    .builds
                    .clone(),
                handle: self
                    .handle
                    .clone(),
            }
        }
    }

    impl State<MultiSelectorWidget> for MultiSelectorState {
        fn init_state(&mut self, _updater: StateUpdater<Self>) {}

        fn build(&self, context: &BuildContext) -> impl Widget {
            self.builds
                .set(
                    self.builds
                        .get()
                        + 1,
                );
            ProviderContext::select::<Counter, usize>(context, |counter| counter.count);
            ProviderContext::select::<Counter, &'static str>(context, |counter| counter.label);
            *self
                .handle
                .borrow_mut() = Some(ProviderHandle::of(context));
            LeafWidget
        }
    }

    impl Widget for MultiSelectorWidget {
        fn to_element(&self, context: &BuildContext) -> Box<dyn Element> {
            StatefulElement::new_with_name(self, context, "MultiSelectorWidget", None)
                .0
                .boxed()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        use std::sync::OnceLock;

        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let _guard = runtime.enter();
        tokio::runtime::Handle::current()
    }

    fn context() -> BuildContext<'static> {
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        BuildContext {
            parent_size: Default::default(),
            canvas,
            scale: 1.0,
            parent_pos: Default::default(),
            cursor_pos: Default::default(),
            box_constraint: Default::default(),
            visible_rect: None,
            window: WindowHandle::headless(Default::default(), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: dummy_async_handle(),
            inherited_states: Rc::new(RwLock::new(HashMap::<TypeId, Rc<dyn Any>>::new())),
        }
    }

    #[test]
    fn update_mutates_the_value_and_notifies_a_watcher() {
        let handle = ProviderHandle::new(Counter::default());
        let dirty = Rc::new(Cell::new(false));
        let consumer = BuildConsumer::new(dirty.clone());
        let window = WindowHandle::headless(Default::default(), 1.0);
        handle.subscribe_watch(&consumer, &window);

        handle.update(|counter| counter.count += 1);

        assert_eq!(
            handle
                .read()
                .count,
            1
        );
        assert!(dirty.get());
        assert!(window.take_redraw_request());
    }

    #[test]
    fn read_returns_a_snapshot_without_requiring_the_value_to_be_clone() {
        let context = context();
        let handle = ProviderHandle::new(NonClone { value: 7 });

        context.with_state(Provided(handle), |context| {
            let snapshot: Snapshot<NonClone> = ProviderContext::read(context);

            assert_eq!(snapshot.value, 7);
        });
    }

    #[test]
    fn watch_returns_a_snapshot_without_requiring_the_value_to_be_clone() {
        let context = context();
        let handle = ProviderHandle::new(NonClone { value: 7 });
        let consumer = BuildConsumer::new(Rc::new(Cell::new(false)));

        context.with_state(Provided(handle), |context| {
            context.with_build_consumer(consumer, |context| {
                let snapshot: Snapshot<NonClone> = ProviderContext::watch(context);

                assert_eq!(snapshot.value, 7);
            });
        });
    }

    #[test]
    fn snapshot_keeps_the_version_that_was_read_before_an_update() {
        let handle = ProviderHandle::new(Counter::default());
        let before = handle.read();

        handle.update(|counter| counter.count = 1);
        let after = handle.read();

        assert_eq!(before.count, 0);
        assert_eq!(after.count, 1);
    }

    #[test]
    fn repeated_watch_in_one_build_is_deduplicated() {
        let handle = ProviderHandle::new(Counter::default());
        let consumer = BuildConsumer::new(Rc::new(Cell::new(false)));
        let window = WindowHandle::headless(Default::default(), 1.0);

        handle.subscribe_watch(&consumer, &window);
        handle.subscribe_watch(&consumer, &window);

        assert_eq!(handle.subscriber_count(), 1);
    }

    #[test]
    fn selector_only_notifies_when_its_projection_changes() {
        let handle = ProviderHandle::new(Counter::default());
        let dirty = Rc::new(Cell::new(false));
        let consumer = BuildConsumer::new(dirty.clone());
        let window = WindowHandle::headless(Default::default(), 1.0);
        handle.subscribe_selector(&consumer, &window, |counter| counter.count);

        handle.update(|counter| counter.label = "changed");
        assert!(!dirty.get());

        handle.update(|counter| counter.count = 1);
        assert!(dirty.get());
    }

    #[test]
    fn reducer_dispatch_uses_the_same_notification_path() {
        #[derive(Clone, Copy)]
        enum Action {
            Increment,
        }

        let handle = ProviderHandle::new(Counter::default());
        let dirty = Rc::new(Cell::new(false));
        let consumer = BuildConsumer::new(dirty.clone());
        let window = WindowHandle::headless(Default::default(), 1.0);
        handle.subscribe_watch(&consumer, &window);

        handle.dispatch(Action::Increment, |counter, action| match action {
            Action::Increment => counter.count += 1,
        });

        assert_eq!(
            handle
                .read()
                .count,
            1
        );
        assert!(dirty.get());
    }

    #[test]
    fn lookup_uses_the_nearest_scope_and_restores_the_outer_provider() {
        let context = context();
        let outer = ProviderHandle::new(Counter { count: 1, label: "outer" });
        let inner = ProviderHandle::new(Counter { count: 2, label: "inner" });

        context.with_state(Provided(outer), |context| {
            assert_eq!(ProviderContext::read::<Counter>(context).label, "outer");
            context.with_state(Provided(inner), |context| {
                assert_eq!(ProviderContext::read::<Counter>(context).label, "inner");
            });
            assert_eq!(ProviderContext::read::<Counter>(context).label, "outer");
        });
        assert!(ProviderContext::try_read::<Counter>(&context).is_none());
    }

    #[test]
    #[should_panic(expected = "No provider for")]
    fn required_lookup_has_a_clear_missing_provider_diagnostic() {
        ProviderContext::read::<Counter>(&context());
    }

    #[test]
    fn read_does_not_subscribe_and_conditional_watch_is_cleaned_up() {
        let context = context();
        let handle = ProviderHandle::new(Counter::default());
        let consumer = BuildConsumer::new(Rc::new(Cell::new(false)));

        context.with_state(Provided(handle.clone()), |context| {
            context.with_build_consumer(consumer.clone(), |context| {
                ProviderContext::read::<Counter>(context);
                ProviderContext::watch::<Counter>(context);
            });
        });
        assert_eq!(handle.subscriber_count(), 1);

        context.with_build_consumer(consumer, |_| {});
        assert_eq!(handle.subscriber_count(), 0);
    }

    #[test]
    fn context_dispatch_resolves_the_scoped_store_by_action_type() {
        #[derive(Clone, Copy)]
        struct Increment;

        let context = context();
        let handle = ProviderHandle::new(Counter::default());
        let dispatch_handle = handle.clone();
        let dispatcher = StoreDispatcher(Rc::new(move |Increment| {
            dispatch_handle.update(|counter| counter.count += 1);
        }));

        context.with_state(dispatcher, |context| {
            ProviderContext::dispatch(context, Increment);
        });

        assert_eq!(
            handle
                .read()
                .count,
            1
        );
    }

    #[test]
    fn provider_widget_publishes_to_a_non_clone_child() {
        let observed = Rc::new(Cell::new(0));
        let provider = Provider::<Counter>::new()
            .create(|| Counter { count: 7, label: "provided" })
            .child(ReadingWidget { observed: observed.clone() });

        let _element = provider.to_element(&context());

        assert_eq!(observed.get(), 7);
    }

    #[test]
    fn provider_owned_value_is_dropped_with_its_element() {
        struct Droppable(Rc<Cell<usize>>);
        impl Drop for Droppable {
            fn drop(&mut self) {
                self.0
                    .set(
                        self.0
                            .get()
                            + 1,
                    );
            }
        }
        struct Child;
        impl Widget for Child {
            fn to_element(&self, _context: &BuildContext) -> Box<dyn Element> {
                Box::new(Leaf)
            }
        }

        let drops = Rc::new(Cell::new(0));
        let provider = Provider::<Droppable>::new()
            .create({
                let drops = drops.clone();
                move || Droppable(drops.clone())
            })
            .child(Child);
        let element = provider.to_element(&context());
        assert_eq!(drops.get(), 0);

        drop(element);

        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn provider_handle_can_be_captured_for_later_updates() {
        let context = context();
        let handle = ProviderHandle::new(Counter::default());

        let captured = context.with_state(Provided(handle), ProviderHandle::<Counter>::of);
        captured.update(|counter| counter.count = 9);

        assert_eq!(
            captured
                .read()
                .count,
            9
        );
    }

    #[test]
    fn watched_descendant_rebuilds_through_the_provider_scope() {
        let builds = Rc::new(Cell::new(0));
        let handle = Rc::new(RefCell::new(None));
        let provider = Provider::<Counter>::new()
            .create(Counter::default)
            .child(WatchingWidget {
                builds: builds.clone(),
                handle: handle.clone(),
                select_count: false,
            });
        let context = context();
        let element = provider.to_element(&context);
        assert_eq!(builds.get(), 1);

        handle
            .borrow()
            .as_ref()
            .unwrap()
            .update(|counter| counter.count += 1);
        element.rebuild_if_dirty(&context);

        assert_eq!(builds.get(), 2);
    }

    #[test]
    fn root_reconciliation_preserves_the_provider_scope_for_descendant_rebuilds() {
        let builds = Rc::new(Cell::new(0));
        let handle = Rc::new(RefCell::new(None));
        let provider = Provider::<Counter>::new()
            .create(Counter::default)
            .child(WatchingWidget { builds: builds.clone(), handle, select_count: false });
        let context = context();
        let element = provider.to_element(&context);
        let root = element
            .option_any()
            .and_then(|element| element.downcast_ref::<StatefulElement>())
            .expect("Provider should create a stateful root element");
        assert!(!root.is_dirty());

        element.mark_needs_rebuild();
        let carries_state = Cell::new(false);
        root.visit_children(&mut |child| carries_state.set(child.is_carry_state()));
        assert!(carries_state.get());
        assert!(!root.is_dirty());
        element.rebuild_if_dirty(&context);

        assert_eq!(builds.get(), 2);
    }

    #[test]
    fn selected_descendant_ignores_unrelated_updates() {
        let builds = Rc::new(Cell::new(0));
        let handle = Rc::new(RefCell::new(None));
        let provider = Provider::<Counter>::new()
            .create(Counter::default)
            .child(WatchingWidget {
                builds: builds.clone(),
                handle: handle.clone(),
                select_count: true,
            });
        let context = context();
        let element = provider.to_element(&context);

        handle
            .borrow()
            .as_ref()
            .unwrap()
            .update(|counter| counter.label = "unrelated");
        element.rebuild_if_dirty(&context);
        assert_eq!(builds.get(), 1);

        handle
            .borrow()
            .as_ref()
            .unwrap()
            .update(|counter| counter.count = 1);
        element.rebuild_if_dirty(&context);
        assert_eq!(builds.get(), 2);
    }

    #[test]
    fn multiple_selectors_from_one_consumer_track_independent_values() {
        let builds = Rc::new(Cell::new(0));
        let handle = Rc::new(RefCell::new(None));
        let provider = Provider::<Counter>::new()
            .create(Counter::default)
            .child(MultiSelectorWidget { builds: builds.clone(), handle: handle.clone() });
        let context = context();
        let element = provider.to_element(&context);

        handle
            .borrow()
            .as_ref()
            .unwrap()
            .update(|counter| counter.label = "changed");
        element.rebuild_if_dirty(&context);

        assert_eq!(builds.get(), 2);
    }
}
