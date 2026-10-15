//! Runs file operations, in order, on Venus's offload pool.
//!
//! A [`FileStorage`](super::FileStorage) owns no thread. Each operation becomes
//! a job in a per-store FIFO queue; when the queue goes from empty to busy, one
//! drain job is handed to `Venus::spawn_blocking` and runs jobs until the queue
//! is empty again. That gives two things the old dedicated writer thread gave,
//! without the thread:
//!
//! * **Order.** Jobs run one at a time in the order they were issued, so a
//!   `read` issued after a `write` sees it, and two writes to one key land in
//!   call order, even though the pool has several workers.
//! * **Cheap quota decisions.** Only the drain job touches `usage`, so the
//!   store is scanned once and then kept up to date incrementally.
//!
//! The awaiting task is woken through the same thread-safe waker Venus gives
//! `Offloaded`, so the result is applied in the frame phase the task belongs to.

use std::collections::VecDeque;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use aimer_venus::Venus;

use super::commit;
use crate::error::{
    StorageCapability, StorageError, StorageIoKind, StorageOperation, StorageResult,
};
use crate::limits::{StorageLimits, Usage};
use crate::storage::{StorageFuture, ready};

type Job = Box<dyn FnOnce(&Inner) + Send>;

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    /// Whether a drain job is queued or running. Set and cleared under the same
    /// lock that guards `jobs`, so a job is never left with nobody to run it.
    draining: bool,
}

/// The state shared between a store handle and its drain jobs.
pub(super) struct Inner {
    pub(super) root: PathBuf,
    pub(super) limits: StorageLimits,
    /// `None` until the first operation that needs a quota decision, and again
    /// after any operation that may have left it wrong.
    pub(super) usage: Mutex<Option<Usage>>,
    /// Whether a committed batch left behind by a crash has been completed. Reset
    /// when a batch fails midway, so the next operation finishes it first.
    pub(super) recovered: AtomicBool,
    /// The files of the last committed batch, still waiting for their sync and
    /// for the journal to be retired. `None` when nothing is outstanding.
    pub(super) deferred: Mutex<Option<Vec<std::path::PathBuf>>>,
    queue: Mutex<Queue>,
}

impl Inner {
    pub(super) fn new(root: PathBuf, limits: StorageLimits) -> Self {
        Self {
            root,
            limits,
            usage: Mutex::new(None),
            recovered: AtomicBool::new(false),
            deferred: Mutex::new(None),
            queue: Mutex::new(Queue::default()),
        }
    }
}

/// Locks `mutex`, ignoring poison: every job is run under `catch_unwind`, and a
/// poisoned `usage` is reset rather than trusted.
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Queues `work` behind everything already issued to `inner` and returns its
/// result.
///
/// Resolves to `Unavailable { capability: Runtime }` when no Venus runtime is
/// installed on the calling thread, because there is no pool to run on.
///
/// Before `work` runs, a batch left half-applied by a crash is completed. A
/// `mutates` operation also waits for the last batch to be synced and its
/// journal retired, so a stale journal is never replayed over a newer write;
/// reads do not wait, because they see applied data either way.
pub(super) fn submit<T, F>(
    inner: &Arc<Inner>,
    operation: StorageOperation,
    mutates: bool,
    work: F,
) -> StorageFuture<T>
where
    T: Send + 'static,
    F: FnOnce(&Inner) -> StorageResult<T> + Send + 'static,
{
    let Some(venus) = Venus::current() else {
        return ready(Err(StorageError::Unavailable {
            capability: StorageCapability::Runtime,
        }));
    };

    let (completer, receive) = oneshot(operation);
    let job: Job = Box::new(move |inner| {
        let result = commit::recover(inner)
            .and_then(|()| {
                if mutates {
                    commit::checkpoint(inner)
                } else {
                    Ok(())
                }
            })
            .and_then(|()| work(inner));
        completer.complete(result);
    });
    let start_drain = {
        let mut queue = lock(&inner.queue);
        queue.jobs.push_back(job);
        !std::mem::replace(&mut queue.draining, true)
    };
    if start_drain {
        let inner = Arc::clone(inner);
        // The jobs report through their own oneshots; the drain's result is not
        // observed, and dropping the handle does not cancel the work.
        drop(venus.spawn_blocking(move || drain(&inner)));
    }
    Box::pin(receive)
}

fn drain(inner: &Inner) {
    loop {
        let next = lock(&inner.queue).jobs.pop_front();
        match next {
            Some(job) => {
                // A panicking job must not strand the jobs behind it. Its completer
                // is dropped by the unwind, which resolves its future with an error,
                // and `usage` may be half-updated, so it is rebuilt on next use.
                if catch_unwind(AssertUnwindSafe(|| job(inner))).is_err() {
                    *lock(&inner.usage) = None;
                }
            }
            None => {
                // Idle: finish the deferred syncs of the last batch now, where no
                // caller is waiting on them. A failure leaves them outstanding for
                // the next mutating operation to retry and report.
                let _ = catch_unwind(AssertUnwindSafe(|| commit::checkpoint(inner)));
                let mut queue = lock(&inner.queue);
                // Something may have been queued during the checkpoint.
                if queue.jobs.is_empty() {
                    queue.draining = false;
                    return;
                }
            }
        }
    }
}

/// The slot a job's result is delivered through.
struct Slot<T> {
    state: Mutex<SlotState<T>>,
}

struct SlotState<T> {
    result: Option<StorageResult<T>>,
    waker: Option<Waker>,
}

fn oneshot<T>(operation: StorageOperation) -> (Completer<T>, Receive<T>) {
    let slot = Arc::new(Slot {
        state: Mutex::new(SlotState {
            result: None,
            waker: None,
        }),
    });
    (
        Completer {
            slot: Arc::clone(&slot),
            operation,
            completed: false,
        },
        Receive { slot },
    )
}

/// The worker half of a result slot.
struct Completer<T> {
    slot: Arc<Slot<T>>,
    operation: StorageOperation,
    completed: bool,
}

impl<T> Completer<T> {
    fn complete(mut self, result: StorageResult<T>) {
        self.completed = true;
        self.finish(result);
    }

    fn finish(&self, result: StorageResult<T>) {
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
            self.finish(Err(StorageError::Io {
                operation: self.operation,
                kind: StorageIoKind::Other,
            }));
        }
    }
}

/// The awaiting half of a result slot.
struct Receive<T> {
    slot: Arc<Slot<T>>,
}

impl<T> Future for Receive<T> {
    type Output = StorageResult<T>;

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
