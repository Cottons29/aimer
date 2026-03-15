# State Management

Managing application state is crucial for building interactive user interfaces. Aimer borrows the `StatefulWidget` / `State` pattern directly from Flutter.

In Aimer, widgets come in two main flavors:
- **Stateless Widgets**: Built once based on their initial configuration and parameters.
- **Stateful Widgets**: Maintain internal data that might change over the lifetime of the widget. When the internal state changes, the widget triggers a rebuild.

Quick rule of thumb:
- Choose `StatelessWidget` for static/pure UI composition.
- Choose `StatefulWidget` + `State` + `StateUpdater` for interactive UI that changes over time.

---

## 3.1 StatelessWidget

A `StatelessWidget` is a widget that does not require mutable state. It simply takes the properties provided to it and builds a widget tree based on those properties. It is ideal for reusable UI components that don't need to change dynamically.

```rust
use aimer::{Widget, BuildContext, StatelessWidget};

pub struct GreetingCard {
    pub message: String,
}

impl StatelessWidget for GreetingCard {
    fn build(&self, _ctx: &BuildContext) -> Box<dyn Widget> {
        Container!(
            padding: LayoutSpacing::all(Spacing::Px(16)),
            child: Text!(
                self.message.clone(),
                text_style: TextStyle!(
                    font_size: 18.0,
                    color: Colors::Black
                )
            )
        )
    }
}
```

---

## 3.2 StatefulWidget and State

Use `StatefulWidget` when the widget needs internal state that can change over time (e.g., in response to user input or network events). 

To create a stateful widget, you generally define two structs:
1. **The Widget (StatefulWidget)**: Holds the initial configuration and creates the state.
2. **The State**: Holds the mutable data and the `build` method.

### Example: Counter App

Here's an overview of how state is managed using a `StatefulWidget`:

```rust
use aimer::{State, StatefulWidget, StateUpdater, Widget};

// 1. Define the Widget
#[derive(Clone)]
pub struct CounterWidget;

impl StatefulWidget for CounterWidget {
    type State = CounterState;

    fn create_state(&self) -> Self::State {
        CounterState { count: 0 }
    }
}

// 2. Define the State
pub struct CounterState {
    count: i32,
}

impl State for CounterState {
    type Widget = CounterWidget;

    fn build(&self, updater: StateUpdater<Self>) -> Box<dyn Widget> {
        let current_count = self.count;
        
        Column!(
            children: vec![
                Text!(format!("Count: {}", current_count)),
                Button!(
                    child: Text!("Increment"),
                    on_press: move || {
                        // Use the updater to mutate state and schedule a rebuild
                        updater.update(|state| {
                            state.count += 1;
                        });
                    }
                )
            ]
        )
    }
}
```

---

## 3.3 The StateUpdater

The `StateUpdater` is the heart of reactive rebuilds. In the `build` method of your `State`, you receive an `updater: StateUpdater<Self>`. It allows you to:
1. Mutate the inner state safely using `updater.update(|state| { ... })`.
2. Automatically signal to the framework that the `Element` tree for this specific widget is dirty and requires a re-render.

This targeted approach ensures performance remains high by only recalculating the parts of the widget tree that have actually changed, rather than rebuilding the entire application UI.
