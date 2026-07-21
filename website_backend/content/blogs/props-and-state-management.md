# Props and State Management in Aimer

Declarative interfaces become easier to reason about when configuration and mutable state have clear owners. Aimer
separates the two: widgets carry immutable props that describe the current interface, while mounted state stores values
that must survive rebuilds, such as counters, selections, hover state, and animation progress.

This post introduces stateless and stateful widgets, follows an update from an event callback through a rebuild, and
explains when to keep state local, lift it to a parent, or share it through a provider.

## Props Are Widget Configuration

Props are ordinary Rust fields. They are supplied when a widget is constructed and read while its child tree is built:

```rust
use aimer::{BuildContext, Text, Widget, StatelessWidget, widget};

#[derive(Clone)]
#[widget(Stateless)]
struct Greeting {
    name: String,
}

impl StatelessWidget for Greeting {
    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        Text::new(format!("Hello, {}!", self.name))
    }
}
```

`Greeting` owns no runtime state. Rebuilding it with another `name` creates a new description, and Aimer reconciles that
description with the existing element tree.

Builder methods are another way to express props. They consume and return the widget, making partially configured
values easy to compose while preserving concrete Rust types:

```rust
#[derive(Clone)]
struct Badge {
    label: String,
    emphasized: bool,
}

impl Badge {
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            emphasized: false,
        }
    }

    fn emphasized(mut self, emphasized: bool) -> Self {
        self.emphasized = emphasized;
        self
    }
}
```

The parent remains the owner of the values it passes down. A child should treat those values as a snapshot of the
parent's current configuration rather than trying to mutate the parent directly.

## Use a Stateless Widget for Pure Presentation

A stateless widget needs only one method:

```rust
pub trait StatelessWidget {
    fn build(&self, ctx: &BuildContext) -> impl Widget;
}
```

Use `#[widget(Stateless)]` when the output depends entirely on props and values available through `BuildContext`. The
widget must be `Clone`, because Aimer retains its configuration for later rebuilds.

Stateless does not mean static. A stateless child can receive a different value every time its parent rebuilds. It can
also receive callbacks that ask the owner to perform an action:

```rust
#[derive(Clone)]
#[widget(Stateless)]
struct CounterLabel {
    count: i32,
}

impl StatelessWidget for CounterLabel {
    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        Text::new(format!("Count: {}", self.count))
    }
}
```

Keeping presentational components stateless makes their behavior explicit: the same props produce the same widget
tree.

## Add Local State When a Value Must Survive Rebuilds

A stateful widget separates fresh configuration from mounted runtime state. `StatefulWidget::create_state` creates that
state when the widget is mounted:

```rust
use aimer::{BuildContext, Button, State, StateUpdater, StatefulWidget, Text, Widget, widget};

#[derive(Clone)]
#[widget(Stateful)]
struct Counter {
    initial_count: i32,
}

struct CounterState {
    initial_count: i32,
    count: i32,
    updater: StateUpdater<Self>,
}

impl StatefulWidget for Counter {
    type State = CounterState;

    fn create_state(&self) -> Self::State {
        CounterState {
            initial_count: self.initial_count,
            count: self.initial_count,
            updater: StateUpdater::new(),
        }
    }
}
```

The widget is temporary configuration; `CounterState` is the object Aimer preserves while compatible widget
descriptions are reconciled. This is why the current count belongs in state rather than in a newly constructed widget.

## Understand the State Lifecycle

The `State` implementation has three important lifecycle methods:

```rust
impl State<Counter> for CounterState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.initial_count = new.initial_count;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        Button::new()
            .on_press({
                let updater = self.updater.clone();
                move || updater.set_state(|state| state.count += 1)
            })
            .child(Text::new(format!("Count: {}", self.count)))
    }
}
```

Each method has a distinct responsibility:

- `create_state` supplies initial props and runtime values for a newly mounted widget.
- `init_state` receives the initialized `StateUpdater` used by event callbacks.
- `adopt_config_from` copies refreshed parent-provided props during reconciliation.
- `build` describes the child tree from the current state.

The distinction inside `adopt_config_from` is important. This example refreshes `initial_count`, because it came from
the parent, but deliberately preserves `count`, because it is live runtime state. Copying every field from `new` would
reset user interaction on each parent rebuild. Copying nothing can leave callbacks or props stale.

## Update State Through StateUpdater

`StateUpdater::set_state` queues a mutation and schedules a rebuild:

```rust
let updater = self.updater.clone();

move || {
    updater.set_state(|state| {
        state.count += 1;
    });
}
```

The mutation is applied on the render thread before the next build. Several updates queued before that frame are
coalesced into one redraw request. Because updates are queued, code should not assume that a call to `set_state` has
already changed a later synchronous read.

When a callback needs to capture a borrowed value, `set_state_with` clones it into the queued closure:

```rust
updater.set_state_with(&next_label, |state, label| {
    state.label = label;
});
```

Store the updater received by `init_state`; calling an uninitialized `StateUpdater::new()` directly will panic. Use
`read(...)` for a synchronous view of the currently applied state, but remember that reading does not subscribe another
widget or schedule a rebuild.

### A Helpful Error for Uninitialized State

Reading state before Aimer has initialized its updater is a lifecycle error. For example, this standalone updater is
only a placeholder and is not connected to mounted state:

```rust
let updater: StateUpdater<CounterState> = StateUpdater::new();
let count = updater.read(|state| state.count);
```

Instead of failing with only a generic panic, Aimer prints a diagnostic inspired by the Rust compiler. It points to the
read or update that caused the problem, shows the missing lifecycle code, and ends with an actionable hint:

```text
State is not initialized and trying to read or update at src/counter.rs:42
   |
   | impl State<YourStatefulWidget> for YourWidgetState {
   |
   |     fn init_state(&mut self, _updater: StateUpdater<Self>)
   |         where
   |             Self: Sized,
   |         {
   |             self.updater = _updater;
   |             ^^^^^^^^^^^^^^^^^^^^^^^^^ add this line to prevent panic
   |         }
   |
   help: call `self.updater = _updater` inside `init_state`
```

The fix is to keep `StateUpdater::new()` only as the initial field value and replace it with the mounted updater during
`init_state`:

```rust
fn init_state(&mut self, updater: StateUpdater<Self>) {
    self.updater = updater;
}
```

After `init_state` runs, `read`, `read_state`, and `set_state` are connected to the mounted state. Before then, all three
report the same diagnostic because silently returning a default value would hide a real lifecycle bug.

## Lift State for Coordinated Widgets

Keep state at the nearest common owner of every widget that reads or changes it. A parent can pass the current value
down as a prop and pass a callback or cloned updater down for actions:

```rust
let count = self.count;
let updater = self.updater.clone();

Column::new().children(vec![
    CounterLabel { count }.boxed(),
    Button::new()
        .on_press(move || updater.set_state(|state| state.count += 1))
        .child(Text::new("Increment"))
        .boxed(),
])
```

The data flow stays predictable:

1. The parent owns the mutable value.
2. The parent passes the value down as props.
3. A child invokes an action callback.
4. The parent updates and rebuilds.
5. Children receive fresh props.

The Aimer website follows this pattern for its theme switcher. The application shell owns the current theme mode, then
passes its updater through the shell frame to the header. Pressing the toggle updates the shell, which rebuilds the
entire themed subtree with one new value.

## Preserve State With Stable Identity

During reconciliation, unkeyed children are normally matched by position. Keyed children are matched by their key and
widget identity. When Aimer finds a match, it preserves the mounted state and adopts the new configuration.

Use stable, unique keys when siblings can move, appear, or disappear:

```rust
items
    .iter()
    .map(|item| ItemRow {
        key: Some(item.id.to_string().into()),
        item: item.clone(),
    })
```

Changing a key intentionally resets that widget's state. Creating a fresh unique key on every build also resets state,
so it is the wrong choice when identity should survive. For a fixed call site, `key!()` creates a stable static key.
Stateful widgets can expose an optional `key: Option<Key>` field, while framework widgets may provide their own key
builder.

Keys do not replace good ownership. They tell reconciliation which mounted state belongs to which widget; they do not
make unrelated widgets share data.

## Share Broad State With Provider

Passing props and callbacks is clearest for nearby components. When the same value is needed across distant branches,
Aimer's optional provider feature can expose it to a subtree:

```rust
Provider::<AppStore>::new()
    .create(AppStore::default)
    .child(app)
```

Descendants can choose how they depend on the value:

- `ctx.read::<T>()` reads a snapshot without subscribing.
- `ctx.watch::<T>()` rebuilds when the provider changes.
- `ctx.select::<T, R>(...)` rebuilds only when a selected value changes.
- `ctx.update::<T>(...)` mutates the nearest matching provider.

Use `watch` and `select` during a widget build. Their non-`try_` forms expect a matching ancestor and panic when none is
present. Provider updates use copy-on-write snapshots, so shared values should be designed to clone cleanly.

Provider is not a replacement for props. Props keep dependencies visible and components reusable; provider is most
useful for application-wide or route-wide state such as a session, store, settings, or cached data.

> The **Aimer** Website is using Provider for manage the blogs on client side.

### A Helpful Error for a Missing Provider

Provider access is type-safe, but the requested type must exist above the consumer in the current widget scope. This
build method asks for `AppStore` without installing a matching provider:

```rust
fn build(&self, ctx: &BuildContext) -> impl Widget {
    let store = ctx.watch::<AppStore>();
    Text::new(format!("Items: {}", store.items.len()))
}
```

Rather than returning unrelated state or a default value, Aimer reports the missing concrete Rust type rather than printing the stacktrace:

```text
No provider for `jaime::panic_recovery::MissingProviderValue` found in the current widget scope

jaime/src/panic_recovery.rs:22:24
        let store = ctx.watch::<AppStore>();
                    ^^^^^^^^^^^^^^^^^^^^^^^^
```

> User can still enable the stacktrace by adding env variable `RUST_BACKTRACE= 0 | 1 | full`

The fix is structural: place a provider for the same type above every descendant that reads, watches, selects, or
updates it:

```rust
Provider::<AppStore>::new()
    .create(AppStore::default)
    .child(App::new())
```

`ctx.read::<AppStore>()`, `ctx.watch::<AppStore>()`, `ctx.select::<AppStore, _>(...)`, and
`ctx.update::<AppStore>(...)` all provide the same missing-provider guidance. When absence is valid rather than a
programming error, use `ctx.try_read::<AppStore>()` or `ctx.try_watch::<AppStore>()` and handle the returned `Option`.
Also keep subscriptions inside `build`: calling `watch` or `select` outside a widget build reports that lifecycle
mistake directly instead of creating a subscription that can never rebuild its consumer.

## Choose the Smallest State Tool

A practical decision order is:

1. Use props when a parent already owns the value.
2. Use a stateless widget when rendering depends only on props and inherited context.
3. Use local state for interaction that belongs to one mounted component.
4. Lift state when several nearby widgets must stay coordinated.
5. Use Provider when many distant descendants depend on the same data.

Aimer keeps these choices close to normal Rust. Props are typed fields, state transitions are closures over concrete
state, and keys make identity explicit. With configuration flowing down and actions flowing back to the owner, even a
large widget tree remains understandable as it rebuilds.