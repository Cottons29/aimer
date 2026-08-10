# Widgets Now Give Their Fields Away

Aimer's central conversion has changed shape. `Widget::to_element` used to borrow the widget:

```rust
fn to_element(&self, ctx: &BuildContext) -> AnyElement;
```

It now **consumes** it:

```rust
fn to_element(self, ctx: &BuildContext) -> AnyElement;
```

One `&` disappeared, and with it a per-frame clone in roughly one hundred and seventy widget
implementations, the implicit `Clone` requirement on every derived widget, and the whole-subtree
rebuild that used to happen every time a button noticed the mouse.

### The Premise Was Backwards

The question that started this work was "can an element borrow from its widget, since the element is
short-lived?" It cannot — because the lifetimes are the other way around.

A widget is a **description**. It is created inside a `build()` call and dropped a few lines later.
An element is the **retained** side: it lives in the persistent element tree and survives frames,
layout, paint, event dispatch, and reconciliation. An element holding `&BoxDecoration` would need
that borrow to outlive the entire tree, while the widget it borrowed from is already gone.

But the correct conclusion is stronger than a borrow. Because the widget is about to die, *nothing
else needs its data*. Copying fields out of it was pure waste:

```rust
// before — clone, then drop the original one line later
fn to_element(&self, ctx: &BuildContext) -> AnyElement {
    RawContainer { box_decoration: self.box_decoration.clone(), .. }.boxed()
}

// after — the short-lived description hands its guts to the retained element
fn to_element(self, ctx: &BuildContext) -> AnyElement {
    RawContainer { box_decoration: self.box_decoration, .. }.boxed()
}
```

`BoxDecoration` owns a `Vec<BoxShadow>`, so the old line was a heap allocation per decorated
container per build, discarded a frame later. The new line is a move.

### The Obstacle: `self` Does Not Fit in a Vtable

A consuming method is not object safe, and Aimer's erased widget handle is exactly a trait object:
`AnyWidget` is a `Rubick` owner over `dyn Widget`. So the trait was split in two.

Widget authors write the safe, consuming trait. Behind `#[doc(hidden)]` sits an object-safe shim that
is blanket-implemented and never written by hand:

```rust
pub trait Widget {
    fn to_element(self, ctx: &BuildContext) -> AnyElement where Self: Sized;
    fn key(&self) -> Option<Key> { None }
    fn debug_name(&self) -> &'static str { "Unknown" }
}

#[doc(hidden)]
pub trait DynWidget: 'static {
    /// # Safety
    /// Leaves the pointee uninitialized; only the erased handle may call it,
    /// and only after the storage was marked vacant.
    unsafe fn to_element_in_place(&mut self, ctx: &BuildContext) -> AnyElement;
    // ..
}
```

`AnyWidget` is now `Rubick<dyn DynWidget, 8>`, and building it moves the payload out of its storage
rather than reading through a reference.

### Moving Out of Rubick Without Leaking It

The obvious implementation — `ptr::read` the payload and `mem::forget` the owner — is memory-safe and
wrong. Rubick's destructor does two things: it drops the value, and, when the payload did not fit
inline, it returns the block to the crate's pooled free list. `ptr::read` handles only the first, so
`forget` would leak the block of every heap-sized widget, on every node, on every frame.

The sound version lives inside `aimer_rubick`, where the pool and the operation table are visible:

```rust
pub unsafe fn take<R>(mut self, consume: impl FnOnce(*mut T) -> R) -> R {
    debug_assert!(self.is_direct(), "a projected owner has no movable target");

    let operations = self.operations;
    let data = self.data_mut();
    // From here the owner owns storage, not a value: an unwind out of
    // `consume` can no longer run the destructor a second time.
    self.operations = Operations::VACANT_REF;
    // .. read the value, then release the pooled block on both paths
}
```

Order carries the safety argument. Installing the vacant operation table *before* the move closes the
double-drop window, and the block is released through a guard so a panicking `to_element` — which
Aimer's recovery path deliberately catches — cannot leak it either. One `debug_assert` rules out the
one construction `ptr::read` would silently corrupt: a projected owner, whose adapters would be
abandoned along with the value.

The whole design was prototyped in an isolated crate, `aimer_laboratory`, before production saw it:
a widget's `String` buffer arriving at the element at the same address, a pooled block coming back for
the next build, exactly one destruction on the panic path, and zero allocations on a warm tree.

### The Hard Case: A Widget That Rebuilds Itself

A consumed widget serves exactly one build. That is fine for a container — its *parent* rebuilds it,
so it receives a fresh description every time. It is not fine for a widget that rebuilds **itself**:
a button on hover, a viewport on a new scroll offset, a theme on every tick of its transition. Its
child widget was eaten by the first build and cannot answer a second one.

Reproducing the child was not available. Cloning it needs a `Clone` bound the tree does not have —
`Row` and `Column` hold erased children that are not `Clone`. Asking the caller for a factory would
turn `child(widget)` into `child(|| widget)` at every call site in every example, page, and book
chapter.

So the child is *retained* instead of reproduced. `ChildBuilder` became a child slot: the first build
consumes the widget and keeps the element it produced; every later build hands the tree a thin proxy
over that same element, forwarding layout, paint, events, identity, and diagnostics to it.

```rust
enum Source {
    Required,                          // no child attached yet
    Once(Rc<Retained>),                // .child(widget): build once, retain
    Build(Rc<dyn Fn() -> AnyWidget>),  // closure form: fresh widget per build
}
```

This is strictly cheaper than what it replaced. The borrowing conversion re-ran the entire subtree on
every hover; the retained slot does not rebuild it at all, so the child's own state, its scroll
offsets, and its GPU resources survive its parent's rebuilds by construction rather than by
reconciliation. The one guard needed elsewhere was a same-pointer short-circuit in the state-carrying
walk, since the retained child is literally the same element on both sides of a rebuild.

`.child(some_widget)` keeps working exactly as before. Not one example, page, or book snippet changed
shape.

### Widgets No Longer Have To Be `Clone`

The second clone class was structural. A stateless element must be able to re-run `build()` when it is
marked dirty — a resize, a media query change — so it has to own its configuration. It used to own a
**copy**, produced by the derive macro:

```rust
let __rebuild_source = ::std::clone::Clone::clone(self);
```

With a consuming signature the element owns **the original**, and the line is simply deleted. As a
side effect, `#[derive(StatelessWidget)]` no longer imposes an invisible `Clone` requirement on user
widgets. `StatefulWidget::create_state` became consuming in the same pass, so a state moves its props
in instead of copying them.

### What This Cost and What It Bought

The trait cannot compile halfway, so the sweep was atomic: the core split, the three codegen
templates, every widget crate, the app layer, the examples, the website, the integration tests, and
the book. `impl Widget for Box<dyn Widget>` was deleted — it cannot host a consuming method — and
child lists in flex, grid, stack, and markdown moved from `iter().map(..)` to `into_iter()`.

The claims are asserted, not assumed. A counting global allocator proves a decorated container's
conversion now costs exactly what *describing* it costs, and the test was checked for vacuity by
putting the clone back: it fails, two allocations against one. Drop recorders prove exactly-once
destruction through the erased path, including on an unwind. The five self-rebuilding widgets each
carry a test that their child element is reused rather than rebuilt.

`cargo test --workspace`: **2320 passed, 0 failed**.

One honest limitation. A re-placed retained child is still marked as needing a rebuild, because a
parent rebuild is exactly when the values it publishes change — a theme mid-transition, a provider's
new value. The element is reused, so identity and state survive, but the build closures inside it do
re-run. Making that conditional on the published value actually changing is the next step, and it is
the remaining half of "a hover costs no subtree rebuild".

### If You Write Widgets

One rule replaces the old habit:

> Move your fields in `to_element`, never clone them.

The widget is dropped immediately after the call, so a `.clone()` there is a per-frame allocation
nobody ever reads. And if your widget rebuilds itself, keep its child in a `ChildBuilder` rather than
asking for the widget twice.
