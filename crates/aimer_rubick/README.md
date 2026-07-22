# `aimer_rubick`

`aimer_rubick` provides [`Rubick<T>`](https://docs.rs/aimer_rubick/latest/aimer_rubick/struct.Rubick.html),
an exclusive owner with small-object optimization. A fitting value is embedded
inside its `Rubick`; a larger or over-aligned value transparently uses one heap
allocation. The crate has no third-party storage dependency.

## Inline does not mean stack allocated

Inline means that the value needs no allocation separate from its owner. A local
`Rubick` is normally on the stack, but an inline `Rubick` inside a `Vec` is inside
the vector's heap allocation. In both cases the value is embedded directly in
the owner.

The inline payload capacity is four machine words and its maximum alignment is
16 bytes. On a 64-bit target this is 32 bytes. A value uses heap storage when
either its size exceeds `INLINE_CAPACITY` or its alignment exceeds
`INLINE_ALIGNMENT`.

The capacity was selected from representative 64-bit Aimer layouts:

| Value | Size | Alignment |
| --- | ---: | ---: |
| `NamedWidget` | 32 | 8 |
| `Column` / `Row` | 64 | 8 |
| `StatelessElement` | 104 | 8 |
| `Text` | 136 | 8 |
| `StatefulElement` | 192 | 8 |

Four words capture the compact erased owner without making every `Rubick` large
enough for medium and large framework values.

## Sized values

Use `Rubick::new` for a sized value. `Deref`, `DerefMut`, `AsRef`, and `AsMut`
provide borrowed access.

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

## Trait targets and explicit projection

Stable Rust does not provide general `CoerceUnsized` support for custom smart
pointers. Consequently, `Rubick<dyn Trait>` does not support an automatic
conversion from `Rubick<Concrete>`. Construct an erased owner with
`Rubick::new_projected` and provide shared and mutable projection adapters.

```rust
use aimer_rubick::Rubick;

trait Counter {
    fn increment(&mut self);
    fn value(&self) -> usize;
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

fn as_counter(value: &Count) -> &(dyn Counter + 'static) {
    value
}

fn as_counter_mut(value: &mut Count) -> &mut (dyn Counter + 'static) {
    value
}

let mut counter: Rubick<dyn Counter> =
    Rubick::new_projected(Count(41), as_counter, as_counter_mut);
counter.increment();

assert_eq!(counter.value(), 42);
assert!(counter.is_inline());
```

The two adapters should expose the same logical target. `Rubick` invokes an
adapter for every borrow from the concrete value's current location. Named
function items and non-capturing closures are usually zero-sized; capturing
closures or values coerced to function pointers are stored with the concrete
value and count toward the inline capacity.

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

All unsafe code is private to this crate. The implementation maintains these
invariants:

- Inline bytes contain exactly one initialized concrete value whose size and
  alignment fit the buffer.
- Heap storage originates from one `Box<U>` and is destroyed with the same
  concrete layout.
- Projection and drop operations are installed for that exact concrete storage
  type.
- Every borrow derives a fresh pointer from the owner's current storage.
- The concrete value is dropped exactly once, including during panic unwinding;
  only heap mode deallocates storage.

`Rubick` is conservatively neither `Send` nor `Sync`. A projected owner erases
the concrete type, and the private operation table cannot express every auto
trait of that hidden type without restricting useful single-threaded values.

## Limitations and non-goals

- `Rubick` provides exclusive ownership, not `Rc`/`Arc`-style shared ownership.
- It does not provide automatic trait-object unsizing.
- It does not promise a stable address for an unpinned payload.
- It does not currently provide a thread-transferable owner.
- The capacity is fixed rather than configurable per owner.

Avoid wrapping an existing `Box<T>` when allocation avoidance is the goal:

```rust
use aimer_rubick::Rubick;

let nested = Rubick::new(Box::new([0_u8; 128]));

// The small `Box` handle may fit inline, but its array was already allocated.
assert!(nested.is_inline());
```

Construct `Rubick` from the concrete value directly so it can choose inline or
heap storage itself.

Widget and element integration is intentionally deferred until this standalone
pointer has been tested and accepted as stable.