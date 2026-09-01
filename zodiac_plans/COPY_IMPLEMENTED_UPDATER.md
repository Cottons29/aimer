# Copy-implemented `StateUpdater`

This plan redesigns Aimer's `StateUpdater<S>` so framework users can move the
same updater into multiple callbacks without calling `.clone()`, while keeping
state cleanup deterministic and preserving the native and portable backends.

The public `StateUpdater<S>` becomes a small, non-owning handle backed by
`StateUpdaterGeneration<T>`. The live state, mutation queue, dirty flag, and
failure state remain owned by `StateStorage`. Copying the handle copies only
its slot identity. It does not copy `S`, increment a reference count, or extend
the state lifetime.

## Decision

Use an Aimer-owned `StateStorage` and `StateUpdaterGeneration<T>` seam instead
of exposing `Rc` through the public updater. The implementation may borrow the
generational-storage technique used by Freya, but the Aimer interface should
use these names and own the lifecycle rules.

The current implementation in
`crates/aimer_widget/src/widget/stateful.rs` cannot implement `Copy` because
`StateUpdaterInner<S>` contains four `Rc` values:

```rust
struct StateUpdaterInner<S> {
    tx: Rc<StateMutationQueue<S>>,
    state: Rc<SyncState<S>>,
    dirty_source: Rc<DirtySource>,
    failure: Rc<FailureState>,
}
```

`Rc::clone` increments a reference count. A bitwise copy of an `Rc` would not
increment that count and would be unsound. Keep these `Rc` values in a private
`StateStorage` slot and expose only a copyable `StateUpdaterGeneration` key.

## User-facing result

After this change, code in `State::build` can look like this:

```rust
fn build(&self, _ctx: &BuildContext) -> impl Widget {
    let updater = self.updater;
    
    
    // before copy : let on_press_updater = updater.clone()
    Column::new()
        .child(
            Button::new().on_press(move |_| {
                // before copy : on_press_updater.set_state(|state| state.count += 1);
                updater.set_state(|state| state.count += 1);
            }),
        )
        .child(
            Button::new().on_press(move |_| {
                updater.set_state(|state| state.count -= 1);
            }),
        )
}
```

Each `move` closure receives a copy of the handle. Both handles address the
same live state. The `StateUpdater` remains UI-thread-only and is still not a
general `Send + Sync` handle.

## Target ownership model

```text
StatefulElement
    owns StateStorage
        owns StateUpdaterGeneration<UpdaterSlot<S>>
            owns state, mutation queue, dirty source, and failure state

StateUpdater<S>
    stores Option<StateUpdaterGeneration<UpdaterSlot<S>>>
    is Copy and Clone
    does not own the slot
```

The owner is the RAII lifetime guard. When the stateful element is unmounted,
its owner drops the slot and the stored state is dropped. Copied updater
handles do not keep the state alive. A later operation through a stale handle
must detect the dropped generation and return an error, no-op, or panic with a
framework diagnostic according to the chosen method contract.

## Public handle shape

The native core should have a shape equivalent to:

```rust
use std::{marker::PhantomData, rc::Rc};

pub struct StateUpdater<S> {
    slot: Option<StateUpdaterGeneration<UpdaterSlot<S>>>,
    _ui_thread_only: PhantomData<Rc<()>>,
}

impl<S> Copy for StateUpdater<S> {}

impl<S> Clone for StateUpdater<S> {
    fn clone(&self) -> Self {
        *self
    }
}

struct UpdaterSlot<S> {
    tx: Rc<StateMutationQueue<S>>,
    state: Rc<SyncState<S>>,
    dirty_source: Rc<DirtySource>,
    failure: Rc<FailureState>,
}
```

The `Copy` and `Clone` implementations must remain manual. A derive may add an
unnecessary `S: Copy` bound. The `S` type itself must not need to implement
`Copy`.

The framework creates the slot under `StateStorage`:

```rust
let storage = StateStorage::new();
let slot = storage.insert(UpdaterSlot {
    tx: tx.clone(),
    state: state_cell.clone(),
    dirty_source: dirty_source.clone(),
    failure: failure.clone(),
});

let updater = StateUpdater {
    slot: Some(slot),
    _ui_thread_only: PhantomData,
};
```

The `StateStorage` instance must be retained by the live `StatefulElement`.
Returning a `StateUpdaterGeneration` after dropping its storage would create an
immediately stale handle. The backing arena used by `StateStorage` must remain
stable while copied generations can still be checked; dropping a state should
recycle its slot, not free the generation metadata needed for stale-handle
validation.

## Method behavior

Every operation resolves the slot before touching the current `Rc` values:

```rust
impl<S: 'static> StateUpdater<S> {
    pub fn set_state(&self, mutation: impl FnOnce(&mut S) + 'static) {
        let slot = self.slot.expect("StateUpdater is not initialized");
        let slot = slot
            .try_read()
            .unwrap_or_else(|_| panic!("StateUpdater is no longer valid"));

        if slot.failure.is_failed() {
            return;
        }

        slot.tx.push(Box::new(mutation));
        if slot.dirty_source.mark() {
            request_animation_frame();
        }
    }

    pub fn read<R>(&self, read: impl FnOnce(&S) -> R) -> R {
        let slot = self.slot.expect("StateUpdater is not initialized");
        let slot = slot
            .try_read()
            .unwrap_or_else(|_| panic!("StateUpdater is no longer valid"));

        let state = unsafe {
            (&*slot.state.0.get())
                .as_ref()
                .expect("State has already been consumed during reconciliation")
        };
        read(state)
    }
}
```

The production implementation should use Aimer's existing diagnostic and
recovery paths instead of the abbreviated panic messages above.

Add a non-panicking operation for callbacks that may legitimately outlive a
widget:

```rust
pub fn try_read<R>(&self, read: impl FnOnce(&S) -> R) -> Option<R>;
pub fn try_set_state(&self, mutation: impl FnOnce(&mut S) + 'static) -> bool;
```

Keep `read_state() -> &S` only if its lifetime and stale-handle behavior can be
proved safe. Prefer the closure-based `read` or a guard type because a
non-owning `Copy` handle may outlive the owner.

## Stateful element integration

Add a non-copy storage owner field to `StatefulElement`:

```rust
struct StatefulElement {
    state_storage: StateStorage,
    // existing fields
}
```

During initial construction:

1. Create the `StateStorage`.
2. Insert the updater slot into that storage.
3. Pass the copyable updater to `init_state`.
4. Retain the storage in the element.

The type-erased `state_sender` can store an `Rc<dyn Any>` containing the
copyable `StateUpdater<S>`, or an equivalent type-erased slot handle. The
`state_updater<S>()` helper should downcast the handle and return `Some(*handle)`
instead of rebuilding an updater from four cloned `Rc` values.

The existing `state_any` state cell can remain separate during the first
native migration so configuration adoption stays localized. The updater slot
can continue to point at the existing `Rc<SyncState<S>>` and mutation queue.
This keeps the first change small while moving ownership of the public updater
behind the new handle seam.

## Reconciliation and keyed state

Reconciliation must preserve the `StateStorage` that owns the live state:

1. A fresh element creates a fresh owner and fresh candidate state.
2. `adopt_state_from` identifies the old element as the live state source.
3. The new element adopts the old state cell, updater handle, rebuild closure,
   and storage.
4. The fresh candidate storage is dropped after its state is no longer needed.
5. The copied updater stored in the live `S` continues to resolve the old live
   slot.

The owner must also be carried through `LiveKeyedState` and any keyed registry
entry that can recreate an element before the old element is dropped. Do not
retain only the generational handle. A copied handle without the live owner is
allowed to become stale.

Add assertions that after adoption:

- the live updater and the rebuilt element resolve the same slot;
- a candidate updater does not continue targeting the discarded candidate;
- the old storage remains alive until the new live element has adopted it;
- dropping the owning storage invalidates all copied handles.

## Portable backend

The `portable-guest` branch must follow the same public model. It cannot keep
`PortableStateHandle<S>` directly inside `StateUpdater`, because that handle
currently contains `Rc` values.

Use one of these equivalent implementations behind the same Aimer-owned
storage interface:

- store `PortableStateHandle<S>` in `StateStorage` and put only
  `StateUpdaterGeneration<PortableStateHandle<S>>` in the public updater; or
- give the portable live-state registry a `StateStorage`-owned generation and
  make the public updater contain a `StateUpdaterGeneration` plus a safe
  registry lookup.

The first option is preferable because native and portable code then share the
same stale-generation semantics. The portable live-state registry already
owns retained state entries, so its owner must live for exactly the retained
state lifetime and be removed when a stable slot is no longer claimed.

The `StateReadGuard` type introduced for portable reads can remain the public
read representation. Native reads should use the same guard shape if that
reduces unsafe reference exposure.

## Dependency and module seam

Create `StateStorage` and `StateUpdaterGeneration<T>` as private implementation
Modules in `aimer_widget` unless another Aimer crate needs the same storage
seam. Ordinary widget users should only see `StateUpdater<S>` and its
documented methods. If a third-party generational arena is used internally, it
must remain behind these Aimer-owned names.

The `StateUpdaterGeneration<T>` implementation can use a UI-thread generational
slab:

```rust
struct StateUpdaterGeneration<T> {
    index: u32,
    generation: u64,
    _marker: PhantomData<fn() -> T>,
}
```

`StateStorage` must retain values, increment generations on recycle, and
validate every handle before access. The storage backing allocation must remain
valid long enough for stale generations to be checked. A raw pointer without a
generation check and independent `StateStorage` lifetime is not an acceptable
implementation.

## Migration phases

### Phase 1: compile-time contract

- Add the private slot and owner abstractions.
- Implement manual `Copy` and `Clone` for the public updater.
- Add compile assertions proving `StateUpdater<NonCopyState>: Copy`.
- Preserve the existing `set_state`, `read`, and `empty` methods.
- Preserve the UI-thread-only auto-trait behavior.

### Phase 2: native construction and reconciliation

- Create and retain the owner in `StatefulElement`.
- Build updaters from one copyable slot.
- Carry the live owner through keyed and unkeyed state adoption.
- Replace the four-`Rc` downcast path in `state_updater`.
- Remove redundant updater clones from framework construction code where the
  only purpose was ownership transfer.

### Phase 3: stale-handle contract

- Add `try_read` and `try_set_state`, or equivalent result-based methods.
- Decide whether existing panicking methods report uninitialized and dropped
  handles separately.
- Replace or constrain `read_state() -> &S` if its reference can outlive a
  validated slot borrow.
- Add lifecycle tests for callbacks retained after unmount.

### Phase 4: portable backend

- Store the portable backend behind the same copyable generational handle.
- Preserve stable slot restoration and state revision behavior.
- Test first-generation creation, retained generations, duplicate slots, and
  slot removal.

### Phase 5: user migration and cleanup

- Update examples and built-in widgets to use direct updater copies.
- Keep `.clone()` source-compatible because `Clone` remains implemented, but
  remove unnecessary framework-internal clones.
- Document that `Copy` does not extend widget lifetime and that stale handles
  must not be used for new work after unmount.
- Measure allocation and frame costs before and after the change.

## Verification plan

### Compile-time tests

```rust
fn assert_copy<T: Copy>() {}

assert_copy::<StateUpdater<NonCopyState>>();
```

Also retain compile-fail checks proving that `StateUpdater<S>` is not `Send`
or `Sync`.

### Runtime tests

- Two callbacks copied from one updater mutate the same state.
- Copying an updater does not clone or move the state value.
- A non-`Copy` state type works without additional bounds.
- Multiple queued updates preserve FIFO order and existing coalescing rules.
- An empty updater still reports the existing initialization diagnostic.
- A stale updater after unmount cannot access freed state.
- A stale generation cannot access a newly reused slot.
- Reconciliation keeps the old live updater target.
- Candidate state and candidate updater are discarded together.
- Keyed reorder preserves the correct updater target.
- Portable state handles preserve the same behavior.

### Performance checks

Compare a representative callback-heavy widget before and after migration:

- updater construction and clone count;
- allocations during build and reconciliation;
- state update queue throughput;
- frame invalidation count;
- native and portable binary size where relevant.

The expected user-facing improvement is zero explicit updater clones at call
sites. The implementation should not claim a frame-time improvement unless
the measurements show one.

## Risks and decisions to resolve before implementation

- A true `Copy` handle is non-owning. This changes the current behavior where
  an updater clone containing `Rc` can keep the state cell alive after the
  element is gone. The stale-handle contract must be explicit.
- The existing unchecked native `read_state() -> &S` API may need a guard or
  `try_` variant to remain sound after the ownership split.
- Reconciliation currently relies on several type-erased `Rc` fields. The
  owner and slot must be transferred in every path, including keyed registry
  reuse and portable generation restoration.
- The owner should not be leaked for ordinary component state. Application-wide
  state may have a separate explicit global-owner API if needed.
- Do not implement `Copy` by storing an `Rc` as a raw pointer. That loses RAII
  reference accounting and would make cleanup unsound.

## Acceptance criteria

The plan is complete when:

- `StateUpdater<NonCopyState>` implements `Copy` and `Clone` without an
  `S: Copy` bound;
- users can move one updater variable into multiple `move` callbacks without
  writing `.clone()`;
- all callbacks address the same live state slot;
- state is released when its owning element is removed;
- stale copied handles fail safely and diagnostically;
- keyed/unkeyed reconciliation preserves the live owner and state;
- native and portable backends pass focused lifecycle and mutation tests;
- the existing UI-thread-only contract remains intact.
