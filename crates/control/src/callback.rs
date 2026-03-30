use std::cell::UnsafeCell;
use std::pin::Pin;
use std::rc::Rc;

/// A callback that can be either synchronous or asynchronous.
// #[cfg(not(target_arch = "wasm32"))]
pub enum Callback<P, R> {
    Sync(Box<dyn Fn(P) -> R>),
    Async(Box<dyn Fn(P) -> Pin<Box<dyn Future<Output = R> + Send>> + Send>),
}

// #[cfg(target_arch = "wasm32")]
// pub enum Callback<P, R> {
//     Sync(Box<dyn Fn(P) -> R>),
//     Async(Box<dyn Fn(P) -> Pin<Box<dyn Future<Output = ()>>>>),
// }

impl<P, R> std::fmt::Debug for Callback<P, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let p_type = std::any::type_name::<P>();
        let r_type = std::any::type_name::<R>();
        match self {
            Callback::Sync(_) => write!(f, "Callback::Sync({p_type} -> {r_type})"),
            Callback::Async(_) => write!(f, "Callback::Async({p_type} -> {r_type})"),
        }
    }
}

// impl<P, R, F: Fn(P) -> R + 'static> From<F> for Callback<P, R> {
//     fn from(f: F) -> Self {
//         Callback::Sync(Box::new(f))
//     }
// }

/// Wrapper to convert an async closure into a `Callback::Async`.
pub struct AsyncCallback<F>(pub F);

#[cfg(not(target_arch = "wasm32"))]
impl<F, Fut, P, R> From<AsyncCallback<F>> for Callback<P, R>
where
    F: Fn(P) -> Fut + Send + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        Callback::Async(Box::new(move |param| Box::pin(ac.0(param))))
    }
}

// #[cfg(target_arch = "wasm32")]
// impl<F, Fut> From<AsyncCallback<F>> for Callback
// where
//     F: Fn() -> Fut + 'static,
//     Fut: Future<Output = ()> + 'static,
// {
//     fn from(ac: AsyncCallback<F>) -> Self {
//         Callback::Async(Box::new(move || Box::pin(ac.0())))
//     }
// }

/// A holder for a `Callback` that can be shared via `Rc`.
/// Accepts both sync closures and `AsyncCallback`-wrapped async closures via `.into()`.
#[derive(Debug)]
pub struct CallbackHolder<P, R>(Rc<UnsafeCell<Option<Callback<P, R>>>>);

impl<P, R> CallbackHolder<P, R> {
    pub fn get(&self) -> *mut Option<Callback<P, R>> {
        self.0.get()
    }
}

impl<P, R> Default for CallbackHolder<P, R> {
    fn default() -> Self {
        CallbackHolder(Rc::new(UnsafeCell::new(None)))
    }
}

impl<P, R> Clone for CallbackHolder<P, R> {
    fn clone(&self) -> Self {
        CallbackHolder(self.0.clone())
    }
}

impl<P, R, F: Fn(P) -> R + 'static> From<F> for CallbackHolder<P, R> {
    fn from(f: F) -> Self {
        CallbackHolder(Rc::new(UnsafeCell::new(Some(Callback::Sync(Box::new(f))))))
    }
}

// #[cfg(not(target_arch = "wasm32"))]
impl<P, R, F, Fut> From<AsyncCallback<F>> for CallbackHolder<P, R>
where
    F: Fn(P) -> Fut + Send + 'static,
    Fut: Future<Output = R> + Send + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        CallbackHolder(Rc::new(UnsafeCell::new(Some(Callback::Async(Box::new(move |param| Box::pin(ac.0(param))))))))
    }
}

// #[cfg(target_arch = "wasm32")]
// impl<F, Fut> From<AsyncCallback<F>> for CallbackHolder
// where
//     F: Fn() -> Fut + 'static,
//     Fut: Future<Output = ()> + 'static,
// {
//     fn from(ac: AsyncCallback<F>) -> Self {
//         CallbackHolder(Rc::new(UnsafeCell::new(Some(Callback::Async(Box::new(move || Box::pin(ac.0())))))))
//     }
// }
