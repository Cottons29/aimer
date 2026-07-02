use std::cell::UnsafeCell;
use std::pin::Pin;
use std::rc::Rc;

type BoxedFuture<P, R> = Box<dyn Fn(P) -> Pin<Box<dyn Future<Output = R> + Send>> + Send>;

/// A callback that can be either synchronous or asynchronous.
pub enum RawInnerCallback<P, R> {
    Sync(Box<dyn Fn(P) -> R>),
    Async(BoxedFuture<P, R>),
    Empty,
}

impl<P, R> RawInnerCallback<P, R> {
    pub fn is_not_empty(&self) -> bool {
        !matches!(self, RawInnerCallback::Empty)
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, RawInnerCallback::Empty)
    }
}

impl<P, R> std::fmt::Debug for RawInnerCallback<P, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let p_type = std::any::type_name::<P>();
        let r_type = std::any::type_name::<R>();
        match self {
            RawInnerCallback::Sync(_) => write!(f, "Callback::Sync({p_type} -> {r_type})"),
            RawInnerCallback::Async(_) => write!(f, "Callback::Async({p_type} -> {r_type})"),
            RawInnerCallback::Empty => write!(f, "Callback::Empty"),
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

thread_local! {
    /// Shared sentinel for all empty callbacks.
    ///
    /// All `CallbackInner::default()` clones point to the same `Rc` allocation.
    ///
    /// **Safety**: the inner value is always `None` — no `P`/`R` values are ever read or written, so casting the type parameters is sound.
    static EMPTY_CB: Rc<UnsafeCell<Option<RawInnerCallback<(), ()>>>> =
        Rc::new(UnsafeCell::new(None));

    /// Data pointer of `EMPTY_CB`, used by `is_default()` for O(1) identity checks.
    static EMPTY_CB_PTR: *const () = {
        EMPTY_CB.with(|rc| Rc::as_ptr(rc) as *const ())
    };
}

impl<P, R> CallbackInner<P, R> {
    pub fn get(&self) -> *mut Option<RawInnerCallback<P, R>> {
        self.0.get()
    }

    /// Returns `true` if this is the default (empty) sentinel.
    ///
    /// Compares the underlying `Rc` data pointer against the shared
    /// `EMPTY_CB` sentinel — O(1), no allocation, no lock.
    pub fn is_default(&self) -> bool {
        EMPTY_CB_PTR.with(|sentinel| Rc::as_ptr(&self.0) as *const () == *sentinel)
    }
}

impl<P, R> Default for CallbackInner<P, R> {
    fn default() -> Self {
        EMPTY_CB.with(|rc| {
            // Clone the Rc ( bumps refcount only, no new allocation ).
            let cloned: Rc<UnsafeCell<Option<RawInnerCallback<(), ()>>>> = Rc::clone(rc);
            // Cast to the target generic params. Safe because the value is
            // always `None` (layout-compatible, no drop glue for P/R).
            let raw = Rc::into_raw(cloned);
            let casted = raw as *const UnsafeCell<Option<RawInnerCallback<P, R>>>;
            // SAFETY: `raw` came from `Rc::into_raw`, so `Rc::from_raw` is valid.
            CallbackInner(unsafe { Rc::from_raw(casted) })
        })
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
