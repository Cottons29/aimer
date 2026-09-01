use std::cell::{Cell, UnsafeCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::{Wake, Waker};
use std::thread::{self, ThreadId};

use crossbeam::queue::SegQueue;

use crate::task::TaskId;

/// What Venus calls when a task is woken from a thread that is not the UI
/// thread.
///
/// The event loop may be parked in the platform's "wait for OS events" call
/// when a worker finishes, and no amount of scheduling on the UI thread can
/// help with that — something has to nudge the loop awake. On winit this wraps
/// `EventLoopProxy::send_event`.
pub type Notifier = Box<dyn Fn() + Send + Sync>;

/// The wakes raised by the UI thread itself, kept out of the cross-thread queue.
///
/// Most wakes never cross a thread: `yield_now`, budget slicing and `set_state`
/// chains all wake from inside a poll, on the UI thread, thousands of times a
/// frame under load. Routing them through the cross-thread queue would put an
/// atomic operation on the hottest path the runtime has, paying for a
/// synchronization that same-thread code does not need.
///
/// # Safety invariant
///
/// Only the thread recorded in [`WakeQueue::owner`] may touch these fields.
/// [`WakeQueue::wake`] checks the current thread before entering the fast
/// path, and [`WakeQueue::drain_into`] / [`WakeQueue::has_pending`] are called
/// exclusively by the scheduler, which is `!Send` and lives on that thread.
struct UiWakes {
    pending: UnsafeCell<Vec<TaskId>>,
    has_pending: Cell<bool>,
}

// SAFETY: every access is confined to the owner thread — see the invariant on
// [`UiWakes`] — so no two threads ever touch the interior data concurrently.
unsafe impl Sync for UiWakes {}

/// The one place where Venus crosses a thread boundary.
///
/// Tasks are non-`Send` and are polled exclusively on the UI thread, but a
/// [`Waker`] is `Send + Sync` by definition, so the *wake* — a task id, nothing
/// more — has to be publishable from anywhere. Holding only ids here is what
/// lets the futures themselves stay `Rc`-friendly.
///
/// Wakes travel one of two roads, split by the thread they come from:
///
/// - **Same-thread wakes** — the overwhelming majority — go into `local`, a
///   plain unsynchronized buffer, because the producer and the consumer are
///   provably the same thread.
/// - **Cross-thread wakes** — a worker finishing an offload — enter the
///   lock-free queue and nudge the parked event loop through the notifier.
///
/// The queue is read by the scheduler at the start of every phase, and the two
/// flags mean the overwhelmingly common "nothing was woken" answer costs one
/// [`Cell`] read plus one atomic load, and no lock at all.
pub(crate) struct WakeQueue {
    /// Wakes raised on the UI thread itself; unsynchronized by design.
    local: UiWakes,
    /// Wakes raised on any other thread.
    shared: SegQueue<TaskId>,
    /// Whether `shared` holds anything, so draining an empty queue never scans.
    has_shared: AtomicBool,
    notifier: OnceLock<Notifier>,
    owner: ThreadId,
}

impl WakeQueue {
    /// Creates a queue owned by the calling thread.
    ///
    /// The calling thread is remembered as the UI thread: wakes originating
    /// there never touch the notifier, because a loop that is running cannot
    /// also be asleep.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            local: UiWakes {
                pending: UnsafeCell::new(Vec::new()),
                has_pending: Cell::new(false),
            },
            shared: SegQueue::new(),
            has_shared: AtomicBool::new(false),
            notifier: OnceLock::new(),
            owner: thread::current().id(),
        })
    }

    /// Installs the callback used to wake a parked event loop.
    pub(crate) fn set_notifier(&self, notifier: Notifier) {
        let _ = self.notifier.set(notifier);
    }

    /// Publishes a wake for `id`, nudging the event loop if the wake came from
    /// another thread.
    ///
    /// A wake from the UI thread takes the lock-free fast path: the loop is
    /// demonstrably awake and the queue's consumer is this very thread, so
    /// neither the queue nor the notifier has anything to add.
    pub(crate) fn wake(&self, id: TaskId) {
        if thread::current().id() == self.owner {
            // SAFETY: this branch runs only on the owner thread, the sole
            // thread allowed to touch `local` — see [`UiWakes`].
            unsafe { (*self.local.pending.get()).push(id) };
            self.local.has_pending.set(true);
            return;
        }

        self.shared.push(id);
        self.has_shared.store(true, Ordering::Release);

        if let Some(notifier) = self.notifier.get() {
            notifier();
        }
    }

    /// Moves every published wake into `out`, leaving the queue empty.
    ///
    /// `out` is the scheduler's reusable scratch buffer, so a steady stream of
    /// wakes allocates nothing. Must be called on the owner thread — which the
    /// scheduler, being `!Send`, guarantees.
    pub(crate) fn drain_into(&self, out: &mut Vec<TaskId>) {
        debug_assert_eq!(thread::current().id(), self.owner);

        if self.local.has_pending.get() {
            // SAFETY: only the owner thread reaches here, and `wake`'s fast
            // path runs on the same thread, so this access cannot overlap
            // another — see [`UiWakes`].
            unsafe { out.append(&mut *self.local.pending.get()) };
            self.local.has_pending.set(false);
        }

        while self.has_shared.swap(false, Ordering::AcqRel) {
            while let Some(id) = self.shared.pop() {
                out.push(id);
            }
        }
    }

    /// Whether anything has been woken since the last drain.
    ///
    /// The event loop uses this to decide whether it may go back to sleep, and
    /// the scheduler gates its dispatch on it. Must be called on the owner
    /// thread — which the scheduler, being `!Send`, guarantees.
    pub(crate) fn has_pending(&self) -> bool {
        debug_assert_eq!(thread::current().id(), self.owner);
        self.local.has_pending.get() || self.has_shared.load(Ordering::Acquire)
    }
}

/// The waker handed to one task, for the whole life of that task.
///
/// Built once at spawn; the resulting [`Waker`] then moves in and out of the
/// slab together with the future, so an ordinary poll costs neither an
/// allocation nor a refcount bump. Clones only happen when a future stashes
/// `cx.waker()` somewhere — which is the future's business, not the
/// scheduler's.
struct TaskWaker {
    id: TaskId,
    queue: Arc<WakeQueue>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.queue.wake(self.id);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.wake(self.id);
    }
}

/// Builds the waker for `id`.
pub(crate) fn waker_for(id: TaskId, queue: &Arc<WakeQueue>) -> Waker {
    Waker::from(Arc::new(TaskWaker {
        id,
        queue: Arc::clone(queue),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(index: u32) -> TaskId {
        TaskId::new(index, 0)
    }

    #[test]
    fn an_empty_queue_drains_to_nothing() {
        let queue = WakeQueue::new();
        let mut drained = Vec::new();

        queue.drain_into(&mut drained);

        assert!(drained.is_empty());
        assert!(!queue.has_pending());
    }

    #[test]
    fn a_wake_survives_until_it_is_drained() {
        let queue = WakeQueue::new();
        let waker = waker_for(id(3), &queue);
        let mut drained = Vec::new();

        waker.wake_by_ref();
        assert!(queue.has_pending());

        queue.drain_into(&mut drained);

        assert_eq!(drained, vec![id(3)]);
        assert!(!queue.has_pending());
    }

    // A wake raised on the UI thread and one raised on a worker land in the
    // same drain: the two publication paths must converge on one queue that the
    // scheduler reads.
    #[test]
    fn wakes_from_the_ui_thread_and_a_worker_drain_together() {
        let queue = WakeQueue::new();
        let mut drained = Vec::new();

        queue.wake(id(0));
        let from_worker = Arc::clone(&queue);
        thread::spawn(move || from_worker.wake(id(1)))
            .join()
            .expect("the worker to finish");
        assert!(queue.has_pending());

        queue.drain_into(&mut drained);

        drained.sort_by_key(|id| id.index());
        assert_eq!(drained, vec![id(0), id(1)]);
        assert!(!queue.has_pending());
    }

    // A wake raised on the UI thread must not ping the event loop: the loop is
    // demonstrably awake, and a redraw request per `yield_now` would be a
    // per-await cost on the hottest path there is.
    #[test]
    fn a_wake_from_the_owning_thread_does_not_notify() {
        use std::sync::atomic::AtomicUsize;

        let queue = WakeQueue::new();
        let pings = Arc::new(AtomicUsize::new(0));
        let counted = pings.clone();
        queue.set_notifier(Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }));

        queue.wake(id(0));
        assert_eq!(pings.load(Ordering::SeqCst), 0);

        let from_worker = Arc::clone(&queue);
        thread::spawn(move || from_worker.wake(id(1)))
            .join()
            .expect("the worker to finish");

        assert_eq!(pings.load(Ordering::SeqCst), 1);
    }
}
