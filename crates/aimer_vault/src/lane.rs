//! Runs blocking secret-store calls, in order, on Venus's offload pool.
//!
//! A [`Lane`] owns no thread. Each operation becomes a job in a per-vault FIFO
//! queue; when the queue goes from empty to busy, one drain job is handed to
//! `Venus::spawn_blocking` and runs jobs until the queue is empty again. Jobs
//! therefore run one at a time in the order they were issued, even though the
//! pool has several workers, so a `read` issued after a `write` sees it.
//!
//! The awaiting task is woken through the thread-safe waker Venus gives its
//! futures, so the result is applied in the frame phase the task belongs to.

use std::collections::VecDeque;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use aimer_venus::Venus;

use crate::error::{VaultCapability, VaultError, VaultResult};
use crate::vault::{VaultFuture, ready};

type Job = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    /// Whether a drain job is queued or running. Set and cleared under the same
    /// lock that guards `jobs`, so a job is never left with nobody to run it.
    draining: bool,
}

/// An ordered queue of blocking jobs. Clones share the queue.
#[derive(Clone, Default)]
pub(crate) struct Lane {
    queue: Arc<Mutex<Queue>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every job runs under `catch_unwind`, so a poisoned lock holds a queue that
    // is still consistent.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Lane {
    /// Queues `work` behind everything already issued to this lane and returns
    /// its result.
    ///
    /// Resolves to `Unavailable { capability: Runtime }` when no Venus runtime is
    /// installed on the calling thread, because there is no pool to run on.
    pub(crate) fn submit<T, F>(&self, work: F) -> VaultFuture<T>
    where
        T: Send + 'static,
        F: FnOnce() -> VaultResult<T> + Send + 'static,
    {
        let Some(venus) = Venus::current() else {
            return ready(Err(VaultError::Unavailable {
                capability: VaultCapability::Runtime,
            }));
        };

        let (completer, receive) = oneshot();
        let job: Job = Box::new(move || completer.complete(work()));
        let start_drain = {
            let mut queue = lock(&self.queue);
            queue.jobs.push_back(job);
            !std::mem::replace(&mut queue.draining, true)
        };
        if start_drain {
            let queue = Arc::clone(&self.queue);
            // The jobs report through their own oneshots; the drain's result is not
            // observed, and dropping the handle does not cancel the work.
            drop(venus.spawn_blocking(move || drain(&queue)));
        }
        Box::pin(receive)
    }
}

fn drain(queue: &Mutex<Queue>) {
    loop {
        let job = {
            let mut queue = lock(queue);
            match queue.jobs.pop_front() {
                Some(job) => job,
                None => {
                    queue.draining = false;
                    return;
                }
            }
        };
        // A panicking job must not strand the jobs behind it. Its completer is
        // dropped by the unwind, which resolves its future with an error.
        let _ = catch_unwind(AssertUnwindSafe(job));
    }
}

struct Slot<T> {
    state: Mutex<SlotState<T>>,
}

struct SlotState<T> {
    result: Option<VaultResult<T>>,
    waker: Option<Waker>,
}

fn oneshot<T>() -> (Completer<T>, Receive<T>) {
    let slot = Arc::new(Slot {
        state: Mutex::new(SlotState {
            result: None,
            waker: None,
        }),
    });
    (
        Completer {
            slot: Arc::clone(&slot),
            completed: false,
        },
        Receive { slot },
    )
}

/// The worker half of a result slot.
struct Completer<T> {
    slot: Arc<Slot<T>>,
    completed: bool,
}

impl<T> Completer<T> {
    fn complete(mut self, result: VaultResult<T>) {
        self.completed = true;
        self.finish(result);
    }

    fn finish(&self, result: VaultResult<T>) {
        let waker = {
            let mut state = lock(&self.slot.state);
            if state.result.is_some() {
                return;
            }
            state.result = Some(result);
            state.waker.take()
        };
        // Outside the lock: waking may run arbitrary scheduler code.
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Drop for Completer<T> {
    /// A job that never completed (it panicked, or its queue was dropped)
    /// resolves its future with an error rather than leaving it pending forever.
    fn drop(&mut self) {
        if !self.completed {
            self.finish(Err(VaultError::Interrupted));
        }
    }
}

/// The awaiting half of a result slot.
struct Receive<T> {
    slot: Arc<Slot<T>>,
}

impl<T> Future for Receive<T> {
    type Output = VaultResult<T>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = lock(&self.slot.state);
        match state.result.take() {
            Some(result) => Poll::Ready(result),
            None => {
                match &mut state.waker {
                    Some(waker) => waker.clone_from(context.waker()),
                    waker @ None => *waker = Some(context.waker().clone()),
                }
                Poll::Pending
            }
        }
    }
}
