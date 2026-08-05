pub mod callback_inner;

use std::any::type_name;
use std::fmt::Debug;
use std::pin::Pin;
use std::rc::Rc;

pub use callback_inner::*;

/// Where an asynchronous callback is spawned.
///
/// Native builds carry the application's Tokio handle, because a widget deep in
/// the tree has no way of finding it otherwise. The browser has a single task
/// queue and needs nothing carried, so the type collapses to a unit and call
/// sites stop needing `#[cfg]` around a parameter.
///
/// See [`CallbackExecutor::execute`] for what happens when the handle is absent.
#[cfg(not(target_arch = "wasm32"))]
pub type AsyncSpawner = Option<tokio::runtime::Handle>;

/// Where an asynchronous callback is spawned. See the native definition.
#[cfg(target_arch = "wasm32")]
pub type AsyncSpawner = ();

/// A spawner naming no particular runtime, leaving
/// [`CallbackExecutor::execute`] to fall back to whichever one the caller is
/// already running inside.
///
/// This is what an element that was never handed a handle passes — a link in a
/// paragraph, a node in an SVG. It is not an error: the ambient runtime is
/// almost always the one the handle would have named anyway.
///
/// # Examples
///
/// ```
/// use aimer_utils::callback::{Callback, CallbackExecutor, ambient_spawner};
///
/// let callback = Callback::from(|n: i32| n + 1);
/// assert_eq!(callback.execute(1, &ambient_spawner()), Some(2));
/// ```
#[cfg(not(target_arch = "wasm32"))]
#[inline]
pub const fn ambient_spawner() -> AsyncSpawner {
    None
}

/// A spawner naming no particular runtime. See the native definition.
#[cfg(target_arch = "wasm32")]
#[inline]
pub const fn ambient_spawner() -> AsyncSpawner {}

/// One invocation of an [`RawInnerCallback::Async`] body, ready to be spawned.
type SpawnedFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Hands `future` to an executor, and says so out loud when there is none.
///
/// One policy for the whole framework, which is the point: this used to be
/// hand-written at every call site, and the copies disagreed — some reached for
/// the ambient runtime, one silently did nothing at all.
#[cfg(not(target_arch = "wasm32"))]
fn spawn(future: SpawnedFuture, spawner: &AsyncSpawner) {
    if let Some(handle) = spawner {
        handle.spawn(future);
        return;
    }

    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(future);
        }
        Err(_) => crate::log::warn(
            "an async callback was discarded: no runtime handle was given and none is running",
        ),
    }
}

/// Hands `future` to the browser's task queue. See the native definition.
#[cfg(target_arch = "wasm32")]
fn spawn(future: SpawnedFuture, _spawner: &AsyncSpawner) {
    wasm_bindgen_futures::spawn_local(future);
}

/// The contract for invoking a callback, however it was registered.
///
/// Implementors expose their body through [`Self::raw`]; [`Self::execute`] then
/// runs it, and is the only place in the framework that decides what an
/// asynchronous callback means. Implementing this trait is how a type that is
/// *not* a [`Callback`] — a newtype, a field wrapper — becomes invocable
/// through the same generic code.
///
/// # Associated Types
/// - `Args`: the argument the body accepts.
/// - `Output`: the value a synchronous body returns.
///
/// # Examples
///
/// ```
/// use std::rc::Rc;
///
/// use aimer_utils::callback::{CallbackExecutor, RawInnerCallback, ambient_spawner};
///
/// struct Greeter {
///     callback: Option<RawInnerCallback<i32, String>>,
/// }
///
/// impl CallbackExecutor for Greeter {
///     type Args = i32;
///     type Output = String;
///
///     fn raw(&self) -> Option<&RawInnerCallback<Self::Args, Self::Output>> {
///         self.callback.as_ref()
///     }
/// }
///
/// let greeter = Greeter {
///     callback: Some(RawInnerCallback::Sync(Rc::new(|n| format!("{n} times")))),
/// };
/// assert_eq!(
///     greeter.execute(3, &ambient_spawner()),
///     Some("3 times".to_owned())
/// );
/// ```
pub trait CallbackExecutor {
    type Args;
    type Output;

    /// The registered body, or `None` when nothing was registered.
    fn raw(&self) -> Option<&RawInnerCallback<Self::Args, Self::Output>>;

    /// Invokes the callback, returning the value a synchronous body produced.
    ///
    /// Returns `None` when nothing was registered, and when the body is
    /// asynchronous — that value belongs to the executor, not to the caller.
    ///
    /// An asynchronous body is spawned on `spawner`'s handle; failing that, on
    /// whichever runtime the caller is already inside — see
    /// [`ambient_spawner`]. Only when there is no runtime at all is the work
    /// discarded, and that is logged rather than passed over in silence.
    fn execute(&self, args: Self::Args, spawner: &AsyncSpawner) -> Option<Self::Output> {
        match self.raw()? {
            RawInnerCallback::Sync(body) => Some(body(args)),
            RawInnerCallback::Async(body) => {
                spawn(body(args), spawner);
                None
            }
        }
    }
}

/// A callback taking `Args` and returning `Return`, which may or may not have
/// been registered.
///
/// Unregistered is the overwhelmingly common case — a widget with eleven
/// possible handlers typically sets one — so "empty" is the `None` of an
/// [`Option`], costing a write and nothing else. Cloning a registered callback
/// is one refcount bump, which matters because every widget rebuild clones
/// every callback it carries.
///
/// # Type Parameters
/// - `Args`: the argument the callback accepts. Defaults to `()`.
/// - `Return`: the value it returns. Defaults to `()`.
///
/// # Examples
///
/// ```
/// use aimer_utils::callback::Callback;
///
/// let increment = Callback::from(|x: i32| x + 1);
/// assert_eq!(increment.call(1), Some(2));
///
/// // A callback nobody registered simply reports that there was nothing to run.
/// assert_eq!(Callback::<i32, i32>::default().call(1), None);
/// ```
pub struct Callback<Args = (), Return = ()> {
    inner: Option<RawInnerCallback<Args, Return>>,
}

impl<Args, Return> Callback<Args, Return> {
    /// Invokes a **synchronous** callback with `args`, returning its result.
    ///
    /// Returns `None` when nothing was registered, and when the body is
    /// asynchronous — there is no executor to hand it to here. Use
    /// [`CallbackExecutor::execute`] where an asynchronous body must run.
    #[inline]
    pub fn call(&self, args: Args) -> Option<Return> {
        match self.inner.as_ref()? {
            RawInnerCallback::Sync(body) => Some(body(args)),
            RawInnerCallback::Async(_) => None,
        }
    }
}

impl<Args, Return> Default for Callback<Args, Return> {
    #[inline]
    fn default() -> Self {
        Self { inner: None }
    }
}

impl<Args, Return> Clone for Callback<Args, Return> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<Args, Return> Debug for Callback<Args, Return> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let args_type = type_name::<Args>();
        let return_type = type_name::<Return>();
        write!(f, "Callback({args_type} -> {return_type})",)
    }
}

impl<Args, Return, F: Fn(Args) -> Return + 'static> From<F> for Callback<Args, Return> {
    #[inline]
    fn from(f: F) -> Self {
        Self {
            inner: Some(RawInnerCallback::Sync(Rc::new(f))),
        }
    }
}

impl<Args, Return, F, Fut> From<AsyncCallback<F>> for Callback<Args, Return>
where
    F: FnOnce(Args) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    #[inline]
    fn from(callback: AsyncCallback<F>) -> Self {
        Self {
            inner: Some(RawInnerCallback::from(callback)),
        }
    }
}

impl<P, R> CallbackExecutor for Callback<P, R> {
    type Args = P;
    type Output = R;

    #[inline]
    fn raw(&self) -> Option<&RawInnerCallback<Self::Args, Self::Output>> {
        self.inner.as_ref()
    }
}

pub type VoidParamedFunction<R> = Callback<R, ()>;

/// A callback taking no argument and returning nothing.
///
/// A distinct type rather than an alias for `Callback<(), ()>` because the
/// blanket `From<F: Fn(Args) -> Return>` impl would demand `Fn(())`, and a
/// plain `|| ...` does not coerce to that. Everything else is delegated, so the
/// two share one representation and one dispatch.
///
/// # Examples
///
/// ```
/// use std::cell::Cell;
/// use std::rc::Rc;
///
/// use aimer_utils::callback::{CallbackExecutor, VoidCallback, ambient_spawner};
///
/// let presses = Rc::new(Cell::new(0));
/// let counted = presses.clone();
/// let on_press = VoidCallback::from(move || counted.set(counted.get() + 1));
///
/// on_press.execute((), &ambient_spawner());
/// assert_eq!(presses.get(), 1);
/// ```
#[derive(Default, Clone)]
pub struct VoidCallback(Callback<(), ()>);

impl VoidCallback {
    /// Registers an `async` closure, to be driven by an executor.
    ///
    /// Unlike the `From<F>` impl — which takes an `Fn()` — this accepts an
    /// [`FnOnce`], so the closure may consume what it captured. It is taken on
    /// its first invocation and every later one does nothing; a handler that
    /// must react to *every* press has to clone its state into the future
    /// rather than move it.
    ///
    /// # Examples
    ///
    /// ```
    /// use aimer_utils::callback::VoidCallback;
    ///
    /// let payload = vec![1, 2, 3];
    /// let callback = VoidCallback::from_async(move || async move {
    ///     let _ = payload.len();
    /// });
    /// ```
    #[inline]
    pub fn from_async<F, Fut>(f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self(Callback::from(AsyncCallback(move |()| f())))
    }
}

impl Debug for VoidCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VoidCallback()")
    }
}

impl<F: Fn() + 'static> From<F> for VoidCallback {
    #[inline]
    fn from(f: F) -> Self {
        Self(Callback::from(move |()| f()))
    }
}

impl CallbackExecutor for VoidCallback {
    type Args = ();
    type Output = ();

    #[inline]
    fn raw(&self) -> Option<&RawInnerCallback<Self::Args, Self::Output>> {
        self.0.raw()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Drives a current-thread runtime long enough for whatever was spawned on
    /// it to reach its first await point and finish.
    fn settle(runtime: &tokio::runtime::Runtime) {
        runtime.block_on(async {
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
        });
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a current-thread runtime")
    }

    #[test]
    fn an_unset_callback_reports_nothing_to_call() {
        let callback = Callback::<i32, i32>::default();

        assert_eq!(callback.call(7), None);
        assert_eq!(callback.execute(7, &ambient_spawner()), None);
    }

    #[test]
    fn a_sync_callback_returns_its_value_through_both_entry_points() {
        let callback = Callback::from(|value: i32| value + 1);

        assert_eq!(callback.call(1), Some(2));
        assert_eq!(callback.execute(1, &ambient_spawner()), Some(2));
    }

    #[test]
    fn clones_share_the_one_closure() {
        let calls = Rc::new(Cell::new(0));
        let counted = calls.clone();
        let callback = Callback::from(move |_: ()| counted.set(counted.get() + 1));

        callback.clone().call(());
        callback.call(());

        assert_eq!(calls.get(), 2);
    }

    // `call` is the synchronous entry point: it cannot spawn, so rather than
    // running an async body on the calling thread it reports that there was
    // nothing to run synchronously.
    #[test]
    fn call_refuses_an_async_callback_rather_than_running_it() {
        let ran = Arc::new(AtomicUsize::new(0));
        let flag = ran.clone();
        let callback: Callback<(), ()> = AsyncCallback(move |_| {
            let flag = flag.clone();
            async move {
                flag.fetch_add(1, Ordering::SeqCst);
            }
        })
        .into();

        assert_eq!(callback.call(()), None);
        assert_eq!(ran.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_async_callback_runs_on_the_handle_it_was_given() {
        let runtime = runtime();
        let ran = Arc::new(AtomicUsize::new(0));
        let flag = ran.clone();
        let callback: Callback<(), ()> = AsyncCallback(move |_| {
            let flag = flag.clone();
            async move {
                flag.fetch_add(1, Ordering::SeqCst);
            }
        })
        .into();

        assert_eq!(callback.execute((), &Some(runtime.handle().clone())), None);
        settle(&runtime);

        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    // The fallback `aimer_svg` and `aimer_text` used to hand-roll: an element
    // holding no handle of its own still reaches the runtime it is running
    // inside. Losing this would have made every SVG and link callback a no-op.
    #[test]
    fn an_async_callback_falls_back_to_the_ambient_runtime() {
        let runtime = runtime();
        let ran = Arc::new(AtomicUsize::new(0));
        let flag = ran.clone();
        let callback = VoidCallback::from_async(move || {
            let flag = flag.clone();
            async move {
                flag.fetch_add(1, Ordering::SeqCst);
            }
        });

        runtime.block_on(async {
            callback.execute((), &ambient_spawner());
        });
        settle(&runtime);

        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    // A one-shot body is taken on its first invocation. `Button` documents that
    // a second press does nothing, so a second invocation must not panic — the
    // two async constructors disagreed about this before they shared a body.
    #[test]
    fn a_second_invocation_of_a_one_shot_async_callback_does_nothing() {
        let runtime = runtime();
        let ran = Arc::new(AtomicUsize::new(0));
        let flag = ran.clone();
        let callback = VoidCallback::from_async(move || {
            let flag = flag.clone();
            async move {
                flag.fetch_add(1, Ordering::SeqCst);
            }
        });

        let handle = Some(runtime.handle().clone());
        callback.execute((), &handle);
        callback.execute((), &handle);
        settle(&runtime);

        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_async_callback_without_any_runtime_is_dropped_rather_than_panicking() {
        let callback = VoidCallback::from_async(|| async {});

        assert_eq!(callback.execute((), &ambient_spawner()), None);
    }

    #[test]
    fn a_void_callback_delegates_to_the_shared_representation() {
        let calls = Rc::new(Cell::new(0));
        let counted = calls.clone();
        let callback = VoidCallback::from(move || counted.set(counted.get() + 1));

        callback.execute((), &ambient_spawner());
        callback.clone().execute((), &ambient_spawner());
        VoidCallback::default().execute((), &ambient_spawner());

        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn debug_names_the_signature() {
        assert_eq!(
            format!("{:?}", Callback::<i32, bool>::default()),
            "Callback(i32 -> bool)"
        );
        assert_eq!(format!("{:?}", VoidCallback::default()), "VoidCallback()");
    }
}
