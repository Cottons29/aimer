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
/// Returns a reference to the optional callback function.
///
/// # Example
/// ```rust
/// use some_module::{CallbackExecutor, RawInnerCallback};
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
/// use crate::control::callback::*;
/// // Example of using a Callback with specific argument and return types
/// let callback = Callback {
///     inner: CallbackInner::new(|x: i32| x + 1),
/// };
/// ```
///
pub struct Callback<Args = (), Return = ()> {
    inner: CallbackInner<Args, Return>,
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
        Self { inner: CallbackInner::from(move |args| f(args)) }
    }
}

impl<Args, Return, F, Fut> From<AsyncCallback<F>> for Callback<Args, Return>
where
    F: Fn(Args) -> Fut + Send + 'static,
    Fut: Future<Output = Return> + Send + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        Self { inner: CallbackInner(Rc::new(UnsafeCell::new(Some(RawInnerCallback::Async(Box::new(move |args| Box::pin(ac.0(args)))))))) }
    }
}

impl<P, R> CallbackExecutor for Callback<P,R> {
    type Args = P;
    type Output = R;
    fn get(&self) -> &Option<RawInnerCallback<Self::Args, Self::Output>> {
        unsafe { &*self.inner.get() }
    }
}



///
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
/// use crate::control::callback::*;
/// // Create a default instance of VoidCallback
/// let callback = VoidCallback::default();
/// ```
///
#[derive(Default, Clone)]
pub struct VoidCallback {
    inner: CallbackInner<(), ()>,
}

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

impl<F, Fut> From<AsyncCallback<F>> for VoidCallback
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        Self { inner: CallbackInner(Rc::new(UnsafeCell::new(Some(RawInnerCallback::Async(Box::new(move |_| Box::pin(ac.0()))))))) }
    }
}

impl CallbackExecutor for VoidCallback {
    type Args = ();
    type Output = ();
     fn get(&self) -> &Option<RawInnerCallback<Self::Args, Self::Output>> {
        unsafe { &*self.inner.get() }
    }
}
