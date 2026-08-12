# Venus: An Async Runtime That Knows What A Frame Is

Aimer has a new root-level crate, `aimer_venus`, sitting beside `aimer_cupid`, `aimer_rubick` and
`aimer_quiver`. It is a runtime, but not the kind you are thinking of. It does not open sockets, it
does not steal work between threads, and it is not a Tokio replacement.

It owns the one thing a general-purpose runtime structurally cannot express:

> *when*, relative to a frame, asynchronous work runs.

### The Question That Started It

It began with a much smaller one: should `Callback` be async by default?

The answer was no, and the reason turned out to be interesting. Aimer's callback body was one of two
flavours:

```rust
pub enum RawInnerCallback<P, R> {
    Sync(Rc<dyn Fn(P) -> R>),
    Async(Rc<AsyncBody<P>>),
}

type AsyncBody<P> = dyn Fn(P) -> Pin<Box<dyn Future<Output = ()> + Send>>;
```

Look at the `Send` on that future. The async flavour required `F: Send, Fut: Send`, because whatever
drove it was a thread pool. But Aimer's element tree is `Rc` and `RefCell` all the way down. So an
async handler literally could not capture a `StateUpdater`, a controller, or an element handle — which
is what roughly 95 % of real handlers capture. The bound was not a design preference; it was physics
imposed by *where the future gets polled*.

Then there is the second half. Even when a handler compiled, its effect landed on a pool, resolved at
some unspecified later moment, and the `set_state` it performed was seen by the *next* frame. That is a
one-frame input lag, and no amount of tuning removes it, because "as soon as possible" is not a
schedule a user interface can be built on.

### Five Workarounds For One Missing Piece

Once you go looking, the absence is visible everywhere in the codebase:

- `aimer_quiver` built a full multi-threaded `Runtime::new()` for every application, including
  headless ones, so that most apps could run zero sockets on it.
- `callback.rs` threaded an `AsyncSpawner` — an `Option<Handle>` — down the entire element tree,
  `#[cfg]`-forked for wasm, and *still* ended in
  `warn("an async callback was discarded: no runtime handle was given")`. That parameter existed only
  because the runtime was external to the framework.
- `AsyncBuilder` shipped its results through a private channel, drained it in `poll_completion` once
  per frame, and kept a `generation` counter to throw away stale replies. That is a hand-rolled
  UI-thread scheduler — written once, for one widget.
- `aimer_utils::block_on` is a park/unpark executor we already owned.

Five independent compensations for the same gap. That is usually the signal that the gap is
architectural.

### Why Not Put It On Another Thread?

The first instinct was to run the UI runtime on its own thread and talk to it over a channel. That
design defeats itself on every axis:

- If tasks *run* off-thread, `Send` is mandatory — you would have written a new runtime and kept the
  single worst constraint of the old one.
- A channel cannot express "before this frame's build phase". It gives you "some time later, drained
  whenever the UI thread next looks", which is the original latency bug wearing a new hat.
- Most `await`s in a UI are not I/O. They are an animation tick, a focus change, a controller notify,
  a value that is already ready. Round-tripping those through `send` + wake + `try_recv` costs more
  than the work itself.
- The mutation has to be applied on the UI thread regardless, because the tree is `Rc`. So the first
  hop buys nothing and the second one is mandatory.

The rule that fell out:

> **Threads for work, the UI thread for scheduling.**

### What Venus Actually Is

A single-threaded, non-`Send`, frame-phase scheduler driven by the event loop, plus one small worker
pool for work that genuinely has to leave.

| Concern      | Server runtime     | Venus                              |
|--------------|--------------------|------------------------------------|
| Unit of time | none, run ASAP     | the frame (16.6 / 8.3 ms)          |
| Thread model | work-stealing pool | one UI thread + optional offload   |
| `Send` bound | required           | only at the `offload` boundary     |
| Wake → run   | µs, unordered      | before *this* frame's build phase  |
| Backpressure | queue depth        | frame budget, deadline-aware       |
| Cancellation | rare               | constant, tied to the element tree |

Internally: a generational task slab, one ready queue *per phase*, and an id-only cross-thread wake
queue with a per-task cached `Waker`. A task pays one `Box::pin` at spawn and nothing per poll —
polling a ready task is a `VecDeque::pop_front`, a slab index and one `Future::poll`.

Futures are lent out during a poll rather than held borrowed, so a task may spawn, abort, or even
drop its own scope re-entrantly without tripping a `RefCell`.

### Three Phases, In Order

```text
input ──▶ frame tasks ──▶ microtasks ──▶ build / layout / paint ──▶ idle ──▶ present
              │                │                                      │
       animation ticks   drained to        the caller's work    budget-gated,
       once per frame    exhaustion                             resumable
```

| Phase       | Runs                       | For                                     |
|-------------|----------------------------|-----------------------------------------|
| `Microtask` | before the build, to empty | `set_state`, focus change, notify       |
| `Frame`     | once per frame             | animation ticks, after-layout callbacks |
| `Idle`      | while the frame has room   | image decode, glyph raster, prefetch    |

Microtasks are deliberately **unbudgeted**. They must land before this frame's build or the one-frame
latency comes straight back, so they cannot be budget-cut; instead they are cheap by contract — a
microtask may mutate state, it may not do work. Debug builds assert once a single drain passes
100 000 polls, so an accidentally self-rewaking microtask is caught by its author, not by a user.

Idle work is the opposite: it runs only while the clock says there is room.

```rust
venus.spawn_idle(async {
    for _tile in 0..4096 {
        // decode_one_tile();
        yield_if_over_budget().await;
    }
});
```

`FrameBudget` is deadline-based with a safety margin, because present, compositor handoff and OS
jitter eat time you do not control — at 120 Hz you target about 7 ms of usable frame, not 8.3. And a
`FrameGovernor` watches the *previous* frame: if frame N−1 overran, frame N spends zero idle budget.
One frame of history is a far more stable governor than trying to predict what a task will cost.

### The `Rc` Test

The property the whole crate exists for was written as a failing test before any of it existed:

```rust
let source = Rc::new(Cell::new(0));
let observed = Rc::new(Cell::new(-1));
let mut app = AimerApp::start_headless(ObservingWidget { .. });
app.render_frame();
assert_eq!(observed.get(), 0);

let mutated = source.clone();
app.venus().spawn(async move {
    aimer_venus::yield_now().await;
    mutated.set(7);
});

app.render_frame();
assert_eq!(observed.get(), 7); // this frame, not the next one
```

Two `Rc`s captured by an `await`ing task, and the effect visible to the very next build. A
cross-thread design cannot pass that test without a blocking join. That one assertion settled the
architecture.

### Wiring It Into The Loop

A scheduler nobody drives is a library, so `aimer_quiver` now owns an `Rc<Venus>` and both drivers —
the windowed `winit` loop and `HeadlessAimerApp` — go through the same two hooks, so a frame costs
identical work whichever asked for it.

`begin_frame` starts the budget, dispatches this frame's input, then runs the frame tasks and drains
the microtasks — immediately before the tree is built. Animation ticks go first so the values the
tree is built from are *this* frame's.

`end_frame` spends the measured slack on idle work, closes the frame for the governor, and asks for
another frame while `has_ready_work()` is true — which is what keeps a sliced task moving without a
timer.

The runtime is also *installed* for the drawing thread at both construction sites. That is the piece
that deletes the `AsyncSpawner` plumbing: a handler eleven elements deep reaches the runtime through
`Venus::current` or `spawn_local`, with nothing handed down to it.

```rust
let ran = Rc::new(Cell::new(false));
let flag = ran.clone();
let spawned = aimer_venus::spawn_local(async move { flag.set(true) });
assert!(spawned.is_some());
app.render_frame();
assert!(ran.get());
```

A worker finishing while the loop is parked needs to nudge it awake. That is a wake-up primitive, not
a task channel: on native it goes through the same coalesced `FrameReady` user event the widget tree
already uses to request a redraw, and on wasm it is a no-op because the browser's microtask queue
*is* the microtask queue. `set_notifier` fires only for wakes raised off the UI thread — a wake raised
on it never pings, because the loop is demonstrably awake.

### Offload Is The Only Place `Send` Appears

An 8 ms budget makes offload non-optional. A 40 ms PNG decode or JSON parse cannot cooperate; it has
to leave. But the API deliberately does not hand the widget a channel to poll:

```rust
venus.spawn(async move {
    let bytes = runtime.offload(|| std::fs::read(path)).await;
    // Back on the UI thread, still holding the `Rc`.
    mutated.set(bytes.len());
});
```

The channel is an implementation detail inside `offload`. The widget sees a future. That is what made
`AsyncBuilder`'s private receiver and `generation` counter deletable: cancellation is now "drop the
task's scope", and a stale reply is simply never produced.

Scopes are the other half of that. An element owns a scope, and unmounting cancels its tasks:

```rust
let scope = venus.scope();
venus.spawn_in(scope.id(), async { /* fetch, decode, … */ });
drop(scope);
assert_eq!(venus.task_count(), 0);
```

`AsyncRuntime`'s per-widget `Drop { abort_handle.abort() }` and `AsyncBuilder`'s generation counter
collapsed into this one shared mechanism.

### Costs, Honestly

A cooperative scheduler cannot preempt. An `async` handler that does 12 ms of straight-line work with
no `await` in the middle *will* drop a frame, and nothing in Venus can stop it. The defences are
ergonomic rather than enforced: `yield_if_over_budget().await`, `offload` as the escape hatch, and a
debug build that prints the duration of any poll that runs long, so the author hears about it before a
user does.

Venus also carries a dev-profile exception, `[profile.dev.package.aimer_venus] opt-level = 3`, for the
same reason `aimer_rubick` does. At `opt-level = 0` a microtask drain cost about 770 ns per task —
which is an unremarkable debug number for code whose entire body is a `Cell::set`, since the
measurement is then 100 % framework overhead and that is exactly the part optimisation deletes. A
framework whose scheduler eats 7.7 ms of an 8.3 ms frame *in debug* would be undebuggable.

And this is additive, not a removal. Tokio still runs `reqwest` in `aimer_assets` and the inspector
server. The pitch is narrower than "we wrote a runtime":

> Aimer owns UI-thread scheduling. Bring your own I/O runtime.

### The Framework Moved Onto It

The two workarounds this began with are gone.

**Callbacks.** `AsyncBody` lost its `Send` bound, so an async handler may capture a `StateUpdater`
and hold it across an `await` — the thing that did not compile before. `AsyncSpawner`, the
`Option<Handle>` threaded down the whole element tree, no longer exists, and neither does the
`#[cfg]`-forked spawn path: one implementation now serves native and the browser alike, because the
runtime belongs to the thread rather than to whoever remembered to pass it down.

```rust
let on_press = VoidCallback::from_async(move || async move {
    let user = load(id).await;
    updater.set_state(|state| state.user = Some(user)); // still holding the `Rc`
});
```

`Button::on_press_async` and its siblings dropped `Send` with it, which is where the change is
actually visible to somebody writing an application.

**`AsyncBuilder`.** The private channel, the `Completion` type, the `generation` counter, the
`AbortHandle`/`Abortable` pair and the hand-written `Drop` are all deleted. A request now owns a
`TaskScope`; abandoning it drops the scope, which drops the task, which drops the future — so there is
no late answer to recognise and discard. The running task holds the request state only weakly, because
a task outliving the widget that asked the question is a task nobody is waiting for.

Because the future no longer has to be `Send`, the eight `#[cfg]`-duplicated implementations collapsed
into four, and `crossbeam-channel`, `futures-util` and `wasm-bindgen-futures` fell out of
`aimer_widget`'s dependencies entirely.

One detail worth stating plainly: an `AsyncBuilder` future is now polled on the UI thread, and Tokio
refuses to *create* its resources outside a runtime context. So each poll happens inside the
application's Tokio context — entered for the duration of the poll and not across the `await`, which
is the state the guard exists to scope. `reqwest::get(..).await` keeps working; genuinely blocking
work belongs on `offload`.

### Where It Stands

`aimer_venus` is green and clippy clean, it compiles for `wasm32-unknown-unknown`, and the frame loop
drives it in both the windowed and headless drivers: a microtask lands before the build, idle work runs
only in the slack after it, the runtime is installed for the drawing thread, and unfinished work asks
for another frame.

`cargo test --workspace` passes with zero failures across 2 394 tests.

That is the point of all of this. Not a faster runtime. A runtime that knows what a frame is.
