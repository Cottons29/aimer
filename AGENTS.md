# Aimer — Development Instructions

These instructions apply to the entire repository. A nested `AGENTS.md` takes precedence for files in its directory.

## Priorities

When requirements compete, use this order:

1. Correctness and safety.
2. The user's explicit request and acceptance criteria.
3. Performance on measured or clearly hot GUI paths.
4. Consistency with the surrounding module.
5. Minimal complexity and dependency cost.

Do not stop at a superficial workaround. Find and fix the root cause while keeping the change within the requested
scope.

## Before Editing

- Read the relevant implementation, tests, and any nested `AGENTS.md` before changing code.
- Use available IDE context or Codegraph MCP when it is relevant, but do not assume an IDE integration exists. CLI
  inspection is valid.
- Do not invent missing facts. First inspect the repository, compiler output, tests, or documentation. Ask the user only
  when the ambiguity cannot be resolved locally and different answers would materially change the implementation.
- Preserve unrelated user changes. Do not revert or overwrite them.
- Match existing module patterns unless this file or the user explicitly requires otherwise.

## Scope and Autonomy

- For requests to explain, review, diagnose, or plan: inspect and report; do not modify code unless asked.
- For requests to build, change, or fix: make the in-scope local edits and run relevant non-destructive checks without
  asking first.
- Ask before destructive actions, external writes, adding a substantial dependency, changing public API beyond the
  request, or materially expanding scope.
- Do not create commits, branches, pull requests, or publish artifacts unless the user explicitly asks. Pull requests
  must be created by a human unless the user overrides this rule.

## Test-Driven Development

Use red-green-refactor for behavior changes:

1. Add a focused test that fails for the intended reason.
2. Run it and confirm the failure.
3. Implement the smallest complete production change that makes it pass.
4. Run the focused test again, then the relevant crate tests.
5. Refactor only while the tests remain green.

Additional test rules:

- Bug fixes require a regression test.
- New behavior must cover its happy path plus relevant invalid-input and boundary cases.
- Pure refactors may rely on existing tests when they already cover the affected behavior.
- Prefer inline `#[cfg(test)] mod tests` modules next to the code they cover, matching the prevailing crate pattern.
- Keep tests deterministic. Fix time and random inputs; use tolerances for floating-point assertions where appropriate.
- Never delete, ignore, weaken, or skip a test merely to make the suite pass.
- If a pre-existing failure is unrelated, report it with evidence; do not silently work around it.
- If a useful test is impractical, explain why before implementing and use the strongest feasible verification.

## Rust and API Design

- Use the latest stable Rust toolchain supported by the workspace.
- Prefer clear ownership and borrowing over allocation. Apply zero-copy techniques when they simplify or measurably
  improve a hot path; do not introduce unsafe code or complex lifetimes without demonstrated benefit.
- Keep `unsafe` blocks minimal and document their safety invariants.
- Never hardcode or log credentials, tokens, signing keys, or other secrets.
- Prefer existing workspace dependencies and standard-library solutions. Add a third-party crate only when its
  maintenance, correctness, or complexity benefit justifies it.
- Prefer the crate version that published more than 1 ~ 2 weeks ago to prevent malicious crates from being introduced.
- Prefer using the git url with a specific commit hash or tag for introduce external crate when possible.
- Declare shared dependencies in root `[workspace.dependencies]`, then use `<dependency>.workspace = true` in member
  crates.
- New crate names use the `aimer_*` prefix.
- Route public umbrella-crate re-exports through `src/lib.rs`. Use `pub use aimer_xxxx as xxxx` when exposing a crate
  under a shorter public name.
- Keep source files at or below 2,000 lines (inline-unittest excluded). Split by responsibility before exceeding that
  limit. Prefer named module, the unittest is not count toward source LOD files such as `gesture.rs` plus
  `gesture/drag.rs`; do not introduce `mod.rs` files.
- Add documentation comments to new public APIs. Document purpose, important invariants, panics/errors, safety
  requirements, and a useful example when appropriate. Follow Rust standard-library style, but keep documentation
  proportional to the API.
- Add comments for invariants and non-obvious decisions, not for code that is already self-explanatory.
- Do not run `cargo fmt`. Preserve the surrounding formatting style in edited code.

## Widget Conventions

For structs that implement `Widget`:

- Provide a zero-argument `new()` that returns the incomplete/default builder state.
- If a child is required, make the child-setting method the final type-state transition that produces a valid widget.
- Put the child type parameter last when practical and consistent with the existing API.
- Mark small builder methods `#[inline]`.
- In `Widget::to_element(self, ctx)`, move owned fields into the retained element. Do not clone fields merely because
  ownership is inconvenient: `self` is consumed.
- A self-rebuilding widget should retain its child through `ChildBuilder` or the repository's equivalent retained-child
  mechanism instead of rebuilding or cloning it unnecessarily.

Example:

```rust
pub struct MyWidget<W = RequiredChild> {
    size: f32,
    child: W,
}

impl MyWidget {
    #[inline]
    pub fn new() -> Self {
        Self {
            size: 0.0,
            child: RequiredChild,
        }
    }

    #[inline]
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    #[inline]
    pub fn child<W: Widget>(self, child: W) -> MyWidget<W> {
        MyWidget {
            size: self.size,
            child,
        }
    }
}

impl<W: Widget + 'static> Widget for MyWidget<W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        RawMyWidget {
            size: self.size,
            child: self.child.to_element(ctx),
        }
            .boxed()
    }
}
```

## Performance

Aimer is a performance-sensitive GUI framework.

- Treat per-frame rendering, layout, input, animation, and widget rebuild paths as hot unless evidence shows otherwise.
- Avoid avoidable allocations, clones, repeated tree walks, blocking work, and unnecessary synchronization on hot paths.
- Do not claim a change is faster without evidence. For non-obvious performance changes, add or run a benchmark,
  profiler, allocation check, or equivalent measurement.
- Do not trade correctness or maintainability for speculative micro-optimizations.
- When changing GPU buffers, FFI, or platform code, validate sizes, alignments, lifetimes, and thread-affinity
  assumptions.

## Visual Design

When the user asks for an example UI without specifying a palette, prefer a monochrome black-and-white theme. An
explicit user-supplied design or existing product style takes precedence.

## Validation Commands

Run commands from the workspace root unless a nested guide says otherwise.

```bash
# Focused test
cargo test -p aimer_animation test_curve_linear

# Crate tests
cargo test -p aimer_animation

# Entire workspace (use when the change scope warrants it)
cargo test --workspace --all-features
```

Before handing off:

- Ensure production and test code compile.
- Run the narrowest relevant checks first, then broaden according to change risk.
- Report exactly what was run and whether it passed.
- Report any checks not run and the reason.
- Summarize changed behavior and identify remaining risks or follow-up work without claiming unverified success.
