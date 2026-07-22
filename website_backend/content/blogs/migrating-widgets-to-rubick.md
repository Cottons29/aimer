# Migrating Aimer Widgets to Rubick

Aimer's widget tree used to erase concrete widgets and elements with `Box<dyn Widget>` and
`Box<dyn Element>`. The approach was simple and reliable, but every erased value required a separate
heap allocation—even when the value was only a few bytes.

We have now migrated `AnyWidget` and `AnyElement` to **Rubick**, Aimer's first-party inline-or-heap
smart pointer. Small values live directly inside their owner, while values that are too large or too
highly aligned transparently fall back to one heap allocation.

### Why We Built Rubick

A declarative UI creates and replaces many short-lived values while rebuilding a widget tree. An
individual allocation may be inexpensive, but allocating every erased leaf, wrapper, and element
adds allocator traffic to a hot path.

We wanted an owner with these properties:

- one value with normal Rust ownership and exactly-once destruction;
- dynamic dispatch for erased `Widget` and `Element` values;
- no additional allocation for common small values;
- transparent heap fallback for large or over-aligned values;
- a safe public API on stable Rust

`Rubick<T>` provides that contract in the standalone `aimer_rubick` crate. On 64-bit targets its
inline payload capacity is 32 bytes with alignment up to 16 bytes. Both size and alignment must fit;
otherwise Rubick allocates storage using the concrete value's layout.

### What Changed in the Widget System

The public erased owners now use Rubick:

```rust
pub type AnyWidget = Rubick<dyn Widget>;
pub type AnyElement = Rubick<dyn Element>;
```

Callers still use the familiar construction path:

```rust
let widget: AnyWidget = MyWidget::new().boxed();
let element: AnyElement = widget.to_element(ctx);
```

The difference is inside `.boxed()`: it projects the concrete value to the requested trait and lets
Rubick choose inline or heap storage. Stable Rust cannot automatically unsize arbitrary custom smart
pointers, so this projection is explicit inside Aimer's safe constructors.

The migration covered widget and element construction, stateless and stateful rebuild storage,
panic recovery, reconciliation, asynchronous builders, router-generated widgets, and downstream
Aimer crates. Existing borrowed widget-to-element conversion remains unchanged; this work removes
avoidable erased-owner allocations, not every clone performed during rebuilding.

### Safety and Movement

An inline payload moves when its `Rubick` owner moves. Rubick therefore never stores a pointer back
into its own inline buffer. Every borrow reconstructs the trait-object view from the owner's current
location, and the private operation table drops the original concrete value exactly once.

This distinction matters: **inline does not always mean stack allocated**. If an `AnyWidget` is an
element of a `Vec`, its payload lives inside the vector's allocation. It still avoids an additional
allocation for the erased value.

Rubick does not promise a stable payload address. Code that needs one must use the appropriate
pinning or owning strategy instead of relying on inline storage.

### Performance Comparison

We added a reproducible ownership microbenchmark that compares construction and destruction of
boxed values with Rubick's inline and heap modes. It runs one million operations per sample and
reports the median of seven release-mode rounds. A counting global allocator records allocations.

Test environment:

- Apple M4 (`arm64`), macOS 27.0;
- `rustc 1.97.1`;
- workspace release profile with optimization and LTO; and
- one warm-up phase before every measured sample.

| Owner and payload | Median time | Allocations per 1,000,000 owners |
|---|---:|---:|
| `Box<Small>` | 9.06 ns/op | 1,000,000 |
| inline `Rubick<Small>` | 1.88 ns/op | 0 |
| `Box<Large>` | 8.44 ns/op | 1,000,000 |
| heap `Rubick<Large>` | 8.68 ns/op | 1,000,000 |

For the 16-byte small payload, inline Rubick was about **4.8× faster** in this microbenchmark and
eliminated all per-owner allocations. For the 64-byte payload, Rubick correctly fell back to the
heap and remained close to `Box`—about 2.8% slower in this run.

Run the same benchmark on your machine with:

```bash
cargo run --release -p website_backend --example rubick_benchmark
```

These numbers measure only owner construction and destruction. They are not an end-to-end frame or
application benchmark, and allocator, CPU, payload mix, and surrounding work can change the result.
The allocation count and fallback behavior are the more portable findings.

### The Tradeoff: A Larger Owner

Inline capacity is not free. In this benchmark, `Box<Small>` is 8 bytes while `Rubick<Small>` is 80
bytes. A larger owner can reduce cache density, especially in large collections. That is why Aimer
uses a fixed 32-byte payload capacity rather than attempting to inline every possible widget.

The expected shape of the optimization is therefore:

- common small values avoid an allocation and allocator latency;
- large values retain predictable heap behavior;
- all owners pay for the larger inline-capable representation; and
- real applications should be profiled rather than assuming every workload improves.

### What Comes Next

The migration establishes Rubick as the erased owner beneath `AnyWidget` and `AnyElement` while
preserving Aimer's lifecycle semantics. The next performance work should measure complete widget-tree
builds, rebuild-heavy interactions, allocation counts per frame, and cache behavior across realistic
desktop and web applications.

Rubick is not a claim that heap allocation is always bad. It gives Aimer a measured middle ground:
keep small values close to their owner, and use the heap when the value's layout actually requires it.