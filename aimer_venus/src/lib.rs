#![doc = include_str!("../README.md")]

mod budget;
// The browser has no threads to offload onto, and `requestIdleCallback` plus a
// budgeted idle phase is what takes its place there.
#[cfg(not(target_arch = "wasm32"))]
mod offload;
mod scheduler;
mod task;
mod venus;
mod yielding;

pub use crate::budget::{
    FrameBudget, FrameGovernor, IDLE_SLICE_FLOOR, MICROTASK_BUDGET_WARNING, time_remaining_in_frame,
};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::offload::{OffloadPool, Offloaded};
pub use crate::scheduler::LocalScheduler;
pub use crate::task::{Notifier, Phase, ScopeId, TaskId, TaskScope};
pub use crate::venus::{Venus, spawn_local};
pub use crate::yielding::{YieldNow, yield_if_over_budget, yield_now};
