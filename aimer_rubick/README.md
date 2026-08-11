# `aimer_rubick`

`aimer_rubick` provides [`Rubick<T, WORDS>`](https://docs.rs/aimer_rubick/latest/aimer_rubick/struct.Rubick.html),
an exclusive owner with small-object optimization. A fitting value is embedded
inside its `Rubick`; a larger or over-aligned value transparently takes one
block from a thread-local pool. The crate has no third-party dependency.

## Representation

An owner is `WORDS + 1` machine words:

```text
Rubick<T, WORDS>
+-------------------------------+---------------------+
| payload buffer (WORDS words)  | &'static Operations |
+-------------------------------+---------------------+
  inline mode: the value itself   projection, drop,
  heap mode:   word 0 = pointer   layout, pool class,
               words 1.. unused   inline flag
```

There is no runtime storage discriminant. Whether a concrete type is stored
inline is a function of its size and alignment against the capacity, and all
three are compile-time constants, so the answer lives in the operation table
that the owner already points at. The table itself is built in a constant and
promoted to `'static`, so constructing an owner writes one word rather than
copying a set of function pointers into every instance.

```rust
use aimer_rubick::Rubick;

assert_eq!(size_of::<Rubick<u32>>(), 5 * size_of::<usize>());
assert_eq!(align_of::<Rubick<u32>>(), align_of::<usize>());
```

## Inline does not mean stack allocated

Inline means that the value needs no allocation separate from its owner. A local
`Rubick` is normally on the stack, but an inline `Rubick` inside a `Vec` is inside
the vector's heap allocation. In both cases the value is embedded directly in
the owner.

The default payload capacity is four machine words, 32 bytes on a 64-bit
target. Inline storage is an array of words, so its alignment is exactly word
alignment: a value uses heap storage when its size exceeds the capacity or its
alignment exceeds `INLINE_ALIGNMENT`.

```rust
use aimer_rubick::{INLINE_CAPACITY, Rubick};

let mut bytes = Rubick::new([0_u8; INLINE_CAPACITY]);
bytes[0] = 7;

assert_eq!(bytes[0], 7);
assert!(bytes.is_inline());

let large = Rubick::new([0_u8; INLINE_CAPACITY + 1]);
assert!(large.is_heap());
```

Zero-sized values are inline when their alignment fits. A value exactly at the
size boundary is inline only when its alignment also fits.

## Capacity is chosen per alias

`WORDS` is a const parameter, so each alias trades owner size against the
allocations it avoids. Aimer uses both ends of that range:

| Alias | Capacity | Owner | Why |
| --- | ---: | ---: | --- |
| `AnyWidget` | 8 words | 72 B | widgets are small, rebuilt every frame, and worth inlining |
| `AnyElement` | 1 word | 16 B | elements are 104–192 B, so no realistic buffer would ever hold one |

Measured 64-bit Aimer layouts:

| Value | Size | Storage |
| --- | ---: | --- |
| `Row` / `Column` | 64 | inline in `AnyWidget` |
| `Text` | 136 | pooled block |
| `NamedWidget` | 80 | pooled block, see below |
| `StatelessElement` | 104 | pooled block as `AnyElement` |
| `StatefulElement` | 192 | pooled block as `AnyElement` |

Eight words is chosen so that the common containers land exactly inside it.
`NamedWidget` cannot: it stores an `AnyWidget` plus a name, so it is always one
word larger than the owner that would have to hold it, whatever the capacity.
That is a property of the wrapper, not of the capacity, and only a redesign of
`NamedWidget` would remove its block.

Giving `AnyElement` a one-word buffer is not a limitation but the point: those
elements were always going to allocate, so an inline buffer would be dead
weight in every node of the retained tree.

```rust
use aimer_rubick::Rubick;

type Thin = Rubick<dyn std::fmt::Debug, 1>;
type Roomy = Rubick<dyn std::fmt::Debug, 8>;

assert_eq!(size_of::<Thin>(), 2 * size_of::<usize>());
assert_eq!(size_of::<Roomy>(), 9 * size_of::<usize>());
```

`Rubick::new` fixes the default capacity so that it needs no turbofish, because
Rust never falls back to a const parameter's default during inference. Use
`Rubick::erase`, which accepts any capacity, for the other sizes.

## Sized values

Use `Rubick::new` for a sized value. `Deref`, `DerefMut`, `AsRef`, and `AsMut`
provide borrowed access.

```rust
use aimer_rubick::Rubick;

let mut name = Rubick::new(String::from("Aimer"));
name.push_str(" GUI");

assert_eq!(&*name, "Aimer GUI");
```

## Trait targets

Stable Rust does not provide general `CoerceUnsized` support for custom smart
pointers, so `Rubick<dyn Trait>` cannot be produced by an implicit conversion
from `Rubick<Concrete>`. The `ErasedFrom` trait supplies the one piece the
compiler withholds: a constant *template* pointer carrying the concrete type's
metadata — the vtable word — with a null data address. Since metadata never
depends on where the value lives, the owner rebuilds a valid `*const dyn Trait`
on every borrow by writing the payload's current address into the template's
data word. Borrowing therefore costs no call at all, and nothing is stored
beside the value.

Implement the trait on the target, which also keeps a downstream blanket
implementation within the orphan rules:

```rust
use aimer_rubick::{ErasedFrom, Rubick};

trait Counter {
    fn increment(&mut self);
    fn value(&self) -> usize;
}

// SAFETY: The template is `null::<C>()` coerced to the target.
unsafe impl<C: Counter + 'static> ErasedFrom<C> for dyn Counter {
    const TEMPLATE: *const Self = std::ptr::null::<C>() as *const dyn Counter;
}

struct Count(usize);

impl Counter for Count {
    fn increment(&mut self) {
        self.0 += 1;
    }

    fn value(&self) -> usize {
        self.0
    }
}

let mut counter: Rubick<dyn Counter> = Rubick::erase(Count(41));
counter.increment();

assert_eq!(counter.value(), 42);
assert!(counter.is_inline());
assert!(counter.is_direct());
```

## Explicit projection

`Rubick::new_projected` covers borrows that are not a plain unsizing, such as
exposing an interior field. It stores two adapters beside the value and calls
one on every borrow, so prefer `erase` whenever it applies.

```rust
use aimer_rubick::Rubick;

struct Envelope {
    stamp: u32,
    message: String,
}

let letter: Rubick<String> = Rubick::new_projected(
    Envelope { stamp: 7, message: String::from("hello") },
    |envelope: &Envelope| &envelope.message,
    |envelope: &mut Envelope| &mut envelope.message,
);

assert_eq!(&*letter, "hello");
assert!(!letter.is_direct());
```

Named function items and non-capturing closures are usually zero-sized;
capturing closures or values coerced to function pointers are stored with the
concrete value and count toward the inline capacity.

## Heap payloads are pooled and reusable

Heap mode does not call the global allocator on the common path. Blocks come
from a thread-local free list of size classes (16 to 512 bytes); allocation
pops a pointer and deallocation pushes it back, both without a lock, and the
class is resolved once per concrete type in its operation table. The pool is
sound precisely because `Rubick` is `!Send`: a payload is always freed on the
thread that allocated it. Layouts that no class can serve fall through to the
global allocator with their exact layout.

`Rubick::replace` builds on the same information. Reconciliation regenerates a
tree whose nodes usually keep their concrete types, so the replacement normally
lands in the same size class and the existing block is reused in place — no
allocator traffic at all, not even a free list operation.

```rust
use aimer_rubick::{ErasedFrom, Rubick};

trait Label {
    fn text(&self) -> &str;
}

// SAFETY: The template is `null::<L>()` coerced to the target.
unsafe impl<L: Label + 'static> ErasedFrom<L> for dyn Label {
    const TEMPLATE: *const Self = std::ptr::null::<L>() as *const dyn Label;
}

struct Title(&'static str);

impl Label for Title {
    fn text(&self) -> &str {
        self.0
    }
}

let mut label: Rubick<dyn Label> = Rubick::erase(Title("draft"));
label.replace(Title("final"));

assert_eq!(label.text(), "final");
```

## Moves, pinning, and addresses

Moving an unpinned owner moves an inline payload and changes its address. This
includes swaps and collection reallocation. `Rubick` never retains an internal
pointer into inline storage, so dynamic dispatch remains valid after such moves.

A heap payload normally keeps the same allocation address when its owner moves,
but `Rubick` does not expose this as a stable-address guarantee. Use Rust's
standard `Pin` APIs when a stable address is part of a type's contract. Once a
`Rubick` is pinned, safe code cannot move it unless its target permits that under
the standard `Unpin` rules.

## Destruction and safety

All unsafe code is private to this crate, except the `ErasedFrom` contract that
an implementor must uphold. The implementation maintains these invariants:

- Inline bytes contain exactly one initialized concrete value whose size and
  alignment fit the buffer.
- Heap storage comes from the pool with the concrete layout and is returned
  with the same class and layout.
- Projection, drop, layout, and class are installed for that exact concrete
  storage type.
- Every borrow derives a fresh pointer from the owner's current storage; a
  rebuilt pointer only ever replaces the data word of a template.
- The concrete value is dropped exactly once, including during panic unwinding
  and including a `replace` whose destructor panics, which leaves the owner in
  a vacant state rather than a doubly owned one.

`Rubick` is conservatively neither `Send` nor `Sync`. A projected owner erases
the concrete type, the operation table cannot express every auto trait of that
hidden type, and the payload pool is thread local.

## Limitations and non-goals

- `Rubick` provides exclusive ownership, not `Rc`/`Arc`-style shared ownership.
- It does not provide automatic trait-object unsizing.
- It does not promise a stable address for an unpinned payload.
- It does not provide a thread-transferable owner.
- It assumes a pointer is one or two machine words, which is asserted at
  compile time for every target type.

Avoid wrapping an existing `Box<T>` when allocation avoidance is the goal:

```rust
use aimer_rubick::Rubick;

let nested = Rubick::new(Box::new([0_u8; 128]));

// The small `Box` handle may fit inline, but its array was already allocated.
assert!(nested.is_inline());
```

Construct `Rubick` from the concrete value directly so it can choose inline or
heap storage itself.

## Benchmark

`cargo run -p aimer_rubick --example rubick_benchmark [--release]` reports the
layout and the cost of construction, dispatch, and a tree rebuild against
`Box<dyn Trait>`. See `BENCHMARK.md` for measured results.
