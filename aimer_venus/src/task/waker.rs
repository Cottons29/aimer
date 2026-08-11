use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Wake, Waker};
use std::thread::{self, ThreadId};

use crate::task::TaskId;

/// What Venus calls when a task is woken from a thread that is not the UI
/// thread.
///
/// The event loop may be parked in the platform's "wait for OS events" call
/// when a worker finishes, and no amount of scheduling on the UI thread can
/// help with that — something has to nudge the loop awake. On winit this wraps
/// `EventLoopProxy::send_event`.
pub type Notifier = Box<dyn Fn() + Send + Sync>;

/// The one place where Venus crosses a thread boundary.
///
/// Tasks are non-`Send` and are polled exclusively on the UI thread, but a
/// [`Waker`] is `Send + Sync` by definition, so the *wake* — a task id, nothing
/// more — has to be publishable from anywhere. Holding only ids here is what
/// lets the futures themselves stay `Rc`-friendly.
///
/// The queue is read by the scheduler at the start of every phase. A lock is
/// acceptable on that path because it is uncontended in the common case, and
/// the [`AtomicBool`] means the overwhelmingly common "nothing was woken"
/// answer costs a single relaxed-ordering load and no lock at all.
pub(crate) struct WakeQueue {
    pending: Mutex<Vec<TaskId>>,
    has_pending: AtomicBool,
    notifier: Mutex<Option<Notifier>>,
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
            pending: Mutex::new(Vec::new()),
            has_pending: AtomicBool::new(false),
            notifier: Mutex::new(None),
            owner: thread::current().id(),
        })
    }

    /// Installs the callback used to wake a parked event loop.
    pub(crate) fn set_notifier(&self, notifier: Notifier) {
        if let Ok(mut slot) = self.notifier.lock() {
            *slot = Some(notifier);
        }
    }

    /// Publishes a wake for `id`, nudging the event loop if the wake came from
    /// another thread.
    pub(crate) fn wake(&self, id: TaskId) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.push(id);
        }
        self.has_pending.store(true, Ordering::Release);

        if thread::current().id() == self.owner {
            return;
        }

        if let Ok(notifier) = self.notifier.lock()
            && let Some(notifier) = notifier.as_ref()
        {
            notifier();
        }
    }

    /// Moves every published wake into `out`, leaving the queue empty.
    ///
    /// `out` is the scheduler's reusable scratch buffer, so a steady stream of
    /// wakes allocates nothing.
    pub(crate) fn drain_into(&self, out: &mut Vec<TaskId>) {
        if !self.has_pending.load(Ordering::Acquire) {
            return;
        }

        if let Ok(mut pending) = self.pending.lock() {
            out.append(&mut pending);
        }
        self.has_pending.store(false, Ordering::Release);
    }

    /// Whether anything has been woken since the last drain.
    ///
    /// The event loop uses this to decide whether it may go back to sleep.
    pub(crate) fn has_pending(&self) -> bool {
        self.has_pending.load(Ordering::Acquire)
    }
}

/// The waker handed to one task, for the whole life of that task.
///
/// Built once at spawn and cloned per poll — an [`Arc`] refcount bump — rather
/// than allocated per poll, because at 120 Hz every per-poll allocation is a
/// frame-time risk.
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
