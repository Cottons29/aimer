use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use aimer_rubick::Rubick;

const ITERATIONS: usize = 1_000_000;
const ROUNDS: usize = 7;

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every operation is forwarded to the system allocator with the same
// pointer and layout. The counter does not affect allocation behavior.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller provides the layout required by `GlobalAlloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was returned by the system allocator for `layout`.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Clone, Copy)]
struct Small([u64; 2]);

#[derive(Clone, Copy)]
struct Large([u64; 8]);

fn measure<T>(mut construct: impl FnMut() -> T) -> (Duration, usize) {
    for _ in 0..10_000 {
        black_box(construct());
    }

    ALLOCATIONS.store(0, Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(construct());
    }
    let elapsed = start.elapsed();
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    (elapsed, allocations)
}

fn median<T>(mut construct: impl FnMut() -> T) -> (Duration, usize) {
    let mut samples = Vec::with_capacity(ROUNDS);
    let mut allocations = 0;
    for _ in 0..ROUNDS {
        let (elapsed, count) = measure(&mut construct);
        samples.push(elapsed);
        allocations = count;
    }
    samples.sort_unstable();
    (samples[ROUNDS / 2], allocations)
}

fn report(name: &str, elapsed: Duration, allocations: usize) {
    let nanoseconds = elapsed.as_nanos() as f64 / ITERATIONS as f64;
    println!(
        "{name:<20} {nanoseconds:>8.2} ns/op, {allocations:>7} allocations per {ITERATIONS} owners"
    );
}

fn main() {
    if cfg!(debug_assertions) {
        panic!("run this benchmark with --release");
    }

    println!("Rubick ownership microbenchmark: {ITERATIONS} iterations, {ROUNDS} rounds");
    println!(
        "owner sizes: Box<Small>={} B, Rubick<Small>={} B",
        size_of::<Box<Small>>(),
        size_of::<Rubick<Small>>()
    );

    let (elapsed, allocations) = median(|| Box::new(black_box(Small([1, 2]))));
    report("Box small", elapsed, allocations);

    let (elapsed, allocations) = median(|| Rubick::new(black_box(Small([1, 2]))));
    report("Rubick inline", elapsed, allocations);

    let (elapsed, allocations) = median(|| Box::new(black_box(Large([1; 8]))));
    report("Box large", elapsed, allocations);

    let (elapsed, allocations) = median(|| Rubick::new(black_box(Large([1; 8]))));
    report("Rubick heap", elapsed, allocations);

    black_box(Small([1, 2]).0);
    black_box(Large([1; 8]).0);
}
