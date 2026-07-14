# Aimer — Development Guide

Instructions for AI coding assistants and human contributors working on the Aimer codebase.

**Never give up on the right solution.**

---

## Project Overview

Aimer is a cross-platform GUI application framework written in Rust, inspired by Flutter's widget
model. It lets you build native user interfaces from a single codebase using a declarative,
composable widget tree (`Container!`, `Row!`, `Column!`, `Text!`, `Button!`, ...).

At its core sits **Cupid**, Aimer's custom high-performance 2D rendering engine, built on
[`wgpu`](https://wgpu.rs). Cupid batches draw calls (rectangles, text, images) and uses hardware
acceleration for rounded corners, borders, clipping, and typography across Metal (Apple), Vulkan /
OpenGL (Linux, Android), D3D (Windows), and WebGPU / WebGL (web).

Key concepts:

- **Declarative UI** — composable widget tree via macros.
- **Stateful widgets** — Flutter-style `StatefulWidget` / `State` with `StateUpdater` for reactive
  rebuilds.
- **Layout engine** — flexbox-inspired `Row`, `Column`, `Container`, `Scrollable`.
- **Cupid** — the wgpu-powered renderer with a `CupidCanvas` API for low-level drawing.

See `README.md` for the current feature milestone and stability markers (`⚠️ Unstable`,
`⛔️ Very Unstable`).

### Repository Layout (Cargo Workspace)

This is a **monorepo** managed as a single Cargo workspace (`resolver = "3"`, `edition = "2024"`).

| Path                     | Purpose                                                          |
|--------------------------|------------------------------------------------------------------|
| `src/`                   | The umbrella `aimer` crate — re-exports the public API.          |
| `aimer_cupid/`           | Cupid rendering engine (wgpu, pipelines, text/glyph rasterizer). |
| `aimer_quiver/`          | Windowing / platform integration layer.                          |
| `crates/aimer_widget`    | Core widget system, element tree, `BuildContext`.                |
| `crates/aimer_container` | Layout containers (`Row`, `Column`, `Scrollable`, spacing).      |
| `crates/aimer_color`     | Color types and named palettes.                                  |
| `crates/aimer_input`     | Gestures and input fields.                                       |
| `crates/aimer_canvas`    | Canvas abstractions.                                             |
| `crates/aimer_attribute` | Shared attributes (dimensions, edges, etc.).                     |
| `crates/aimer_utils`     | Shared utilities.                                                |
| `crates/aimer_animation` | Animation controllers, curves, tweens, keyframes.                |
| `crates/aimer_events`    | Event system.                                                    |
| `crates/aimer_router`    | Navigation / routing.                                            |
| `crates/aimer_inspector` | Widget inspector (unstable).                                     |
| `crates/aimer_assets`    | Asset loading.                                                   |
| `crates/aimer_style`     | Styling primitives.                                              |
| `crates/aimer_text`      | Text layout / typography.                                        |
| `crates/aimer_macro`     | Proc-macros (`#[aimer::main]`, widget macros).                   |
| `crates/aimer_provider`  | Dependency/state provider.                                       |
| `crates/aimer_sdk`       | SDK aggregation.                                                 |
| `dev_tools/aimer_cli`    | The `aimer` CLI binary (create/run/build/doctor/...).            |
| `dev_tools/aimer_lsp`    | Language server tooling.                                         |
| `jaime/`                 | Internal tooling crate.                                          |
| `website/`               | Project website (WASM demo).                                     |
| `aimer_book/`            | Documentation / book.                                            |

> **Large monorepo note:** subprojects may carry their own nested `AGENTS.md`. When working inside a
> subproject, read its local `AGENTS.md` first — it takes precedence over this root file for that
> subproject's specifics.

---

## Golden Rules

- **Use CodeGraph to understand code.** It is fast and always safe for reading/navigating the
  codebase. Prefer it before opening files blindly.
- **Use the IDE (IDEA/CLion) integration to edit code** when connected — it is the safest, fastest
  path for refactors and renames.
- **Never write "Lazy Senior Dev" code.** Do not merely patch the symptom with spaghetti that other
  developers will curse. Solve the actual problem cleanly.
- **Follow Test Driven Development.** Write the failing test first, then the code that makes it pass.

### Test Driven Development

TDD relies on a very short cycle: turn a requirement into a specific, failing test, then write only
the code needed to make it pass, then refactor. Do not add behavior that isn't proven by a test.

**_Before you write code, write the test cases first!_**

---

## Build & Test Commands

Run from the workspace root unless noted. Requires the latest stable Rust toolchain.

### Build

```bash
# Build the entire workspace
cargo build

# Build a single crate
cargo build -p aimer_widget

# Optimized build (release profile: LTO, codegen-units=1, panic=abort, stripped)
cargo build --release
```

### Test

```bash
# Run all tests across the workspace
cargo test

# Test a single crate
cargo test -p aimer_animation

# Run a single test by name
cargo test -p aimer_animation test_curve_linear
```

### CLI (end-user tooling)

The `aimer` binary lives in `dev_tools/aimer_cli`. To run it locally:

```bash
cargo run -p aimer_cli -- <command>   # e.g. create, run, build, assemble, clean, doctor, migrate
```

End users install it via:

```bash
cargo install --git https://github.com/Cottons29/aimer.git aimer_cli --branch nightly-0.0.1
```

---

## Code Style Guidelines

Formatting is enforced by `rustfmt.toml`. Do not hand-format against these rules — run `cargo fmt`.

- **Edition:** Rust 2024.
- **Line width:** `max_width = 140`, `chain_width = 80`.
- **Indentation:** 4 spaces, no hard tabs; Unix newlines.
- **Imports:** grouped `StdExternalCrate` (std → external crates → local), granularity `Module`,
  reordered automatically. Do not manually reorder imports.
- **Heuristics:** `use_small_heuristics = "Max"`.
- **Trailing commas:** `Vertical`.
- **Doc comments:** code inside doc comments is formatted; comments are wrapped.
- **Clippy:** `avoid-breaking-exported-api = false` — the API is pre-1.0 and may change; prefer the
  cleaner design over API stability, but call out breaking changes.

General conventions:

- Match the surrounding code's patterns and idioms; keep changes consistent with the module.
- Crate names use the `aimer_*` prefix; keep new crates consistent.
- Only add comments where the existing code does; avoid noise.
- Prefer the workspace dependency table (`[workspace.dependencies]` in the root `Cargo.toml`) — add
  new deps there and reference them with `<dep>.workspace = true`.

---

## Testing Instructions

- **TDD is mandatory.** Write a failing test that captures the requirement or reproduces the bug
  first; confirm it fails; then implement until it passes.
- Tests live inline as `#[cfg(test)] mod tests` next to the code they cover (the prevailing pattern
  across `crates/*`). Follow that layout.
- For **bug fixes**: add a regression test that fails before the fix.
- For **new features**: cover the happy path, negative/invalid input, and edge cases.
- For **refactors**: rely on existing tests; add coverage only where it is missing.
- **Determinism:** rendering/layout/animation code can be timing- or float-sensitive — seed or fix
  inputs and assert with appropriate tolerances instead of exact floats where relevant.
- **Never** disable, `#[ignore]`, delete, or weaken tests to make a suite pass, and never use skip
  flags. If a test fails, assume your change caused it and fix the root cause.
- Both production and test code must compile before you run tests; fix all compile errors first.

---

## Security Considerations

- **GPU / unsafe code:** Cupid and the platform layers use `wgpu`, `bytemuck`, and platform FFI
  (`objc`, `core-foundation`, `jni`, `ndk`). Keep `unsafe` blocks minimal, localized, and
  documented with the invariants they rely on. Never widen an `unsafe` boundary casually.
- **Buffer/GPU memory:** validate sizes and alignments before uploading to GPU buffers; treat
  `bytemuck` casts as trust boundaries.
- **Networking / TLS:** `reqwest` is configured with `native-tls` and platform verification
  (`rustls-platform-verifier`); do not disable certificate verification.
- **Untrusted input:** fonts, images, and assets (`fontdue`, `ttf-parser`, `image`, `rustybuzz`)
  may come from untrusted sources — handle parse failures gracefully; never `unwrap()` on
  externally supplied data.
- **Secrets:** never hardcode credentials, tokens, or signing keys. Keep them out of the repo and
  out of logs.
- **Dependencies:** prefer vetted, already-used crates; pin new dependencies through the workspace
  table and justify additions.

---

## Commit & Pull Request Guidelines

- Write focused, imperative commit messages (e.g. `Fix glyph atlas overflow on resize`). Group
  related changes; avoid mixing unrelated concerns in one commit.
- Do **not** commit on your own initiative — only when explicitly asked. When you do commit, add
  Junie as co-author:

  ```bash
  git commit --trailer "Co-authored-by: Junie <junie@jetbrains.com>"
  ```

- Before opening a PR: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` must all pass.
- Describe **what** changed and **why** in the PR, note any stability impact (link to the milestone
  markers in `README.md`), and mention affected platforms if the change is renderer/platform
  specific.
- Never submit with a failing build or failing tests.

---

## Extra Notes for New Contributors

- Platform prerequisites: macOS/iOS need Xcode + Metal hardware; Android needs the NDK; web needs
  `trunk`.
- The public entry point is `aimer` (`src/lib.rs`), which re-exports the sub-crates — prefer routing
  new public API through it.
- The render profile in release is aggressive (`lto = true`, `panic = "abort"`, `strip = symbols`);
  don't rely on unwinding for control flow in production paths.
- Anything you'd tell a new teammate — a subtle invariant, a flaky platform, a large asset — belongs
  here or in the relevant nested `AGENTS.md`.
