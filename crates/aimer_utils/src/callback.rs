pub mod callback_inner;

pub use callback_inner::*;
use std::any::type_name;
use std::cell::UnsafeCell;
use std::fmt::Debug;
use std::rc::Rc;
///
/// A trait that defines the contract for executing a callback.
///
/// This trait is designed to be implemented by types that manage a callback function,
/// encapsulated within an `Option<RawInnerCallback>`. It provides a method to retrieve
/// the inner callback reference.
///
/// # Associated Types
/// - `Args`: The argument type(s) that the callback function will accept.
/// - `Output`: The return type of the callback function.
///
/// # Required Methods
/// - `get(&self) -> &Option<RawInnerCallback<Self::Args, Self::Output>>`:
///   Returns a reference to the optional callback function.
///
/// # Example
/// ```rust
/// use aimer_utils::callback::{CallbackExecutor, RawInnerCallback};
///
/// struct MyCallbackExecutor {
///     callback: Option<RawInnerCallback<i32, String>>,
/// }
///
/// impl CallbackExecutor for MyCallbackExecutor {
///     type Args = i32;
///     type Output = String;
///
///     fn get(&self) -> &Option<RawInnerCallback<Self::Args, Self::Output>> {
///         &self.callback
///     }
/// }
/// ```
///
/// This trait provides a flexible mechanism for working with optional callbacks
/// while allowing customization of input/output types.
///
pub trait CallbackExecutor {
    type Args;
    type Output;
    fn get(&self) -> &Option<RawInnerCallback<Self::Args, Self::Output>>;
}

///
/// Represents a generic callback mechanism encapsulating a function or closure
/// that can be invoked with specified arguments and returns a specified result.
///
/// # Type Parameters
/// - `Args`: The type of the arguments that the callback expects. Defaults to `()`.
/// - `Return`: The type of the value that the callback returns. Defaults to `()`.
///
/// # Fields
/// - `inner`: An internal representation of the callback, typically used to store
///   the function or closure that is executed when the callback is invoked.
///
/// # Example
/// ```
/// use aimer_utils::callback::Callback;
///
/// // Example of using a Callback with specific argument and return types
/// let callback = Callback::from(|x: i32| x + 1);
/// ```
///
pub struct Callback<Args = (), Return = ()> {
    inner: CallbackInner<Args, Return>,
}

impl<Args, Return> Callback<Args, Return> {
    pub fn callable(&self) -> Option<&Self> {
        if self.inner.is_default() { None } else { Some(self) }
    }

    /// Invoke a **synchronous** callback with `args`, returning its result.
    ///
    /// Returns `None` when the callback is the default/empty sentinel (never
    /// registered) or is an async callback — async callbacks must be driven by
    /// an executor via [`CallbackExecutor::get`], not this convenience method.
    pub fn call(&self, args: Args) -> Option<Return> {
        // SAFETY: mirrors the established `CallbackExecutor` access pattern —
        // the inner cell is only ever read here on the single UI thread.
        match unsafe { (*self.inner.get()).as_ref() } {
            Some(RawInnerCallback::Sync(f)) => Some(f(args)),
            _ => None,
        }
    }
}

unsafe impl<Args, Return> Send for Callback<Args, Return>
where
    Args: Send,
    Return: Send,
{
}
unsafe impl<Args, Return> Sync for Callback<Args, Return>
where
    Args: Sync,
    Return: Sync,
{
}

impl<Args, Return> Default for Callback<Args, Return> {
    fn default() -> Self {
        Self { inner: CallbackInner::default() }
    }
}

impl<Args, Return> Clone for Callback<Args, Return> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
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
    fn from(f: F) -> Self {
        Self { inner: CallbackInner::from(f) }
    }
}

impl<Args, Return, F, Fut> From<AsyncCallback<F>> for Callback<Args, Return>
where
    F: FnOnce(Args) -> Fut + Send + 'static,
    Fut: Future<Output = Return> + Send + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        let f = std::sync::Mutex::new(Some(ac.0));
        Self {
            inner: CallbackInner(Rc::new(UnsafeCell::new(Some(RawInnerCallback::Async(
                Box::new(move |args| {
                    let f = f.lock().unwrap().take();
                    if let Some(f) = f {
                        Box::pin(f(args))
                    } else {
                        Box::pin(async { panic!("AsyncCallback called more than once") })
                    }
                }),
            ))))),
        }
    }
}

impl<P, R> CallbackExecutor for Callback<P, R> {
    type Args = P;
    type Output = R;
    fn get(&self) -> &Option<RawInnerCallback<Self::Args, Self::Output>> {
        unsafe { &*self.inner.get() }
    }
}

pub type VoidParamedFunction<R> = Callback<R, ()>;

/// A struct representing a callback with no input and no output (void callback).
///
/// `VoidCallback` is a wrapper around `CallbackInner<(), ()>` that provides
/// functionality for handling callbacks that neither take any arguments nor
/// return any results.
///
/// # Derive Attributes
/// - `Default`: Allows the struct to be instantiated with default values.
/// - `Clone`: Enables the struct to be cloned.
///
/// # Fields
/// - `inner`: The inner implementation of the callback, stored
///   as a `CallbackInner<(), ()>`.
///
/// # Example
/// ```
/// use aimer_utils::callback::VoidCallback;
///
/// // Create a default instance of VoidCallback
/// let callback = VoidCallback::default();
/// ```
///
#[derive(Default, Clone)]
pub struct VoidCallback {
    inner: CallbackInner<(), ()>,
}

impl VoidCallback {
    pub fn callable(&self) -> Option<&Self> {
        if self.inner.is_default() { None } else { Some(self) }
    }

    /// Create a `VoidCallback` from an async function (`fn() -> impl Future<Output=()>`).
    ///
    /// Unlike the `From<F>` impl (which requires `Fn()`), this accepts
    /// functions that return a `Future` — the callback stores the future
    /// producer and must be driven by an executor.
    ///
    /// This method accepts `FnOnce` closures, which allows capturing mutable
    /// state. The closure is wrapped in a `Mutex<Option<F>>` and taken on
    /// first invocation — subsequent calls produce an empty future.
    ///
    /// # Example
    /// ```ignore
    /// let data = vec![1, 2, 3];
    /// let cb = VoidCallback::from_async(move || async move {
    ///     process(data).await;
    /// });
    /// ```
    pub fn from_async<F, Fut>(f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let f = std::sync::Mutex::new(Some(f));
        Self {
            inner: CallbackInner(Rc::new(UnsafeCell::new(Some(RawInnerCallback::Async(
                Box::new(move |_| {
                    let f = f.lock().unwrap().take();
                    if let Some(f) = f { Box::pin(f()) } else { Box::pin(async {}) }
                }),
            ))))),
        }
    }
}

unsafe impl Send for VoidCallback {}
unsafe impl Sync for VoidCallback {}

impl Debug for VoidCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoidCallback() -> ()").finish()
    }
}

impl<F: Fn() + 'static> From<F> for VoidCallback {
    fn from(f: F) -> Self {
        Self { inner: CallbackInner::from(move |_| f()) }
    }
}

// impl<F: Fn() + 'static> From<Option<F>> for VoidCallback {
//     fn from(f: Option<F>) -> Self {
//         match f {
//             Some(f) => Self { inner: CallbackInner::from(move |_| f()) },
//             None => Self{inner: CallbackInner::default()},
//         }
//     }
// }

impl<F, Fut> From<AsyncCallback<F>> for VoidCallback
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        let f = std::sync::Mutex::new(Some(ac.0));
        Self {
            inner: CallbackInner(Rc::new(UnsafeCell::new(Some(RawInnerCallback::Async(
                Box::new(move |_| {
                    let f = f.lock().unwrap().take();
                    if let Some(f) = f { Box::pin(f()) } else { Box::pin(async {}) }
                }),
            ))))),
        }
    }
}

impl CallbackExecutor for VoidCallback {
    type Args = ();
    type Output = ();
    fn get(&self) -> &Option<RawInnerCallback<Self::Args, Self::Output>> {
        unsafe { &*self.inner.get() }
    }
}

pub trait IntoVoidCallback {
    fn into_void_callback(self) -> VoidCallback;
}
