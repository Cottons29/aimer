//! The one place work leaves the UI thread.

use std::pin::Pin;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
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
/// # Panics
///
/// A closure that panics takes its worker thread with it, and the awaiting task
/// stays parked forever rather than observing a value that does not exist. Do
/// not panic in offloaded work; the release profile aborts the process on panic
/// in any case.
pub struct OffloadPool {
    jobs: Option<Sender<Job>>,
    workers: Vec<JoinHandle<()>>,
}

impl OffloadPool {
    /// Spawns a pool of `threads` workers, at least one.
    pub fn new(threads: usize) -> Self {
        let threads = threads.max(1);
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));

        let workers = (0..threads)
            .map(|index| {
                let receiver = Arc::clone(&receiver);
                thread::Builder::new()
                    .name(format!("aimer-venus-offload-{index}"))
                    .spawn(move || worker(receiver))
                    .expect("an offload worker thread")
            })
            .collect();

        Self {
            jobs: Some(sender),
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

        let job: Job = Box::new(move || completion.complete(work()));
        if let Some(jobs) = self.jobs.as_ref() {
            // A send only fails once the pool is being dropped, at which point
            // nothing is left to await the result either.
            let _ = jobs.send(job);
        }

        Offloaded { rendezvous }
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
    /// Closes the queue and joins the workers.
    ///
    /// Joining means an application shutting down does not race a worker that is
    /// halfway through writing into a rendezvous.
    fn drop(&mut self) {
        self.jobs.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker(receiver: Arc<Mutex<Receiver<Job>>>) {
    loop {
        // The lock is held only long enough to take one job, so a worker never
        // blocks its peers while running.
        let job = {
            let Ok(receiver) = receiver.lock() else {
                return;
            };
            receiver.recv()
        };

        match job {
            Ok(job) => job(),
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use super::*;
    use crate::Venus;

    #[test]
    fn a_pool_always_has_at_least_one_worker() {
        assert_eq!(OffloadPool::new(0).thread_count(), 1);
        assert!(OffloadPool::with_default_threads().thread_count() >= 1);
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
