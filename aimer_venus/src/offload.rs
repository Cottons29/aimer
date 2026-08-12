//! The one place work leaves the UI thread.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread::{self, JoinHandle};

/// One unit of work queued for a worker thread.
type Job = Box<dyn FnOnce() + Send>;

 enum Slot<T> {
    /// Nobody has finished yet; the waker of whoever is awaiting, if it has been
    /// polled at all.
    Waiting(Option<Waker>),
    Finished(T),
    Delivered,
}

/// The rendezvous between a worker thread and the awaiting UI-thread task.
struct Rendezvous<T> {
    slot: Mutex<Slot<T>>,
}

impl<T> Rendezvous<T> {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            slot: Mutex::new(Slot::Waiting(None)),
        })
    }

    /// Publishes `value` and wakes the awaiting task.
    ///
    /// The waker is taken *before* the lock is released, and called after, so a
    /// wake never runs while the worker holds the mutex the UI thread is about
    /// to want.
    fn complete(&self, value: T) {
        let waker = {
            let Ok(mut slot) = self.slot.lock() else {
                return;
            };
            match std::mem::replace(&mut *slot, Slot::Finished(value)) {
                Slot::Waiting(waker) => waker,
                // Only a worker completes a rendezvous, and each job runs once.
                _ => None,
            }
        };

        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// A value being computed on a worker thread.
///
/// Awaited by a task on the UI thread, which keeps its non-`Send` captures the
/// whole time: only the closure and its result cross the boundary. When the
/// value arrives, the awaiting task is woken, and the wake lands in the frame
/// phase the task belongs to — so the result is applied to state at a defined
/// point in the frame rather than "eventually".
///
/// See [`Venus::offload`](crate::Venus::offload).
#[must_use = "offloaded work is only observed by awaiting it"]
pub struct Offloaded<T> {
    rendezvous: Arc<Rendezvous<T>>,
}

impl<T> Future for Offloaded<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Ok(mut slot) = self.rendezvous.slot.lock() else {
            // The worker panicked while holding the lock. The value can never
            // arrive, so the only honest answer is to keep the task parked
            // rather than to invent one.
            return Poll::Pending;
        };

        match &mut *slot {
            Slot::Waiting(waker) => {
                if waker.as_ref().is_none_or(|held| !held.will_wake(cx.waker())) {
                    *waker = Some(cx.waker().clone());
                }
                Poll::Pending
            }
            Slot::Finished(_) => match std::mem::replace(&mut *slot, Slot::Delivered) {
                Slot::Finished(value) => Poll::Ready(value),
                _ => unreachable!("the slot was just observed to be finished"),
            },
            Slot::Delivered => {
                debug_assert!(false, "an `Offloaded` future was polled after completion");
                Poll::Pending
            }
        }
    }
}

/// One worker's slice of the pool's shared state.
struct WorkerSlot {
    /// This worker's own job queue: the pool pushes here, the owner pops from
    /// the front, and a worker whose own queue is empty steals from a sibling's
    /// front. Per-worker queues are what keep dispatch parallel — two workers
    /// taking jobs touch two different locks, not one shared one.
    jobs: Mutex<VecDeque<Job>>,
    /// Raised by the worker when a full scan of every queue found nothing,
    /// lowered when it picks work up again.
    ///
    /// The handshake that makes the flag reliable: the worker raises it
    /// *before* its final scan, and the submitter reads it *after* pushing a
    /// job — both through `SeqCst` and the queue locks — so any job is either
    /// seen by that final scan or its submitter sees the raised flag and rings
    /// the alarm.
    idle: AtomicBool,
    /// The wake token: `true` means the worker owes the queues another scan.
    /// Guarded by its own mutex so a ring landing between the final scan and
    /// the wait is never lost.
    token: Mutex<bool>,
    alarm: Condvar,
}

impl WorkerSlot {
    fn new() -> Self {
        Self {
            jobs: Mutex::new(VecDeque::new()),
            idle: AtomicBool::new(false),
            token: Mutex::new(false),
            alarm: Condvar::new(),
        }
    }

    /// Hands the worker a wake token and rings its alarm.
    fn ring(&self) {
        if let Ok(mut token) = self.token.lock() {
            *token = true;
        }
        self.alarm.notify_one();
    }

    /// Blocks until a wake token arrives, then consumes it.
    fn wait_for_ring(&self) {
        let Ok(mut token) = self.token.lock() else {
            return;
        };
        while !*token {
            match self.alarm.wait(token) {
                Ok(woken) => token = woken,
                Err(_) => return,
            }
        }
        *token = false;
    }
}

/// The state a pool shares with its workers.
struct PoolShared {
    slots: Box<[WorkerSlot]>,
    shutdown: AtomicBool,
}

/// A small pool of threads for work that cannot be sliced.
///
/// A frame budget makes this non-optional rather than a nicety: a forty
/// millisecond parse or a PNG decode has no loop to yield from, so no amount of
/// cooperative scheduling saves the frame — the work has to leave the thread
/// entirely and come back as a wake.
///
/// This is also the *only* place Venus requires `Send`, which is the point.
/// Requiring it at the callback layer, as a general-purpose runtime does, taxes
/// every handler in the framework for the sake of the few that do I/O.
///
/// # Dispatch
///
/// Every worker owns its own queue; a submitted job goes to an idle worker's
/// queue when one exists, and round-robin across the busy ones otherwise. A
/// worker whose own queue runs dry steals from its siblings before parking, so
/// a job queued behind a slow one is picked up by whichever worker frees up
/// first — no single lock serializes dispatch, and no worker sleeps while work
/// is stranded elsewhere.
///
/// # Panics
///
/// A closure that panics takes its worker thread with it, and the awaiting task
/// stays parked forever rather than observing a value that does not exist. Do
/// not panic in offloaded work; the release profile aborts the process on panic
/// in any case.
pub struct OffloadPool {
    shared: Arc<PoolShared>,
    /// Where the next job lands when every worker is busy, so a burst spreads
    /// across the queues instead of piling onto one.
    cursor: AtomicUsize,
    workers: Vec<JoinHandle<()>>,
}

impl OffloadPool {
    /// Spawns a pool of `threads` workers, at least one.
    pub fn new(threads: usize) -> Self {
        let threads = threads.max(1);
        let shared = Arc::new(PoolShared {
            slots: (0..threads).map(|_| WorkerSlot::new()).collect(),
            shutdown: AtomicBool::new(false),
        });

        let workers = (0..threads)
            .map(|index| {
                let shared = Arc::clone(&shared);
                thread::Builder::new()
                    .name(format!("aimer-venus-offload-{index}"))
                    .spawn(move || worker(&shared, index))
                    .expect("an offload worker thread")
            })
            .collect();

        Self {
            shared,
            cursor: AtomicUsize::new(0),
            workers,
        }
    }

    /// Spawns a pool sized for the machine, leaving the UI thread a core.
    ///
    /// Capped at four: offloaded work in a GUI is bursty, and threads parked on
    /// an empty queue are pure footprint.
    pub fn with_default_threads() -> Self {
        let available = thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(2);
        Self::new(available.saturating_sub(1).clamp(1, 4))
    }

    /// Runs `work` on a worker thread, resolving on the UI thread.
    pub fn offload<T, F>(&self, work: F) -> Offloaded<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let rendezvous = Rendezvous::new();
        let completion = Arc::clone(&rendezvous);

        self.submit(Box::new(move || completion.complete(work())));
        Offloaded { rendezvous }
    }

    /// Queues `job` and makes sure a worker will get to it.
    fn submit(&self, job: Job) {
        let slots = &self.shared.slots;

        // An idle worker's own queue is the best home for the job; when every
        // worker is busy, round-robin spreads the burst across their queues.
        let target = Self::idle_worker(slots)
            .unwrap_or_else(|| self.cursor.fetch_add(1, Ordering::Relaxed) % slots.len());
        if let Ok(mut jobs) = slots[target].jobs.lock() {
            jobs.push_back(job);
        }

        // Re-read the flags *after* the push — see [`WorkerSlot::idle`]. Any
        // idle worker will do when the target itself is not: it steals.
        let sleeper = if slots[target].idle.load(Ordering::SeqCst) {
            Some(target)
        } else {
            Self::idle_worker(slots)
        };
        if let Some(index) = sleeper {
            slots[index].ring();
        }
    }

    /// The first worker currently advertising an empty scan, if any.
    fn idle_worker(slots: &[WorkerSlot]) -> Option<usize> {
        slots
            .iter()
            .position(|slot| slot.idle.load(Ordering::SeqCst))
    }

    /// How many worker threads the pool owns.
    #[inline]
    pub fn thread_count(&self) -> usize {
        self.workers.len()
    }
}

impl Default for OffloadPool {
    #[inline]
    fn default() -> Self {
        Self::with_default_threads()
    }
}

impl Drop for OffloadPool {
    /// Closes the pool and joins the workers.
    ///
    /// A worker only exits once every queue is empty, so a job the pool
    /// accepted still runs; and joining means an application shutting down does
    /// not race a worker that is halfway through writing into a rendezvous.
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        for slot in self.shared.slots.iter() {
            slot.ring();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker(shared: &PoolShared, me: usize) {
    let slot = &shared.slots[me];
    loop {
        if let Some(job) = claim(shared, me) {
            job();
            continue;
        }

        // Nothing anywhere on a first pass. Raise the idle flag *before*
        // scanning once more: a job pushed before the raise is caught by this
        // scan, and one pushed after it sees the flag and rings the alarm — so
        // the worker never sleeps through a submission.
        slot.idle.store(true, Ordering::SeqCst);
        if let Some(job) = claim(shared, me) {
            slot.idle.store(false, Ordering::SeqCst);
            job();
            continue;
        }

        // The shutdown check sits behind the empty scan on purpose: a pool
        // being dropped drains before it dies.
        if shared.shutdown.load(Ordering::SeqCst) {
            return;
        }

        slot.wait_for_ring();
        slot.idle.store(false, Ordering::SeqCst);
    }
}

/// Takes one job: the front of this worker's own queue, or failing that, the
/// front of a sibling's — a worker with time on its hands steals rather than
/// letting work sit behind a busy peer.
fn claim(shared: &PoolShared, me: usize) -> Option<Job> {
    let slots = &shared.slots;
    if let Ok(mut jobs) = slots[me].jobs.lock()
        && let Some(job) = jobs.pop_front()
    {
        return Some(job);
    }

    // Victims are scanned starting past `me`, so no single queue is every
    // thief's first stop.
    for offset in 1..slots.len() {
        let victim = (me + offset) % slots.len();
        if let Ok(mut jobs) = slots[victim].jobs.lock()
            && let Some(job) = jobs.pop_front()
        {
            return Some(job);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::Venus;

    /// Spins until `condition` holds, panicking with `what` if it never does.
    ///
    /// The deadline is generous because CI machines stall; a passing run never
    /// comes near it.
    fn wait_until(what: &str, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            thread::yield_now();
        }
    }

    #[test]
    fn a_pool_always_has_at_least_one_worker() {
        assert_eq!(OffloadPool::new(0).thread_count(), 1);
        assert!(OffloadPool::with_default_threads().thread_count() >= 1);
    }

    // The dispatch property the pool must never lose: work queued while a
    // worker is stuck belongs to the *pool*, not to that worker. Every worker
    // is first wedged on a gate, a batch of quick jobs is queued behind them,
    // and then a single worker is released — that one worker must be able to
    // reach and finish every quick job, wherever it was queued.
    #[test]
    fn a_blocked_worker_does_not_strand_the_jobs_queued_behind_it() {
        let pool = OffloadPool::new(2);
        let occupied = Arc::new(AtomicUsize::new(0));

        let gates: Vec<mpsc::Sender<()>> = (0..pool.thread_count())
            .map(|_| {
                let (open, gate) = mpsc::channel::<()>();
                let counted = occupied.clone();
                // The result is observed through the counter, not awaited.
                drop(pool.offload(move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                    let _ = gate.recv();
                }));
                open
            })
            .collect();
        wait_until("every worker to pick up its blocker", || {
            occupied.load(Ordering::SeqCst) == 2
        });

        let done = Arc::new(AtomicUsize::new(0));
        for _ in 0..8 {
            let counted = done.clone();
            drop(pool.offload(move || {
                counted.fetch_add(1, Ordering::SeqCst);
            }));
        }

        // One worker comes back; the other stays wedged the whole time.
        gates[0].send(()).expect("the blocked worker to be alive");
        wait_until("the free worker to finish every queued job", || {
            done.load(Ordering::SeqCst) == 8
        });

        gates[1].send(()).expect("the blocked worker to be alive");
    }

    // Dropping the pool joins the workers, and joining means draining: a job
    // the pool accepted is a job that runs, even when the drop arrives while
    // the queue is still full.
    #[test]
    fn jobs_accepted_before_the_pool_drops_still_run() {
        let pool = OffloadPool::new(1);
        let ran = Arc::new(AtomicUsize::new(0));

        let (open, gate) = mpsc::channel::<()>();
        drop(pool.offload(move || {
            let _ = gate.recv();
        }));
        for _ in 0..16 {
            let counted = ran.clone();
            drop(pool.offload(move || {
                counted.fetch_add(1, Ordering::SeqCst);
            }));
        }

        open.send(()).expect("the blocked worker to be alive");
        drop(pool);
        assert_eq!(ran.load(Ordering::SeqCst), 16);
    }

    // Many small jobs from one submitter — the shape the dispatch rework is
    // for. Every job must land exactly once, none lost to a wake race.
    #[test]
    fn a_storm_of_small_jobs_all_lands() {
        let jobs = 10_000;
        let done = Arc::new(AtomicUsize::new(0));

        let pool = OffloadPool::new(4);
        for _ in 0..jobs {
            let counted = done.clone();
            drop(pool.offload(move || {
                counted.fetch_add(1, Ordering::SeqCst);
            }));
        }
        drop(pool);

        assert_eq!(done.load(Ordering::SeqCst), jobs);
    }

    // The property that makes `offload` worth having: the awaiting task keeps
    // its `Rc` across the boundary, because only the closure and the result
    // cross it.
    #[test]
    fn an_awaiting_task_keeps_its_non_send_state_across_the_boundary() {
        let venus = Venus::new();
        let state = Rc::new(Cell::new(0));

        let runtime = venus.clone();
        let mutated = state.clone();
        venus.spawn(async move {
            let value = runtime
                .offload(|| {
                    thread::sleep(Duration::from_millis(5));
                    41
                })
                .await;
            mutated.set(value + 1);
        });

        while venus.task_count() > 0 {
            venus.run_microtasks();
        }

        assert_eq!(state.get(), 42);
    }

    #[test]
    fn several_offloads_all_come_back() {
        let venus = Venus::new();
        let total = Rc::new(Cell::new(0));

        for value in 1..=8 {
            let runtime = venus.clone();
            let summed = total.clone();
            venus.spawn(async move {
                let value = runtime.offload(move || value).await;
                summed.set(summed.get() + value);
            });
        }

        while venus.task_count() > 0 {
            venus.run_microtasks();
        }

        assert_eq!(total.get(), 36);
    }
}
