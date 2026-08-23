# Shared Ownership and Experimental Hot Reload

Two recent Aimer experiments look unrelated at first glance. One adds `Shared<T>`, a small single-threaded smart pointer in `aimer_std`. The other adds a feature-gated hot-reload path that builds a guest WebAssembly module and keeps a native application alive while its widget code changes.

They are connected by one boundary: ownership is useful only when it is explicit about where it ends.

## A smart pointer for the UI thread

`Shared<T>` is an owning, reference-counted pointer for code that intentionally stays on one thread. It stores the value and its strong and weak counters in one allocation. Cloning a handle is cheap, and the value is dropped when the final strong handle disappears.

```rust
use aimer_std::read_only::Shared;

let state = Shared::new(String::from("Aimer"));
let another_view = state.clone();

assert_eq!(&*another_view, "Aimer");
assert_eq!(Shared::strong_count(&state), 2);
```

The type is deliberately not `Send` or `Sync`. Aimer's widget tree, controllers, and many of its framework handles are also single-threaded. Making that restriction part of the pointer's type-level contract prevents a value that depends on UI-thread state from quietly crossing into a worker thread.

That choice is different from saying that Aimer cannot do background work. It can, and the work can return to the UI scheduler. It means the ownership of the UI value itself remains on the thread that renders it.

## Strong and weak ownership

A strong `Shared<T>` keeps `T` alive. A `Weak<T>` keeps only the allocation alive and can be upgraded while the value still has a strong owner:

```rust
use aimer_std::read_only::{Shared, Weak};

let state = Shared::new(42_u32);
let observer: Weak<u32> = Shared::downgrade(&state);

assert_eq!(observer.upgrade().map(|value| *value), Some(42));
drop(state);
assert!(observer.upgrade().is_none());
```

The weak handle is important for tree-shaped data. A parent can own its children strongly while a child keeps a weak back-reference to the parent. Strong cycles still leak, just as they do with other reference-counted pointers, so `Weak` is the non-owning edge that makes the ownership graph explicit.

When a value is uniquely owned, `Shared::get_mut` provides mutable access without a clone. `Shared::make_mut` provides the copy-on-write version: if another strong or weak handle exists, it clones the value into a new allocation before returning a mutable reference. This keeps ordinary shared reads cheap while preserving a clear rule for mutation.

## Keeping a field alive without a raw pointer

UI code often wants a long-lived view of one field inside a larger state object. `Shared::project` creates a `SharedRef` for that purpose:

```rust
struct AppState {
    title: String,
    counter: u32,
}

let state = Shared::new(AppState {
    title: "Hot reload".into(),
    counter: 1,
});
let title = state.project(|state: &AppState| &state.title);
drop(state);

assert_eq!(&*title, "Hot reload");
```

`SharedRef` owns the complete root value and stores a selector that is evaluated when the field is accessed. It does not keep a raw interior pointer that could outlive a moved or replaced owner. The result is an owning, read-only projection with a straightforward lifetime: the selected field is available for as long as the root owner is alive.

## What crosses a hot-reload boundary?

Experimental hot reload has a different ownership problem. The guest contains application code and a guest runtime. The host owns the native window, renderer, retained tree, and materializers. A reload creates a new guest generation, validates its bounded document, and commits it at a native safe point.

The system does **not** serialize a `Shared<T>`, an `Rc<T>`, a `Weak<T>`, a Rust closure, or an executor handle into the reload document. Those are process-local ownership details. Pointer addresses and reference counts would have no stable meaning in a different WebAssembly instance or in the native host.

Instead, the guest lowers widgets into a bounded AWIR document. The document carries stable schema identities, property values, child relationships, state records, and callback identities. The host validates that representation and asks its permanent widget registry to materialize a native tree.

That gives each side a clean job:

| Local to a generation | Safe to carry across the boundary |
| --- | --- |
| `Shared<T>` and `Weak<T>` ownership | Stable widget, property, and callback identities |
| `SharedRef` selectors | Bounded `PortableValue` encodings |
| `Rc`, `RefCell`, and executor state | AWIR nodes, strings, and validated blobs |
| Native handles and pointer addresses | Versioned state snapshots and capability requests |

`Shared` can therefore be useful inside a guest or native UI implementation, but it is never the state-transfer format. State that should survive a compatible reload must have an explicit, bounded, versioned contract. Local pointer graphs are rebuilt; portable state is migrated.

## From a source edit to a committed generation

The hot-reload implementation is a pipeline, not a dynamic library swap. The CLI, guest, protocol, interpreter, and native host each own a separate part of the transaction:

```text
source change
    │
    ▼
watch → shadow project → wasm guest build → authenticated upload
                                      │
                                      ▼
                         isolated candidate generation
                                      │
              validate → transfer state → build AWIR → materialize
                                      │
                                      ▼
                         native event-loop safe point
                                      │
                         commit or reject and roll back
```

### 1. The CLI creates an isolated guest

`aimer_cli` first resolves the target, toolchain, package, and development policy. In hot-reload mode it creates an ephemeral session identity and builds the permanent native host with the development-only reload feature enabled. The mutable application code is kept out of the native bundle.

For the automatic path, the CLI copies the application into a bounded staging area called the shadow project. The shadow transform discovers the application entry point and its owned modules, preserves source coordinates for diagnostics, enables the `portable-guest` feature, and emits a generated root factory plus an `aimer_wasm_guest::GuestProgram` adapter. The output is deterministic: the same source and configuration produce the same generated guest bytes. Native-only dependencies, path escapes, unsupported source shapes, and excessive source/resource bounds are rejected before Cargo runs.

The generated adapter exposes the controlled guest lifecycle rather than exporting the application directly:

```text
manifest → initialize → build
                    ├→ callback rebuild
                    ├→ export state
                    ├→ migrate state (optional)
                    ├→ import state
                    └→ dispose
```

`StatefulWidget` and `StatelessWidget` derives use the same portable child-lowering path. A stateful widget keeps its state in the guest lifecycle and exposes a versioned state image; a stateless widget lowers its build result and children without smuggling a native element into the module.

### 2. The guest produces bounded data

The guest never hands the host a Rust object, pointer, `Rc`, `Shared`, closure, or `Future`. Each export returns a checked byte image through the guest ABI. The host negotiates output capacity, validates the returned pointer/length/alignment against the guest allocation ledger, copies the bytes into host-owned storage, and releases the guest allocation with the exact ownership tuple.

The main image is the binary Widget IR, or AWIR. It contains a generation ID, schema/version identities, bounded string and value tables, nodes, properties, children, and callback bindings. State is carried through a separate canonical state bundle so the host can distinguish widget structure from migration data. `PortableValue` codecs define canonical bytes for values such as options, tuples, vectors, maps, sets, and style records. A decoder rejects malformed, trailing, duplicate, non-canonical, or over-limit data before a native widget sees it.

The manifest is validated alongside the module. It declares the application identity, ABI range, widget schemas, callback schemas, capabilities, limits, and module digest. The same structural policy is applied locally by the CLI and again by the host; local validation is a fast diagnostic, while the running application remains the security boundary.

### 3. The listener authenticates before accepting a module

The development listener uses the `aimer_reload_protocol` framed binary protocol. The transport may be loopback TCP, an Android forward, or a device route, but the framing and state machine are the same:

```text
AMRL | protocol version | message kind | flags | payload length
     | session ID | request ID | direction sequence | auth tag | payload
```

At launch, the CLI creates a random session token and session ID and injects them through a target-private channel. The app answers with a fresh nonce; both sides authenticate the handshake transcript and derive connection-specific directional keys. The token is not printed in logs or placed in the project. After authentication, frame sequences must increase exactly, session IDs must match, and the tag must verify before a payload is decoded.

Module transfer is deliberately boring and bounded. `ModuleBegin` declares the length, SHA-256 digest, application/build identity, ABI, and capability-manifest digest. `ModuleChunk` carries contiguous chunks in order. `ModuleEnd` succeeds only when the byte count and incremental digest match. An interrupted or rejected upload is discarded before it reaches the interpreter.

The CLI watcher follows a small state machine:

```text
Idle → Dirty → Building → ReadyToPush → Uploading → WaitingForResult → Idle
```

Burst notifications are coalesced, only one guest build runs at a time, and one follow-up build is scheduled when a second change arrives during an active build. A guest compile failure or reload rejection leaves the running application and authenticated session alive.

### 4. Each candidate gets its own sandbox and generation

When the host receives a complete module, it assigns a monotonically increasing candidate generation ID. It creates a fresh `wasmi::Store`, memory/table limiter, fuel budget, `TaskScope`, callback snapshot, and resource registries. The candidate store never borrows objects from the active store. Imports are explicitly linked from the declared Aimer capability manifest; WASI, undeclared imports, start functions that escape initialization, and native SDK dependencies are not allowed.

The candidate is preflighted in this order:

1. Validate the WebAssembly format, ABI signatures, limits, manifest, and digest.
2. Instantiate the isolated store and run `aimer_initialize` with immutable host snapshots.
3. Export the old guest state and run the candidate migration, if the schema declares one.
4. Import the resulting state into the candidate and verify it.
5. Run the candidate's initial `aimer_build` and copy/decode its AWIR document.
6. Resolve every schema through the permanent native registry and materialize a disconnected native tree.
7. Build a side-effect-free reconciliation plan and prevalidate staged resources.

Initialization, migration, and the first build may register dormant resources, but they cannot publish irreversible effects. Network requests, timers, subscriptions, and capability handles are generation-owned; effects become active only after commit. This is why a candidate can be discarded without undoing work performed by the current application.

### 5. State transfer is a barrier, not a pause button

Before exporting state, the host establishes a state-transfer barrier at an event-loop safe point. The old native root remains installed and renderable, but callbacks and generation-owned completions stop entering the old guest. Incoming input is copied into a bounded FIFO with its original sequence number. If the queue reaches its limit, the system reports backpressure or rejection instead of silently dropping input.

The old guest exports a canonical state bundle. The candidate may migrate that bundle to a new schema, imports it, and performs a verification build. A failure in old export, migration, import, or verification rejects the candidate because preserving full state could not be proven. The old guest and native tree remain the active pair.

### 6. Commit happens only at one native safe point

The candidate is never published during preparation. `aimer_quiver` installs it only between event dispatch and tree traversal operations on the UI thread. The safe-point commit checks that the expected old generation is still active and that no newer candidate superseded this one. It then:

1. stops admitting new events to the old generation;
2. performs the already-planned keyed/positional native state carry;
3. swaps the root, callback table, generation ID, and generation handle as one coherent snapshot;
4. activates prevalidated staged resources and requests a frame;
5. releases the barrier and replays queued events once against the new callback table;
6. retires the old generation.

The old callback table can never be paired with the new root. Retirement is idempotent: mark the generation inactive, remove event routing, cancel tasks, revoke timers/subscriptions/requests/capability handles, run best-effort guest disposal, and finally drop the interpreter store. Late completions carry their generation ID and are rejected after retirement.

Any ordinary failure before native state carry follows the inverse path. The candidate root, callback table, reconciliation plan, staged effects, and resources are discarded; the old snapshot remains unchanged; the barrier is released; and queued events are replayed to the old generation exactly once. A newer candidate can supersede an older one while it is still in preflight, but only one candidate may reach the commit phase.

## Callbacks belong to a generation

The same rule applies to callbacks. A callback closure stays inside the guest generation that created it. AWIR carries a stable callback identity, widget key, event kind, and event schema, not the closure's captured memory. A synchronous event is encoded as a bounded `AEVT` document and is accepted only after the host checks the generation ID, callback ID, widget key, event kind, schema version, and strictly increasing event sequence.

For the supported async path, a host-owned task receives a generation-local task ID. Completion, failure, and cancellation travel as bounded `AASY` records carrying the generation, task, callback, and event sequence. The host validates every field before removing the task or advancing its sequence; stale, duplicated, out-of-order, over-limit, or retired-generation records leave the active generation unchanged. Guest-owned futures are cancelled by the generation's Venus `TaskScope`; host-owned requests are held in the generation resource registry.

The native host can therefore reconnect a stable callback identity to a materialized event node without copying a closure. An executor, a `Shared` pointer, and a native closure remain on their original side of the boundary.

This separation is what makes a reload more than replacing a function pointer. The candidate must pass schema validation, state compatibility, callback identity checks, and resource limits before it can replace the active generation. If any step fails, the previous native tree remains active.

## Why the feature is still experimental

The hot-reload path is feature-gated and intended for development. It adds a guest build, a reload listener, a native safe point, and a permanent materializer registry; it does not change the normal native AOT path. A native release build should not ship the development listener or the interpreter just because an application uses `Shared`.

The remaining proof is intentionally practical: launch real application surfaces, interact with them, edit a widget, observe state and callback continuity, and confirm that a failed candidate leaves the running app untouched. Visual validation matters because a protocol can be structurally correct while a screen still has a missing materializer, an unexpected layout change, or a callback that never reaches the event tree.

The useful mental model is simple:

> Shared ownership keeps one generation coherent. Portable identities and bounded records let the next generation take over safely.

That is the boundary Aimer is building toward: inexpensive local ownership on the UI thread, explicit values at the reload seam, and a native application that can keep running while the guest evolves.
