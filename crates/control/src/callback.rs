pub mod callback_inner;

pub use callback_inner::*;
use std::any::type_name;
use std::cell::UnsafeCell;
use std::fmt::Debug;
use std::rc::Rc;

pub trait CallbackExecutor {
    type Args;
    type Output;
    fn get(&self) -> &Option<RawInnerCallback<Self::Args, Self::Output>>;
}
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
