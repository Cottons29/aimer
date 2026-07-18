use std::cell::{Cell, RefCell, UnsafeCell};
use std::future::Future;
use std::marker::PhantomData;
use std::rc::Rc;

use aimer_attribute::{ResolvedSize, Size, Vec2d};
use crossbeam_channel::{Receiver, Sender, unbounded};
use futures_util::future::{AbortHandle, Abortable};

use crate::base::BuildContext;
use crate::widget::stateful::{SyncChild, carry_child_state};
use crate::{
    AnyWidget, Drawable, Element, EventElement, Key, LayoutElement, Rebuildable, RequiredChild,
    State, StateUpdater, StatefulElement, StatefulWidget, VisitorElement, Widget,
};

/// The current state of an [`AsyncBuilder`] operation.
pub enum AsyncSnapshot<T, E> {
    Waiting,
    Data(T),
    Error(E),
}

#[doc(hidden)]
pub struct FutureFactory<F, T, E> {
    factory: Rc<F>,
    marker: PhantomData<fn() -> Result<T, E>>,
}

#[doc(hidden)]
pub struct SnapshotBuilder<B, T, E> {
    builder: Rc<B>,
    marker: PhantomData<fn(&AsyncSnapshot<T, E>)>,
}

/// Builds one subtree from the eventual result of an asynchronous operation.
///
/// The future starts after the widget is mounted. Rebuilding a parent with the
/// same request key keeps the current operation; changing the request key
/// cancels it and starts a new one.
///
/// ```ignore
/// AsyncBuilder::new()
///     .request_key(user_id)
///     .future(move || async move { load_user(user_id).await })
///     .child(|snapshot| match snapshot {
///         AsyncSnapshot::Waiting => Text::new("Loading...").boxed(),
///         AsyncSnapshot::Data(user) => Text::new(&user.name).boxed(),
///         AsyncSnapshot::Error(error) => Text::new(error.to_string()).boxed(),
///     })
/// ```
pub struct AsyncBuilder<K = (), F = RequiredChild, B = RequiredChild> {
    request_key: K,
    future_factory: F,
    snapshot_builder: B,
    widget_key: Option<Key>,
}

impl AsyncBuilder {
    pub fn new() -> Self {
        Self {
            request_key: (),
            future_factory: RequiredChild,
            snapshot_builder: RequiredChild,
            widget_key: None,
        }
    }
}

impl Default for AsyncBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, F, B> AsyncBuilder<K, F, B> {
    pub fn request_key<NK>(self, request_key: NK) -> AsyncBuilder<NK, F, B> {
        AsyncBuilder {
            request_key,
            future_factory: self.future_factory,
            snapshot_builder: self.snapshot_builder,
            widget_key: self.widget_key,
        }
    }

    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.widget_key = Some(key.into());
        self
    }
}

impl<K, B> AsyncBuilder<K, RequiredChild, B> {
    pub fn future<F, Fut, T, E>(self, factory: F) -> AsyncBuilder<K, FutureFactory<F, T, E>, B>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        AsyncBuilder {
            request_key: self.request_key,
            future_factory: FutureFactory { factory: Rc::new(factory), marker: PhantomData },
            snapshot_builder: self.snapshot_builder,
            widget_key: self.widget_key,
        }
    }
}

impl<K, F, T, E> AsyncBuilder<K, FutureFactory<F, T, E>, RequiredChild> {
    pub fn child<B>(
        self,
        builder: B,
    ) -> AsyncBuilder<K, FutureFactory<F, T, E>, SnapshotBuilder<B, T, E>>
    where
        B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
    {
        AsyncBuilder {
            request_key: self.request_key,
            future_factory: self.future_factory,
            snapshot_builder: SnapshotBuilder { builder: Rc::new(builder), marker: PhantomData },
            widget_key: self.widget_key,
        }
    }
}

struct Completion<T, E> {
    generation: u64,
    result: Result<T, E>,
}

struct AsyncRuntimeInner<T, E> {
    snapshot: AsyncSnapshot<T, E>,
    generation: u64,
    revision: u64,
    started: bool,
    abort_handle: Option<AbortHandle>,
}

struct AsyncRuntime<T, E> {
    inner: RefCell<AsyncRuntimeInner<T, E>>,
    sender: Sender<Completion<T, E>>,
    receiver: Receiver<Completion<T, E>>,
}

impl<T, E> AsyncRuntime<T, E> {
    fn new() -> Self {
        let (sender, receiver) = unbounded();
        Self {
            inner: RefCell::new(AsyncRuntimeInner {
                snapshot: AsyncSnapshot::Waiting,
                generation: 0,
                revision: 0,
                started: false,
                abort_handle: None,
            }),
            sender,
            receiver,
        }
    }

    fn revision(&self) -> u64 {
        self.inner
            .borrow()
            .revision
    }

    fn reset(&self) {
        let mut inner = self
            .inner
            .borrow_mut();
        if let Some(handle) = inner
            .abort_handle
            .take()
        {
            handle.abort();
        }
        inner.generation = inner
            .generation
            .wrapping_add(1);
        inner.revision = inner
            .revision
            .wrapping_add(1);
        inner.snapshot = AsyncSnapshot::Waiting;
        inner.started = false;
    }

    fn begin(&self) -> Option<(u64, futures_util::future::AbortRegistration)> {
        let mut inner = self
            .inner
            .borrow_mut();
        if inner.started {
            return None;
        }
        inner.started = true;
        let generation = inner.generation;
        let (abort_handle, registration) = AbortHandle::new_pair();
        inner.abort_handle = Some(abort_handle);
        Some((generation, registration))
    }

    fn poll_completion(&self) {
        while let Ok(completion) = self
            .receiver
            .try_recv()
        {
            let mut inner = self
                .inner
                .borrow_mut();
            if completion.generation != inner.generation {
                continue;
            }
            inner.snapshot = match completion.result {
                Ok(data) => AsyncSnapshot::Data(data),
                Err(error) => AsyncSnapshot::Error(error),
            };
            inner.revision = inner
                .revision
                .wrapping_add(1);
            inner.abort_handle = None;
        }
    }
}

impl<T, E> Drop for AsyncRuntime<T, E> {
    fn drop(&mut self) {
        if let Some(handle) = self
            .inner
            .get_mut()
            .abort_handle
            .take()
        {
            handle.abort();
        }
    }
}

#[doc(hidden)]
pub struct AsyncBuilderState<K, F, B, T, E> {
    request_key: K,
    future_factory: Rc<F>,
    snapshot_builder: Rc<B>,
    runtime: Rc<AsyncRuntime<T, E>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<K, F, Fut, B, T, E> StatefulWidget
    for AsyncBuilder<K, FutureFactory<F, T, E>, SnapshotBuilder<B, T, E>>
where
    K: Clone + Eq + 'static,
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget + 'static,
    T: Send + 'static,
    E: Send + 'static,
{
    type State = AsyncBuilderState<K, F, B, T, E>;

    fn create_state(&self) -> Self::State {
        AsyncBuilderState {
            request_key: self
                .request_key
                .clone(),
            future_factory: self
                .future_factory
                .factory
                .clone(),
            snapshot_builder: self
                .snapshot_builder
                .builder
                .clone(),
            runtime: Rc::new(AsyncRuntime::new()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<K, F, Fut, B, T, E> StatefulWidget
    for AsyncBuilder<K, FutureFactory<F, T, E>, SnapshotBuilder<B, T, E>>
where
    K: Clone + Eq + 'static,
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget + 'static,
    T: 'static,
    E: 'static,
{
    type State = AsyncBuilderState<K, F, B, T, E>;

    fn create_state(&self) -> Self::State {
        AsyncBuilderState {
            request_key: self
                .request_key
                .clone(),
            future_factory: self
                .future_factory
                .factory
                .clone(),
            snapshot_builder: self
                .snapshot_builder
                .builder
                .clone(),
            runtime: Rc::new(AsyncRuntime::new()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<K, F, Fut, B, T, E> State<AsyncBuilder<K, FutureFactory<F, T, E>, SnapshotBuilder<B, T, E>>>
    for AsyncBuilderState<K, F, B, T, E>
where
    K: Clone + Eq + 'static,
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget + 'static,
    T: Send + 'static,
    E: Send + 'static,
{
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: &Self) {
        self.future_factory = new
            .future_factory
            .clone();
        self.snapshot_builder = new
            .snapshot_builder
            .clone();
        if self.request_key != new.request_key {
            self.request_key = new
                .request_key
                .clone();
            self.runtime
                .reset();
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AsyncFrame {
            future_factory: self
                .future_factory
                .clone(),
            snapshot_builder: self
                .snapshot_builder
                .clone(),
            runtime: self
                .runtime
                .clone(),
            marker: PhantomData::<fn() -> Fut>,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<K, F, Fut, B, T, E> State<AsyncBuilder<K, FutureFactory<F, T, E>, SnapshotBuilder<B, T, E>>>
    for AsyncBuilderState<K, F, B, T, E>
where
    K: Clone + Eq + 'static,
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget + 'static,
    T: 'static,
    E: 'static,
{
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: &Self) {
        self.future_factory = new
            .future_factory
            .clone();
        self.snapshot_builder = new
            .snapshot_builder
            .clone();
        if self.request_key != new.request_key {
            self.request_key = new
                .request_key
                .clone();
            self.runtime
                .reset();
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AsyncFrame {
            future_factory: self
                .future_factory
                .clone(),
            snapshot_builder: self
                .snapshot_builder
                .clone(),
            runtime: self
                .runtime
                .clone(),
            marker: PhantomData::<fn() -> Fut>,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<K, F, Fut, B, T, E> Widget
    for AsyncBuilder<K, FutureFactory<F, T, E>, SnapshotBuilder<B, T, E>>
where
    K: Clone + Eq + 'static,
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget + 'static,
    T: Send + 'static,
    E: Send + 'static,
{
    fn key(&self) -> Option<Key> {
        self.widget_key
            .clone()
    }

    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new_with_name(self, ctx, "AsyncBuilder", self.key())
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "AsyncBuilder"
    }
}

#[cfg(target_arch = "wasm32")]
impl<K, F, Fut, B, T, E> Widget
    for AsyncBuilder<K, FutureFactory<F, T, E>, SnapshotBuilder<B, T, E>>
where
    K: Clone + Eq + 'static,
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget + 'static,
    T: 'static,
    E: 'static,
{
    fn key(&self) -> Option<Key> {
        self.widget_key
            .clone()
    }

    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new_with_name(self, ctx, "AsyncBuilder", self.key())
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "AsyncBuilder"
    }
}

struct AsyncFrame<F, Fut, B, T, E> {
    future_factory: Rc<F>,
    snapshot_builder: Rc<B>,
    runtime: Rc<AsyncRuntime<T, E>>,
    marker: PhantomData<fn() -> Fut>,
}

impl<F, Fut, B, T, E> AsyncFrame<F, Fut, B, T, E>
where
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
{
    fn child_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let inner = self
            .runtime
            .inner
            .borrow();
        (self.snapshot_builder)(&inner.snapshot).to_element(ctx)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<F, Fut, B, T, E> Widget for AsyncFrame<F, Fut, B, T, E>
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget + 'static,
    T: Send + 'static,
    E: Send + 'static,
{
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(AsyncFrameElement {
            child: SyncChild(UnsafeCell::new(self.child_element(ctx))),
            future_factory: self
                .future_factory
                .clone(),
            snapshot_builder: self
                .snapshot_builder
                .clone(),
            runtime: self
                .runtime
                .clone(),
            rendered_revision: Cell::new(
                self.runtime
                    .revision(),
            ),
            marker: PhantomData::<fn() -> Fut>,
        })
    }

    fn debug_name(&self) -> &'static str {
        "AsyncFrame"
    }
}

#[cfg(target_arch = "wasm32")]
impl<F, Fut, B, T, E> Widget for AsyncFrame<F, Fut, B, T, E>
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<T, E>> + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget + 'static,
    T: 'static,
    E: 'static,
{
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(AsyncFrameElement {
            child: SyncChild(UnsafeCell::new(self.child_element(ctx))),
            future_factory: self
                .future_factory
                .clone(),
            snapshot_builder: self
                .snapshot_builder
                .clone(),
            runtime: self
                .runtime
                .clone(),
            rendered_revision: Cell::new(
                self.runtime
                    .revision(),
            ),
            marker: PhantomData::<fn() -> Fut>,
        })
    }

    fn debug_name(&self) -> &'static str {
        "AsyncFrame"
    }
}

struct AsyncFrameElement<F, Fut, B, T, E> {
    child: SyncChild,
    future_factory: Rc<F>,
    snapshot_builder: Rc<B>,
    runtime: Rc<AsyncRuntime<T, E>>,
    rendered_revision: Cell<u64>,
    marker: PhantomData<fn() -> Fut>,
}

impl<F, Fut, B, T, E> AsyncFrameElement<F, Fut, B, T, E> {
    fn current_child(&self) -> &dyn Element {
        // Safety: Aimer's rendering pipeline is single-threaded. Child replacement
        // happens only while processing this element on that render thread.
        unsafe {
            (&*self
                .child
                .0
                .get())
                .as_ref()
        }
    }

    fn replace_child(&self, child: Box<dyn Element>) {
        // Safety: see `current_child`; no child reference is retained across this
        // replacement.
        unsafe {
            *self
                .child
                .0
                .get() = child;
        }
    }
}

impl<F, Fut, B, T, E> AsyncFrameElement<F, Fut, B, T, E>
where
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
{
    fn update_child(&self, ctx: &BuildContext) {
        if self
            .rendered_revision
            .get()
            == self
                .runtime
                .revision()
        {
            return;
        }
        let new_child = {
            let inner = self
                .runtime
                .inner
                .borrow();
            (self.snapshot_builder)(&inner.snapshot).to_element(ctx)
        };
        carry_child_state(self.current_child(), new_child.as_ref(), ctx);
        self.replace_child(new_child);
        self.rendered_revision
            .set(
                self.runtime
                    .revision(),
            );
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<F, Fut, B, T, E> AsyncFrameElement<F, Fut, B, T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
    T: Send + 'static,
    E: Send + 'static,
{
    fn refresh(&self, ctx: &BuildContext) {
        if let Some((generation, registration)) = self
            .runtime
            .begin()
        {
            let future = (self.future_factory)();
            let sender = self
                .runtime
                .sender
                .clone();
            let window = ctx
                .window
                .clone();
            ctx.async_handle
                .spawn(async move {
                    if let Ok(result) = Abortable::new(future, registration).await {
                        let _ = sender.send(Completion { generation, result });
                        window.request_redraw();
                    }
                });
        }
        self.runtime
            .poll_completion();
        self.update_child(ctx);
    }
}

#[cfg(target_arch = "wasm32")]
impl<F, Fut, B, T, E> AsyncFrameElement<F, Fut, B, T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
    T: 'static,
    E: 'static,
{
    fn refresh(&self, ctx: &BuildContext) {
        if let Some((generation, registration)) = self
            .runtime
            .begin()
        {
            let future = (self.future_factory)();
            let sender = self
                .runtime
                .sender
                .clone();
            let window = ctx
                .window
                .clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(result) = Abortable::new(future, registration).await {
                    let _ = sender.send(Completion { generation, result });
                    window.request_redraw();
                }
            });
        }
        self.runtime
            .poll_completion();
        self.update_child(ctx);
    }
}

impl<F, Fut, B, T, E> VisitorElement for AsyncFrameElement<F, Fut, B, T, E> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.current_child());
    }

    fn debug_name(&self) -> &'static str {
        "AsyncFrame"
    }
}

impl<F, Fut, B, T, E> EventElement for AsyncFrameElement<F, Fut, B, T, E> {}

#[cfg(not(target_arch = "wasm32"))]
impl<F, Fut, B, T, E> Rebuildable for AsyncFrameElement<F, Fut, B, T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
    T: Send + 'static,
    E: Send + 'static,
{
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.refresh(ctx);
        self.current_child()
            .rebuild_if_dirty(ctx);
    }

    fn mark_needs_rebuild(&self) {
        self.current_child()
            .mark_needs_rebuild();
    }
}

#[cfg(target_arch = "wasm32")]
impl<F, Fut, B, T, E> Rebuildable for AsyncFrameElement<F, Fut, B, T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
    T: 'static,
    E: 'static,
{
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.refresh(ctx);
        self.current_child()
            .rebuild_if_dirty(ctx);
    }

    fn mark_needs_rebuild(&self) {
        self.current_child()
            .mark_needs_rebuild();
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<F, Fut, B, T, E> Drawable for AsyncFrameElement<F, Fut, B, T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
    T: Send + 'static,
    E: Send + 'static,
{
    fn draw(&self, ctx: &BuildContext) {
        self.refresh(ctx);
        self.current_child()
            .draw(ctx);
    }
}

#[cfg(target_arch = "wasm32")]
impl<F, Fut, B, T, E> Drawable for AsyncFrameElement<F, Fut, B, T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + 'static,
    B: Fn(&AsyncSnapshot<T, E>) -> AnyWidget,
    T: 'static,
    E: 'static,
{
    fn draw(&self, ctx: &BuildContext) {
        self.refresh(ctx);
        self.current_child()
            .draw(ctx);
    }
}

impl<F, Fut, B, T, E> LayoutElement for AsyncFrameElement<F, Fut, B, T, E> {
    fn pos(&self) -> Option<Vec2d> {
        self.current_child()
            .pos()
    }

    fn size(&self) -> Option<Size> {
        self.current_child()
            .size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.current_child()
            .layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.current_child()
            .computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.current_child()
            .content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.current_child()
            .layer()
    }

    fn flex(&self) -> Option<f32> {
        self.current_child()
            .flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.current_child()
            .get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.current_child()
            .invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.current_child()
            .pos_start_end()
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
fn async_builder_accepts_local_futures() {
    struct ProbeWidget;

    impl Widget for ProbeWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            unreachable!("compile-time WebAssembly API probe")
        }
    }

    let local_value = Rc::new(Cell::new(1_usize));
    let widget = AsyncBuilder::new()
        .future(move || {
            let local_value = local_value.clone();
            async move { Ok::<_, Rc<()>>(local_value) }
        })
        .child(|snapshot| match snapshot {
            AsyncSnapshot::Waiting | AsyncSnapshot::Data(_) | AsyncSnapshot::Error(_) => {
                Box::new(ProbeWidget) as AnyWidget
            }
        });

    fn require_widget(_widget: impl Widget) {}
    require_widget(widget);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use aimer_attribute::ResolvedSize;

    use crate::base::{BuildContext, WindowHandle};
    use crate::{
        AnyWidget, AsyncBuilder, AsyncSnapshot, Drawable, Element, EventElement, LayoutElement,
        Rebuildable, VisitorElement, Widget,
    };

    struct MarkerWidget(&'static str);

    impl Widget for MarkerWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            Box::new(MarkerElement(self.0))
        }

        fn debug_name(&self) -> &'static str {
            self.0
        }
    }

    struct MarkerElement(&'static str);

    impl VisitorElement for MarkerElement {
        fn debug_name(&self) -> &'static str {
            self.0
        }
    }

    impl Drawable for MarkerElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl EventElement for MarkerElement {}
    impl LayoutElement for MarkerElement {}
    impl Rebuildable for MarkerElement {}

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

    fn contains(element: &dyn Element, name: &'static str) -> bool {
        if element.debug_name() == name {
            return true;
        }
        let found = Rc::new(Cell::new(false));
        let found_in_child = found.clone();
        element.visit_children(&mut |child| {
            if contains(child, name) {
                found_in_child.set(true);
            }
        });
        found.get()
    }

    fn marker(snapshot: &AsyncSnapshot<usize, &'static str>) -> AnyWidget {
        match snapshot {
            AsyncSnapshot::Waiting => MarkerWidget("Waiting").boxed(),
            AsyncSnapshot::Data(_) => MarkerWidget("Data").boxed(),
            AsyncSnapshot::Error(_) => MarkerWidget("Error").boxed(),
        }
    }

    #[tokio::test]
    async fn launches_once_and_rebuilds_from_waiting_to_data_after_redraw() {
        let launches = Arc::new(AtomicUsize::new(0));
        let factory_launches = launches.clone();
        let widget = AsyncBuilder::new()
            .request_key(7_u64)
            .future(move || {
                factory_launches.fetch_add(1, Ordering::SeqCst);
                async { Ok::<_, &'static str>(42_usize) }
            })
            .child(marker);
        let ctx = context();
        let element = widget.to_element(&ctx);

        assert!(contains(element.as_ref(), "Waiting"));
        element.rebuild_if_dirty(&ctx);
        assert_eq!(launches.load(Ordering::SeqCst), 1);

        tokio::task::yield_now().await;
        assert!(
            ctx.window
                .take_redraw_request()
        );
        element.rebuild_if_dirty(&ctx);

        assert!(contains(element.as_ref(), "Data"));
        assert_eq!(launches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn renders_typed_errors_with_a_different_widget_type() {
        let widget = AsyncBuilder::new()
            .future(|| async { Err::<usize, _>("failed") })
            .child(marker);
        let ctx = context();
        let element = widget.to_element(&ctx);

        element.rebuild_if_dirty(&ctx);
        tokio::task::yield_now().await;
        element.rebuild_if_dirty(&ctx);

        assert!(contains(element.as_ref(), "Error"));
    }

    #[tokio::test]
    async fn unchanged_request_identity_does_not_launch_again_during_reconciliation() {
        let launches = Arc::new(AtomicUsize::new(0));
        let make_widget = || {
            let launches = launches.clone();
            AsyncBuilder::new()
                .request_key("same")
                .future(move || {
                    launches.fetch_add(1, Ordering::SeqCst);
                    async { Ok::<_, &'static str>(1_usize) }
                })
                .child(marker)
        };
        let ctx = context();
        let old = make_widget().to_element(&ctx);
        tokio::task::yield_now().await;
        old.rebuild_if_dirty(&ctx);
        let new = make_widget().to_element(&ctx);

        crate::widget::stateful::carry_child_state(old.as_ref(), new.as_ref(), &ctx);

        assert_eq!(launches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn changed_request_identity_restarts_once() {
        let launches = Arc::new(AtomicUsize::new(0));
        let make_widget = |request_key| {
            let launches = launches.clone();
            AsyncBuilder::new()
                .request_key(request_key)
                .future(move || {
                    launches.fetch_add(1, Ordering::SeqCst);
                    async { Ok::<_, &'static str>(1_usize) }
                })
                .child(marker)
        };
        let ctx = context();
        let old = make_widget(1_u64).to_element(&ctx);
        old.rebuild_if_dirty(&ctx);
        tokio::task::yield_now().await;
        old.rebuild_if_dirty(&ctx);
        let new = make_widget(2_u64).to_element(&ctx);

        crate::widget::stateful::carry_child_state(old.as_ref(), new.as_ref(), &ctx);
        assert!(contains(new.as_ref(), "Waiting"));
        new.rebuild_if_dirty(&ctx);

        assert_eq!(launches.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn stale_completion_cannot_replace_a_newer_generation() {
        let runtime = super::AsyncRuntime::<usize, &'static str>::new();
        let old_generation = runtime
            .inner
            .borrow()
            .generation;
        runtime.reset();
        runtime
            .sender
            .send(super::Completion { generation: old_generation, result: Ok(1) })
            .unwrap();

        runtime.poll_completion();

        assert!(matches!(
            runtime
                .inner
                .borrow()
                .snapshot,
            AsyncSnapshot::Waiting
        ));
    }

    #[tokio::test]
    async fn dropping_the_element_cancels_a_pending_future() {
        struct DropGuard(Arc<AtomicUsize>);
        impl Drop for DropGuard {
            fn drop(&mut self) {
                self.0
                    .fetch_add(1, Ordering::SeqCst);
            }
        }

        let drops = Arc::new(AtomicUsize::new(0));
        let future_drops = drops.clone();
        let widget = AsyncBuilder::new()
            .future(move || {
                let guard = DropGuard(future_drops.clone());
                async move {
                    let _guard = guard;
                    std::future::pending::<Result<usize, &'static str>>().await
                }
            })
            .child(marker);
        let ctx = context();
        let element = widget.to_element(&ctx);

        element.rebuild_if_dirty(&ctx);
        tokio::task::yield_now().await;
        drop(element);
        tokio::task::yield_now().await;

        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}
