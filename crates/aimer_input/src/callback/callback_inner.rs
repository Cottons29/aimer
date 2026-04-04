use std::cell::UnsafeCell;
use std::pin::Pin;
use std::rc::Rc;

/// A callback that can be either synchronous or asynchronous.
pub enum RawInnerCallback<P, R> {
    Sync(Box<dyn Fn(P) -> R>),
    Async(Box<dyn Fn(P) -> Pin<Box<dyn Future<Output = R> + Send>> + Send>),
}

impl<P, R> std::fmt::Debug for RawInnerCallback<P, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let p_type = std::any::type_name::<P>();
        let r_type = std::any::type_name::<R>();
        match self {
            RawInnerCallback::Sync(_) => write!(f, "Callback::Sync({p_type} -> {r_type})"),
            RawInnerCallback::Async(_) => write!(f, "Callback::Async({p_type} -> {r_type})"),
        }
    }
}

/// Wrapper to convert an async closure into a `Callback::Async`.
pub struct AsyncCallback<F>(pub F);

#[cfg(not(target_arch = "wasm32"))]
impl<F, Fut, P, R> From<AsyncCallback<F>> for RawInnerCallback<P, R>
where
    F: Fn(P) -> Fut + Send + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        RawInnerCallback::Async(Box::new(move |param| Box::pin(ac.0(param))))
    }
}

/// A holder for a `Callback` that can be shared via `Rc`.
/// Accepts both sync closures and `AsyncCallback`-wrapped async closures via `.into()`.
#[derive(Debug)]
pub struct CallbackInner<P, R>(pub Rc<UnsafeCell<Option<RawInnerCallback<P, R>>>>);

impl<P, R> CallbackInner<P, R> {
    pub fn get(&self) -> *mut Option<RawInnerCallback<P, R>> {
        self.0.get()
    }
}

impl<P, R> Default for CallbackInner<P, R> {
    fn default() -> Self {
        CallbackInner(Rc::new(UnsafeCell::new(None)))
    }
}

impl<P, R> Clone for CallbackInner<P, R> {
    fn clone(&self) -> Self {
        CallbackInner(self.0.clone())
    }
}

impl<P, R, F: Fn(P) -> R + 'static> From<F> for CallbackInner<P, R> {
    fn from(f: F) -> Self {
        CallbackInner(Rc::new(UnsafeCell::new(Some(RawInnerCallback::Sync(Box::new(f))))))
    }
}

impl<P, R, F, Fut> From<AsyncCallback<F>> for CallbackInner<P, R>
where
    F: Fn(P) -> Fut + Send + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        CallbackInner(Rc::new(UnsafeCell::new(Some(RawInnerCallback::Async(Box::new(move |param| Box::pin(ac.0(param))))))))
    }
}
