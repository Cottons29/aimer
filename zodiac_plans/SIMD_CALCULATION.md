# SIMD calculation plan

## Decision

Use SIMD selectively inside numeric kernels. Do not make the `Widget`,
`Element`, reconciliation, or event interfaces SIMD-aware.

The framework contains two different kinds of work:

- Data-parallel numeric work, where SIMD may help: sizes, offsets, weights,
  interpolation, geometry, colors, and image data.
- Irregular tree work, where SIMD is usually the wrong abstraction: dynamic
  child measurement, trait-object dispatch, retained-child traversal, state,
  input, focus, routing, providers, and text shaping.

The scalar implementation remains the correctness baseline for every target.
An optimized kernel is adopted only after a benchmark proves that it improves a
real frame path.

## Goals

- [ ] Identify the numeric kernels that consume enough data to justify SIMD.
- [ ] Preserve the existing public widget and element interfaces.
- [ ] Keep scalar fallbacks for unsupported CPU and WebAssembly targets.
- [ ] Preserve layout behavior, ordering, precision, and invalid-input handling.
- [ ] Measure CPU time, allocations, frame time, and code-size impact before and
      after each optimization.

## Non-goals

- [ ] SIMD-ify every loop in the framework.
- [ ] Replace tree traversal or dynamic widget dispatch with a vector API.
- [ ] Use `target-cpu=native` for distributable binaries.
- [ ] Add unsafe architecture intrinsics before a scalar benchmark establishes a
      meaningful target.
- [ ] Move GPU work onto the CPU; the renderer already delegates parallel work
      to GPU pipelines where appropriate.

## Candidate areas

### 1. Flex layout

Primary target: the pure numeric part of `aimer_flex`.

- [ ] Benchmark `distribute_flex_space` in
      [flex_child.rs](crates/aimer_flex/src/flex/flex_child.rs), including zero,
      negative, sparse, uniform, and large weight arrays.
- [ ] Benchmark size scanning and uniform detection in
      [flex_layout.rs](crates/aimer_flex/src/flex/flex_layout.rs).
- [ ] Separate child measurement from numeric table construction. Calls to
      `child.computed_size(...)` remain scalar and dynamic.
- [ ] Consider structure-of-arrays storage for main-axis sizes, cross-axis
      sizes, and flex weights if profiling shows that `ResolvedSize`'s current
      array-of-structures layout limits vectorization.
- [ ] Keep prefix-offset accumulation scalar unless a benchmark shows that a
      tiled/prefix-scan implementation pays for its complexity.
- [ ] Treat wrapping in [wrap_layout.rs](crates/aimer_flex/src/flex/wrap_layout.rs)
      as branch-heavy and dependency-heavy; optimize it only after simpler
      kernels are measured.

### 2. Scroll and animation math

- [ ] Profile scroll physics and overscroll calculations in
      `crates/aimer_scroll/src/scrollable/physics/`.
- [ ] Profile curve evaluation, tweening, and interpolation in
      `crates/aimer_animation/src/primitives/`.
- [ ] Batch independent values where the existing APIs already operate on
      collections; do not vectorize stateful controller/event transitions.
- [ ] Preserve the framework's current floating-point tolerances and frame
      scheduling behavior.

### 3. Geometry, colors, and CPU-side render preparation

- [ ] Profile repeated matrix, rectangle, transform, color, and clip math in
      `aimer_attribute`, `aimer_color`, `aimer_space`, and `aimer_cupid`.
- [ ] Look for batches of plain numeric values before introducing a SIMD type.
- [ ] Keep canvas/FFI calls and GPU command submission scalar and ordered.
- [ ] Validate buffer sizes, alignment, ownership, and thread affinity if a
      SIMD representation crosses a GPU or FFI seam.

### 4. Image and bulk data operations

- [ ] Profile image conversion, blending, and pixel-format operations before
      adding a kernel.
- [ ] Prefer existing optimized image/GPU paths when they already dominate the
      operation.
- [ ] Provide scalar handling for tails, empty buffers, unaligned data, and
      unsupported formats.

## Kernel design

Keep a small private seam between framework policy and numeric implementation:

```text
Widget/Element policy
        |
        v
validated, contiguous numeric inputs
        |
        +--> scalar kernel
        |
        +--> optional SIMD kernel
        |
        v
validated numeric result
```

- [ ] Keep kernel inputs simple and contiguous: slices, lengths, strides, and
      plain numeric values.
- [ ] Make kernels deterministic and independently testable without a window,
      canvas, or widget tree.
- [ ] Prefer compiler auto-vectorization first. The release profile already
      enables optimization and LTO.
- [ ] If explicit SIMD is justified, add target-specific implementations behind
      a private dispatch layer and retain the scalar implementation.
- [ ] Use runtime feature detection or compile-time target selection only where
      the deployment target guarantees the required instructions.
- [ ] Keep `unsafe` limited to the smallest intrinsic wrapper and document its
      alignment, bounds, and target-feature invariants.

## Target support

- [ ] Native desktop: provide a scalar baseline and optional SSE2/AVX2 or NEON
      paths where supported by the deployment policy.
- [ ] Android/iOS: validate the selected ARM SIMD path on the minimum supported
      devices; do not assume desktop CPU features.
- [ ] WebAssembly: keep a non-SIMD build and an explicitly opted-in `simd128`
      build. Do not make SIMD a requirement for browsers that lack support.
- [ ] Avoid making `portable_simd` a default dependency while it remains an
      unstable Rust API.

## Implementation phases

### Phase 0 — Baseline and profiling

- [ ] Add focused benchmarks for flex distribution, size-table construction,
      wrapping, scroll physics, and animation primitives.
- [ ] Measure representative small, medium, and large child counts.
- [ ] Record p50/p95 CPU time, allocations, frame time, and binary size.
- [ ] Capture profiles for cold layout, cached layout, scrolling, resizing, and
      animated layout.

### Phase 1 — SIMD-friendly data flow

- [ ] Remove avoidable allocations and conversions from measured hot paths.
- [ ] Separate dynamic child measurement from pure numeric post-processing.
- [ ] Introduce contiguous numeric buffers only where the profile justifies
      their memory cost.
- [ ] Confirm that the scalar refactor is behaviorally identical.

### Phase 2 — First optimized kernel

- [ ] Implement and benchmark the best-supported candidate, expected to be flex
      weight distribution or a size reduction.
- [ ] Compare compiler auto-vectorization against an explicit kernel.
- [ ] Add scalar/SIMD parity tests with tolerances appropriate to each result.
- [ ] Reject the change if the gain disappears in an end-to-end frame profile.

### Phase 3 — Additional numeric domains

- [ ] Apply the same measurement gate to scroll physics and animation math.
- [ ] Apply it to geometry/color/image operations only when they show up in
      profiles.
- [ ] Keep each kernel's optimization independent so one unsupported target does
      not disable unrelated framework features.

### Phase 4 — Cross-target rollout

- [ ] Add native feature dispatch and scalar fallback tests.
- [ ] Add WebAssembly SIMD and non-SIMD build checks where supported by the
      toolchain.
- [ ] Verify deterministic layout results within documented floating-point
      tolerances across target implementations.
- [ ] Check code size, startup time, and battery/thermal impact on mobile.

### Phase 5 — Adoption and maintenance

- [ ] Document the benchmark that justifies each SIMD kernel.
- [ ] Keep a scalar reference implementation beside each optimized kernel.
- [ ] Re-run the benchmark suite when layout data structures or target support
      changes.
- [ ] Remove an optimized path if it no longer provides measurable benefit.

## Acceptance criteria

The SIMD work is complete only when:

- [ ] Every optimized kernel has a representative benchmark and scalar parity
      tests.
- [ ] The end-to-end frame path improves on a measured workload, not just a
      microbenchmark.
- [ ] All supported targets retain a working scalar fallback.
- [ ] Layout positions, sizes, hit testing, scrolling, animation timing, and
      rendering order remain correct.
- [ ] No public widget or element API requires callers to know about SIMD.
- [ ] The final documentation records the selected targets, dispatch strategy,
      benchmark result, and known numerical limitations.
