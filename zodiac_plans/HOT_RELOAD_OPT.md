# Hot Reload Optimization Plan

## Problem statement

Hot-reload callbacks currently execute application Rust code inside the Wasmi
guest and exchange Widget IR with the native host. This is correct for code
isolation and reloadability, but the current design treats many operations as
full guest rebuilds.

Observed behavior in `website`:

- `same_looking.rs` button animations advance at roughly 0.5–2 seconds per
  frame instead of approaching 60 FPS.
- `get_started_button.rs` takes roughly 0.8 seconds to respond even though its
  callback does not change UI state or contain an animation.

A 60 FPS target leaves approximately 16.7 ms for the complete frame. The
current hot-reload animation path cannot meet that target when a full Widget
IR build, WASM memory transfer, native materialization, reconciliation, and
layout each occur on every animation frame.

## Current execution paths

### Ordinary callback

```text
native widget event
  -> native closure captures StableId128
  -> bounded callback queue
  -> event-loop wake
  -> application safe point
  -> CallbackEvent encoding and validation
  -> Wasmi aimer_dispatch_event
  -> guest callback body
  -> optional Widget IR output
  -> native materialization/reconciliation
```

The native callback adapter is installed in
`aimer_quiver/src/hot_reload.rs` (`bind_button_callbacks`). The safe-point
dispatch is implemented in `aimer_quiver/src/hot_reload/live.rs`.

### Portable animation

`ImplicitAnimatedBuilder` lowers its current child into the portable guest and
stores animation state in `PortableBuildContext`. While active, it calls
`PortableBuildContext::request_frame()` on every guest build.

The native host sees that frame request through `has_async_work()`. At the next
safe point it calls `poll_async()`, receives a new complete Widget IR image,
and calls `install_widget_image()`:

```text
guest animation tick
  -> request_frame
  -> native poll_async
  -> complete guest Widget IR rebuild
  -> WASM/native buffer copy
  -> materialize_aimer_widget_tree
  -> plan_element_reconciliation
  -> layout and draw
```

This is fundamentally different from native mode, where
`ImplicitAnimatedElement::draw()` ticks an `AnimationController` locally,
rebuilds only its animated child, and requests another native redraw.

## Confirmed issues

### 1. Every completed synchronous callback requests a rebuild

`crates/aimer_widget/src/portable/widget_ir.rs` currently calls
`context.queue_rebuild()` whenever a synchronous portable callback completes.
That means a callback that only performs a side effect still returns a full
Widget IR image.

`HoverableGetStartedButton` is an example: its callback prints a message and
calls `webbrowser::open()`, but it does not mutate application state. In
hot-reload mode, the dependency is rewritten to
`aimer_cli/portable_webbrowser`, whose `open()` implementation returns an
unsupported error because a guest has no direct native browser handle. The
browser shim is not expected to account for 0.8 seconds; the unnecessary full
rebuild is the expensive operation.

### 2. Portable implicit animations rebuild the complete guest tree per frame

`website/src/components/animation_button.rs` creates one
`ImplicitAnimatedBuilder` for each platform button. A selection change can
animate four button subtrees, and `same_looking.rs` also starts an
`AnimatedSwitcher` for the platform image.

Every active portable animation requests another guest build. The host then
materializes and reconciles the complete page, not just the changing visual
properties. This explains why the animation cost grows with page complexity.

### 3. Safe-point work is serialized with rendering

`LiveReloadHost::process_safe_point()` runs guest polling, callback dispatch,
and possible Widget IR installation before the frame is drawn. This preserves
tree coherence, but any expensive guest rebuild directly becomes frame time.

### 4. Native and portable animation semantics diverge

The native `AnimatedBuilder` path already owns a native animation controller,
ticks during draw, and requests a native redraw. Portable
`ImplicitAnimatedBuilder` does not use an equivalent native animation node;
its builder closure is evaluated in the guest, so the native host cannot
interpolate the guest-produced child without a richer portable representation.

### 5. AWIR preparation and commit share the UI safe point

`LiveReloadHost::process_safe_point()` currently performs module
instantiation, state transfer, Widget IR preparation, materialization,
reconciliation, and generation commit on the application thread. The listener
thread only transfers the module and queues a pending command. Consequently,
the old UI cannot render while the new candidate is being prepared.

The reload path should split preparation from publication. A background worker
can prepare an isolated candidate while the active generation continues to
render. The worker must never mutate the active widget tree or native UI
objects. The final native reconciliation and generation swap must still happen
on the application thread at a safe point.

### 6. One source edit can trigger 4–5 WASM compilations

One editor save can produce several filesystem notifications: content writes,
metadata updates, temporary-file replacement, or rename events. The current
watch path accepts every notification and only batches events that arrive
within a short receive window. The production loop then starts a rebuild for
each remaining `RebuildGuest` notification.

This can display four or five `Compiling` build numbers for one logical edit.
The build counter is incremented before compilation, so these numbers are
compilation attempts, not necessarily committed native generations. Events
that arrive while a build or upload is in progress can also remain queued and
start another build when the first one finishes. The existing
`ChangeCoalescer` and `WatchStateMachine` abstractions are not yet connected to
the production watcher.

The watcher should treat one quiet batch of relevant source changes as one
reload request:

- filter `notify::EventKind` to source create/modify/remove/rename events and
  ignore access, metadata-only, and known editor temporary-file events;
- normalize and deduplicate paths before classification;
- use a trailing quiet window, such as 100–200 ms, after the last relevant
  event;
- track `Idle`, `Building`, `Uploading`, and terminal states with a dirty flag;
  changes during an active request must not start a concurrent build;
- after the active request finishes, perform at most one follow-up build for
  the newest dirty batch;
- avoid overlapping watch roots and continue ignoring Cargo target outputs.

Expose separate diagnostics for raw filesystem events, coalesced batches,
guest build attempts, uploads, and committed generations. This makes it clear
whether a repeated status is caused by the editor, the watcher, Cargo, or the
host commit path.

## Optimization goals

1. Side-effect-only callbacks should not rebuild Widget IR.
2. A normal hot-reload callback should complete within one display frame when
   it does not change a large UI tree.
3. Active hot-reload animations should target 60 FPS on a representative page:
   p95 frame processing below 16.7 ms, excluding unavoidable platform GPU
   presentation variance.
4. Animation state must remain generation-local and safe across reloads.
5. Native input and rendering must remain single-thread coherent; no callback
   may mutate the live tree from the listener thread.
6. One logical source edit should produce one guest compilation, except for a
   single intentional follow-up when a newer edit arrives during compilation.

## Prioritized optimization path

### [ ] Phase 0: Measure before changing behavior

Add release-gated timing counters or trace spans around:

- callback enqueue;
- callback safe-point start;
- `GuestInstance::dispatch_event()`;
- `GuestInstance::poll_async()`;
- Widget IR byte length and node count;
- `materialize_aimer_widget_tree()`;
- reconciliation commit;
- layout and draw completion.

Record separate distributions for:

- `Get Started` side-effect-only callback;
- a state-changing platform selection callback;
- one animation frame with no callback;
- initial module installation.

This distinguishes event-loop delay from Wasmi execution, serialization,
materialization, reconciliation, and rendering cost. Debug logging and
`widget_ir_diagnostics` must be disabled for the performance baseline.

### [ ] Phase 1: Stop unnecessary callback rebuilds

Change portable callback completion semantics so a rebuild is requested only
when one of these conditions is true:

- a retained/live state mutation was actually submitted;
- the callback explicitly requests a rebuild;
- an async callback completes with a state or output change.

Do not infer “rebuild required” solely from synchronous completion. Preserve
the existing callback error, generation, and cancellation behavior.

Add regression tests for:

- a synchronous side-effect-only callback returning no Widget IR;
- a synchronous callback that calls `StateUpdater::set_state()` returning one
  rebuilt image;
- callback failure still returning a diagnostic without publishing a tree;
- multiple state mutations coalescing into one rebuild.

This should make `HoverableGetStartedButton` cheap, although opening a browser
still needs a native capability/event if it is expected to work in hot-reload
mode.

### [ ] Phase 2: Coalesce watcher events into one guest build

Connect the production watcher to the existing `ChangeCoalescer` and
`WatchStateMachine`, or provide an equivalent state machine at the same
boundary. The watcher must emit one logical change batch rather than exposing
raw filesystem notifications directly to `rebuild_and_push()`.

Add regression tests for:

- five notifications for one file save producing one guest build;
- notifications spaced across the quiet window being grouped correctly;
- changes arriving during compilation producing one follow-up build without
  overlapping compiler processes;
- temporary, metadata-only, and ignored target events producing no build;
- a failed build still clearing the current batch and coalescing later edits.

Record both the raw event count and the resulting build count in diagnostics.
The expected invariant is one build per semantic source revision, not one
build per operating-system notification.

### [ ] Phase 3: Native-local animation node

Introduce a portable animation representation that carries the animation
parameters and animatable values needed by the native host, rather than only a
guest-produced child image. The minimum representation should contain:

- stable animation slot/key;
- current value and target value;
- duration;
- curve;
- animation status;
- the child/property payload required by the native materializer.

The native materializer should create or update a native animation element.
The element should:

- tick an `AnimationController` on the UI thread;
- interpolate supported visual properties locally;
- request only a native redraw while active;
- avoid calling Wasmi for intermediate frames.

The first implementation should target the concrete hot path used by the
website: `f32` progress driving width, border radius, colors, and checkmark
spacing. General arbitrary guest closures should remain on the existing
portable fallback path until they have a representable native form.

### [ ] Phase 4: Prepare AWIR in the background and gate commit with `R`

Split the reload pipeline into a background preparation phase and a short
application-thread commit phase:

```text
file change
  -> guest compile and module upload
  -> background Wasmi/AWIR candidate preparation
  -> pending prepared candidate
  -> user presses R in the `aimer-cli` terminal
  -> UI-thread validation, reconciliation, generation swap, and frame request
```

The UI thread should first capture the minimum immutable input needed for the
candidate, such as the active generation revision and a serialized state
bundle. The background phase should then perform only work that is independent
of the live widget tree, including module validation, Wasmi instantiation,
state migration from the captured bundle, Widget IR generation, candidate
validation, and reconciliation-plan construction where the required data is
owned and thread-safe. The UI phase should perform only the final native
mutation: verify the candidate revision, apply the reconciliation plan, install
the new root and callback table, invalidate layout, and request a frame.

Manual `R` mode is a terminal command owned by `aimer-cli`; it is not an app
window key binding and does not enter the guest callback path. It should work
as follows:

- Keep rendering the current generation while preparation is in progress.
- Store at most one pending candidate and use latest-candidate-wins behavior.
- If a newer edit arrives, cancel preparation or discard its stale result.
- If `R` is pressed in the `aimer-cli` terminal before preparation completes,
  record a commit request and commit as soon as the matching candidate is
  ready.
- If no candidate is ready, keep the current UI unchanged and show a bounded
  “preparing reload” status rather than blocking the event loop.
- Keep generation identity, callback ownership, state-transfer checks, and
  resource cleanup on the UI-thread commit path.

When hot reload is disabled, preserve the existing terminal hot-restart
behavior: both `r` and `Shift+R` remain hot-restart commands. They must not be
reinterpreted as the manual prepared-candidate commit command.

### Terminal status and notification UX

The `aimer-cli` terminal should show one in-place spinner/status line while the
background reload pipeline is active. It should update for meaningful state
transitions instead of printing one line per filesystem notification:

```text
Watching              ·
Change detected       ·
Coalescing changes    ·
Locking reload        ·
Compiling WASM        ·
Uploading module      ·
Preparing AWIR        ·
Waiting for R         ·
Reload Ready          ✓  press R to apply
Reloading             ·
Reloaded generation 7 ✓
```

The ready bulletin should use green terminal styling when color is available:
`Reload Ready ✓  press R to apply`. Failure, rejection, and cancellation
should use the existing diagnostic path with an appropriate non-green status.
The spinner and bulletin must be driven by the background state transitions;
they must not block compilation, AWIR preparation, or the UI event loop.

For interactive terminals, provide optional notification integration:

- emit one terminal bell only when a candidate becomes ready, never for every
  source event or spinner update;
- use a supported terminal notification escape sequence when capability
  detection or an explicit opt-in confirms it is safe;
- allow notifications to be disabled with the existing CLI convention, such
  as `NO_COLOR`/non-interactive detection or a dedicated reload-notify option;
- fall back to the visible `Reload Ready` bulletin when the terminal does not
  support notifications;
- never emit spinner control sequences or notification escapes in CI, piped
  output, or captured logs.

Terminal input and status output must share one synchronized CLI console so a
typed `R` is not lost while the spinner redraws. Pressing `R` should change the
status to `Reloading`, then replace it with `Reloaded generation N` or a
bounded rejection diagnostic.

The candidate passed between threads must be an owned, `Send` value. It must
not contain `Rc`, `RefCell`, `BuildContext`, live native elements, window
handles, or callbacks that capture the active tree. If the current Wasmi or
AWIR types cannot satisfy this boundary, split them into a thread-safe
prepared representation and a UI-owned materialization step.

Add tests for:

- the active generation continuing to render while preparation runs;
- terminal `R` committing exactly the matching source revision;
- terminal `R` before preparation completion committing once preparation
  finishes;
- the terminal spinner updating in place for background compile, upload,
  preparation, and commit states;
- a green `Reload Ready` bulletin appearing exactly once per ready candidate;
- one optional terminal notification occurring only when reload becomes ready;
- non-interactive output containing no cursor-control or notification escapes;
- a stale candidate never replacing a newer generation;
- a rejected candidate leaving the active tree unchanged;
- the UI-thread commit requesting at most one frame.

Measure the commit duration separately from background preparation. The commit
path should stay within the frame budget; if reconciliation itself exceeds
16.7 ms, add incremental or frame-budgeted reconciliation rather than moving
native tree mutation to the worker.

### [ ] Phase 5: Make `ImplicitAnimatedBuilder` portable lowering native-aware

Update `ImplicitAnimatedBuilder` so hot-reload lowering emits the native-aware
animation node instead of recursively rebuilding its child in the guest for
each tick. The initial target update may still cross the WASM boundary once.

The native host must retain the old visual value when a target changes so
retargeting remains continuous. A generation replacement must discard the
native animation element and seed the new generation from its initial value.

Add tests for:

- target changes starting one native animation;
- equal target values not restarting an animation;
- retargeting from the current interpolated value;
- animation completion stopping redraw requests;
- generation replacement not retaining stale callback or animation state.

### [ ] Phase 6: Reduce rebuild scope for non-animation state changes

For callbacks that genuinely change UI state, add a bounded Widget IR patch or
subtree update path. A callback should not need to serialize and materialize
the complete page when a stable subtree is the only affected region.

The patch protocol must retain generation and callback validation, document
limits, stable keys, and atomic publication. If a patch cannot be validated or
materialized, the active tree must remain unchanged.

### [ ] Phase 7: Runtime and allocation tuning

Only after the architectural work is measured:

- compare optimized Wasmi builds against the current debug pipeline;
- reuse guest input/output buffers where safe;
- avoid duplicate Widget IR validation when the same buffer has already been
  validated at the ABI boundary;
- reduce temporary allocations in native materialization;
- evaluate an optimizing WASM runtime only if guest execution remains a
  measured bottleneck.

These changes cannot compensate for a full guest rebuild per 16.7 ms frame,
so they are later-stage work.

## Acceptance criteria

- `Get Started` side-effect-only callback does not produce a Widget IR image.
- `Get Started` hot-reload callback latency is measured separately from the
  external browser-launch operation.
- `same_looking.rs` animation frames do not call Wasmi after the initial target
  update when the native animation node is active.
- Background AWIR preparation does not block active rendering, and manual `R`
  commits only the matching prepared candidate.
- One logical source edit produces one guest build, with at most one queued
  follow-up build when a newer edit arrives during the active build.
- The UI-thread candidate commit, including native reconciliation, has a
  measured p95 below 16.7 ms on the representative page.
- Representative animation frame processing has p95 below 16.7 ms in an
  optimized build, with the test page and diagnostics configuration recorded.
- No callback queue overflow, stale-generation acceptance, dropped-event
  ambiguity, or live-tree mutation from a non-application thread is introduced.
- Native AOT behavior remains unchanged.

## Non-goals

- Guaranteeing 60 FPS when the platform GPU, browser launch, asset decoding,
  or host machine is independently overloaded.
- Allowing arbitrary guest closures to execute directly against native widget
  objects.
- Removing generation validation, event ordering, resource limits, or the
  application safe point.
