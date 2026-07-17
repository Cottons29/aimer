# State Management

Aimer supports local widget state and provider-owned state. Both are confined to the UI thread.

- Use `StatefulWidget`, `State`, and `StateUpdater` for state owned by one widget.
- Use `Provider` or `NotifierProvider` for state shared by a subtree.
- Use `StoreProvider` when mutations should be expressed as typed actions.

## Local widget state

A state object receives its updater once in `init_state`. Calling `set_state` queues mutations in order and schedules one redraw.

```rust
use aimer::{BuildContext, State, StateUpdater, StatefulWidget};

struct Counter;

struct CounterState {
    count: usize,
    updater: StateUpdater<Self>,
}

impl StatefulWidget for Counter {
    type State = CounterState;

    fn create_state(&self) -> Self::State {
        CounterState { count: 0, updater: StateUpdater::empty() }
    }
}

impl State<Counter> for CounterState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, _context: &BuildContext) -> impl aimer::Widget {
        aimer::Text::new(format!("Count: {}", self.count))
    }
}
```

Store the updater in an event callback and call `updater.set_state(|state| state.count += 1)` to rebuild the widget.

## Scoped providers

Providers own their value for the lifetime of their element. Lookup resolves the nearest ancestor of the requested type, so a nested provider can override an outer provider without affecting siblings. The child is always configured last.

```rust
use aimer::{NotifierProvider, Text};

#[derive(Clone, Default)]
struct CounterState {
    count: usize,
}

let app = NotifierProvider::<CounterState>::new()
    .create(CounterState::default)
    .child(Text::new("Application"));
```

Import `ProviderContext` to use provider methods on `BuildContext`:

```rust
use aimer::{BuildContext, ProviderContext};

fn build(context: &BuildContext) {
    let snapshot = context.read::<CounterState>();
    let watched = context.watch::<CounterState>();
    let count = context.select::<CounterState, usize>(|state| state.count);
}
```

- `read` returns a cloned snapshot and does not subscribe.
- `watch` returns a cloned snapshot and rebuilds the current widget after any update.
- `select` rebuilds only when its projected `PartialEq` value changes.
- `try_read` and `try_watch` return `None` when no matching provider exists.
- Required lookup methods panic immediately with the missing Rust type in the diagnostic.

Old dependencies are removed before every rebuild, so conditional watches stop receiving updates when their branch is no longer built.

## Updating outside build

Event callbacks cannot retain a borrowed `BuildContext`. Capture a UI-local `ProviderHandle` during build instead:

```rust
use aimer::{BuildContext, ProviderHandle};

fn build(context: &BuildContext) {
    let counter = ProviderHandle::<CounterState>::of(context);
    let on_press = move || {
        counter.update(|state| state.count += 1);
    };
}
```

`ProviderHandle::read` returns a borrow guard instead of cloning. Do not hold that guard while calling `update`.

## Reducer stores

`StoreProvider` uses the same subscription runtime, but publishes a dispatcher for its action type.

```rust
use aimer::{ProviderContext, StoreProvider, Text};

#[derive(Clone, Default)]
struct AppState {
    signed_in: bool,
}

enum AppAction {
    SignedIn,
    SignedOut,
}

let app = StoreProvider::<AppState, AppAction>::new()
    .create(AppState::default)
    .reducer(|state, action| match action {
        AppAction::SignedIn => state.signed_in = true,
        AppAction::SignedOut => state.signed_in = false,
    })
    .child(Text::new("Application"));

// During build or another operation with a scoped context:
// context.dispatch(AppAction::SignedOut);
```

Actions are reduced synchronously on the UI thread, then ordinary watchers and selectors are notified. Worker threads must send results back to the UI event loop before updating or dispatching provider state; provider handles intentionally do not implement cross-thread sharing.
