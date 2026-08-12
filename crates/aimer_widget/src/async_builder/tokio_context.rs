//! Keeping a user's future inside the application's Tokio context while Venus
//! polls it.
//!
//! An [`AsyncBuilder`](super::AsyncBuilder) future is now driven on the thread
//! that owns the frame rather than on a Tokio worker. That is the point of the
//! migration — it is what lets the future capture an [`Rc`](std::rc::Rc) and
//! what makes its result visible to the same frame's build — but it removes a
//! guarantee the old path gave away for free: Tokio's resources refuse to be
//! *created* outside a runtime context.
//!
//! So `reqwest::get(..).await`, `tokio::fs::read(..).await`, a `sleep` — every
//! future that constructs a Tokio resource lazily on its first poll — would
//! panic with "there is no reactor running" if it were polled bare.
//!
//! The fix is to enter the context for the duration of each poll and no longer.
//! Holding an `EnterGuard` across an await point would leave the whole thread
//! marked as being inside the runtime between polls, which is exactly the state
//! the guard exists to scope.

#[cfg(not(target_arch = "wasm32"))]
pub(super) use native::bind;
#[cfg(target_arch = "wasm32")]
pub(super) use browser::bind;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use tokio::runtime::Handle;

    use crate::base::BuildContext;

    /// Wraps `future` so that every poll happens inside `ctx`'s Tokio runtime.
    ///
    /// The handle is cloned once per request rather than once per poll, and the
    /// future is boxed once at the same moment; both costs are paid when a
    /// request is launched, never on the frame path.
    #[inline]
    pub(in crate::async_builder) fn bind<F: Future>(
        ctx: &BuildContext,
        future: F,
    ) -> InTokioContext<F> {
        InTokioContext {
            handle: ctx.async_handle.clone(),
            future: Box::pin(future),
        }
    }

    /// A future that enters a Tokio runtime for exactly as long as it is being
    /// polled.
    pub(in crate::async_builder) struct InTokioContext<F> {
        handle: Handle,
        future: Pin<Box<F>>,
    }

    impl<F: Future> Future for InTokioContext<F> {
        type Output = F::Output;

        #[inline]
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.get_mut();
            let _guard = this.handle.enter();
            this.future.as_mut().poll(cx)
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use crate::base::BuildContext;

    /// Returns `future` unchanged.
    ///
    /// The browser has one task queue and no runtime to be inside of, so there
    /// is nothing to enter and no wrapper to pay for.
    #[inline]
    pub(in crate::async_builder) fn bind<F: Future>(_ctx: &BuildContext, future: F) -> F {
        future
    }
}
