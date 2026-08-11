use std::task::Waker;

use crate::task::{LocalFuture, Phase, ScopeId, TaskId};

/// One spawned task, as the scheduler stores it.
///
/// `future` is an [`Option`] because polling *moves the future out* of the
/// slab: the scheduler's interior state must be un-borrowed while a task runs,
/// or a task that spawns another task would panic on a re-entrant borrow.
pub(crate) struct Task {
    future: Option<LocalFuture>,
    waker: Waker,
    pub(crate) phase: Phase,
    pub(crate) scope: ScopeId,
    /// Whether this task is already sitting in a ready queue. Without it a task
    /// woken five times before it next runs would be polled five times.
    pub(crate) queued: bool,
}

struct Slot {
    generation: u32,
    task: Option<Task>,
}

/// A generational arena of live tasks.
///
/// Vacated slots are reused, so a long-lived application spawning a task per
/// frame does not grow the arena. Each reuse bumps the slot's generation, which
/// is the whole reason [`TaskId`] carries one: a ready queue or a cross-thread
/// wake may still name a task that has since been cancelled, and that stale id
/// must resolve to "gone" rather than to whoever moved into the slot.
pub(crate) struct TaskSlab {
    slots: Vec<Slot>,
    vacant: Vec<u32>,
    live: usize,
}

impl TaskSlab {
    pub(crate) const fn new() -> Self {
        Self {
            slots: Vec::new(),
            vacant: Vec::new(),
            live: 0,
        }
    }

    /// Reserves an id without occupying it.
    ///
    /// A task's waker names the task, so the id has to exist before the [`Task`]
    /// can be built. Every `reserve` is immediately followed by [`Self::occupy`].
    pub(crate) fn reserve(&mut self) -> TaskId {
        match self.vacant.pop() {
            Some(index) => TaskId::new(index, self.slots[index as usize].generation),
            None => {
                let index = self.slots.len() as u32;
                self.slots.push(Slot {
                    generation: 0,
                    task: None,
                });
                TaskId::new(index, 0)
            }
        }
    }

    /// Stores `task` in the slot reserved for `id`.
    pub(crate) fn occupy(
        &mut self,
        id: TaskId,
        future: LocalFuture,
        waker: Waker,
        phase: Phase,
        scope: ScopeId,
    ) {
        let slot = &mut self.slots[id.index() as usize];
        debug_assert!(slot.task.is_none(), "a reserved slot was occupied twice");
        slot.task = Some(Task {
            future: Some(future),
            waker,
            phase,
            scope,
            queued: true,
        });
        self.live += 1;
    }

    /// The live task `id` names, or `None` when `id` is stale.
    pub(crate) fn get_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        let slot = self.slots.get_mut(id.index() as usize)?;
        if slot.generation != id.generation() {
            return None;
        }
        slot.task.as_mut()
    }

    /// Whether `id` still names a live task.
    pub(crate) fn contains(&self, id: TaskId) -> bool {
        self.slots
            .get(id.index() as usize)
            .is_some_and(|slot| slot.generation == id.generation() && slot.task.is_some())
    }

    /// Lends the future out for one poll, together with the task's waker.
    ///
    /// Returns `None` when `id` is stale, or when the future is already out —
    /// which is how a task woken while it is running avoids being polled
    /// re-entrantly.
    pub(crate) fn lend(&mut self, id: TaskId) -> Option<(LocalFuture, Waker)> {
        let task = self.get_mut(id)?;
        let future = task.future.take()?;
        Some((future, task.waker.clone()))
    }

    /// Gives a lent future back after a `Poll::Pending`.
    ///
    /// Returns `false` when the task was cancelled while it was running — a
    /// widget unmounting itself from its own handler — in which case the future
    /// is dropped here instead.
    pub(crate) fn restore(&mut self, id: TaskId, future: LocalFuture) -> bool {
        match self.get_mut(id) {
            Some(task) => {
                task.future = Some(future);
                true
            }
            None => false,
        }
    }

    /// Drops the task `id` names, vacating its slot.
    pub(crate) fn remove(&mut self, id: TaskId) -> bool {
        let Some(slot) = self.slots.get_mut(id.index() as usize) else {
            return false;
        };
        if slot.generation != id.generation() || slot.task.is_none() {
            return false;
        }

        slot.task = None;
        slot.generation = slot.generation.wrapping_add(1);
        self.vacant.push(id.index());
        self.live -= 1;
        true
    }

    /// Drops every task belonging to `scope`, returning how many there were.
    ///
    /// This is what an unmounting element relies on: no abort handle per task,
    /// no generation counter per widget, one sweep.
    pub(crate) fn remove_scope(&mut self, scope: ScopeId) -> usize {
        let mut removed = 0;
        for index in 0..self.slots.len() {
            let slot = &mut self.slots[index];
            if slot.task.as_ref().is_none_or(|task| task.scope != scope) {
                continue;
            }

            slot.task = None;
            slot.generation = slot.generation.wrapping_add(1);
            self.vacant.push(index as u32);
            removed += 1;
        }
        self.live -= removed;
        removed
    }

    /// How many tasks are alive.
    pub(crate) const fn len(&self) -> usize {
        self.live
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(slab: &mut TaskSlab, scope: ScopeId) -> TaskId {
        let id = slab.reserve();
        slab.occupy(
            id,
            Box::pin(async {}),
            Waker::noop().clone(),
            Phase::Microtask,
            scope,
        );
        id
    }

    #[test]
    fn a_reused_slot_invalidates_the_id_that_vacated_it() {
        let mut slab = TaskSlab::new();

        let first = insert(&mut slab, ScopeId::ROOT);
        assert!(slab.remove(first));

        let second = insert(&mut slab, ScopeId::ROOT);

        assert_eq!(first.index(), second.index(), "the slot should be reused");
        assert!(!slab.contains(first));
        assert!(slab.contains(second));
        assert_eq!(slab.len(), 1);
    }

    #[test]
    fn a_lent_future_cannot_be_lent_twice() {
        let mut slab = TaskSlab::new();
        let id = insert(&mut slab, ScopeId::ROOT);

        let (future, _) = slab.lend(id).expect("the future to be available");
        assert!(slab.lend(id).is_none());

        assert!(slab.restore(id, future));
        assert!(slab.lend(id).is_some());
    }

    #[test]
    fn a_future_lent_by_a_cancelled_task_is_not_restored() {
        let mut slab = TaskSlab::new();
        let id = insert(&mut slab, ScopeId::ROOT);

        let (future, _) = slab.lend(id).expect("the future to be available");
        slab.remove(id);

        assert!(!slab.restore(id, future));
        assert_eq!(slab.len(), 0);
    }

    #[test]
    fn removing_a_scope_leaves_every_other_scope_alone() {
        let mut slab = TaskSlab::new();
        let doomed = ScopeId::new(1);
        let kept = ScopeId::new(2);

        let first = insert(&mut slab, doomed);
        let second = insert(&mut slab, kept);
        let third = insert(&mut slab, doomed);

        assert_eq!(slab.remove_scope(doomed), 2);

        assert!(!slab.contains(first));
        assert!(slab.contains(second));
        assert!(!slab.contains(third));
        assert_eq!(slab.len(), 1);
    }
}
