# Parallel Text Preparation, Smarter Caches, and Framework-Level Modal Widgets

Two parts of Aimer's interface system have received important updates. Cupid can now prepare cold text
in parallel and reuse persistent caches across frames, while applications can present animated modal
content above the entire app without manually building an overlay into every page.

These changes solve different visual problems, but they share the same goal: application code should
describe the intended result while the framework handles the rendering and lifecycle details.

## Preparing Text as Batches

Text reaches the GPU only after several CPU-heavy stages. Cupid shapes Unicode text into glyph runs,
lays those runs out for the requested wrapping width, and rasterizes glyphs that are not already in an
atlas. Doing that work one text node at a time makes a cold frame pay each cost serially.

Cupid now collects missing work into three preparation batches:

1. shaping jobs turn unique text and style combinations into glyph runs
2. layout jobs position shaped runs for a wrapping width
3. glyph jobs rasterize unique glyphs whose metrics or bitmap are still missing

`PreparationBatch` deduplicates jobs by key while preserving their first-seen order. The renderer can
therefore prepare one result for repeated labels or glyphs and still merge every result deterministically
before creating GPU instances.

## Bounded Native Parallelism

On native platforms, batches of four or more jobs run through one process-wide Rayon thread pool. The
pool reserves one logical processor for the application when possible and caps text workers at four on
desktop or two on Android and iOS. Small batches stay serial, avoiding thread-pool overhead when direct
iteration is cheaper.

Each worker receives an independent `GlyphPreparationContext`. Font bytes and parsed font records are
shared through reference-counted snapshots, but shaping buffers, lookup caches, and bitmap state remain
worker-local. No worker touches the GPU atlas, canvas state, or renderer caches. Only after an entire
batch succeeds does the render thread validate its order and commit the results.

WebAssembly uses the same owned job/result contract with serial execution. One-worker systems and a
failed pool construction also fall back to that path, so parallelism changes throughput rather than
rendering behavior.

## Cache the Right Stage

Parallel work improves cold batches; caches keep warm frames from repeating them. **Cupid** separates the
caches because each stage has different invalidation inputs:

- the shaping cache is keyed by text, font size, family, style, and weight, because shaping does not
  depend on the available width
- the layout cache adds the wrapping width, so the same shaped run can be reused at a new width while
  only line placement is recomputed
- the rasterizer caches glyph indices, advance widths, font bytes, HarfRust shaper metadata, and
  rasterized glyph descriptors
- alpha and color glyph atlases map `GlyphKey` values to GPU texture regions, allowing repeated glyphs
  to skip rasterization and upload.

The shaping and layout caches persist across frames, scrolling, animation, and screen transitions.
Both are bounded at 4,096 entries and are cleared only after crossing that hard limit, rather than when
the visible text-node count changes. Glyph-index lookup is capped at 16,384 entries, and retained CPU
bitmap data is limited to 8 MiB. Once a bitmap reaches the GPU atlas, Cupid releases its bytes but keeps
the lightweight descriptor needed for layout and future atlas decisions.

The glyph atlas starts at 512 by 512 pixels and grows up to 2,048 by 2,048. Growth preserves existing
regions with a GPU texture copy. At the cap, overflow resets and repacks the atlas instead of allowing
GPU memory to grow without a bound; pending upload bytes are also discarded immediately after upload,
so Cupid does not retain a full CPU-side mirror of the texture.

## Warm-Up and Measurement

Applications that know their common text can warm the pipeline deliberately. `warm_text` pre-shapes and
lays out a string for a specified width and populates its glyph atlas entries. `warm_glyph_set` prepares
common characters at selected font sizes, which helps dynamic strings such as counters and usernames
avoid first-use rasterization.

Cupid also includes `text_shaping_benchmark`, which compares shaping one grapheme cluster at a time with
shaping a complete run and compares cold serial batches with cold parallel batches. It prints timings
and computed speedups for the current machine; performance claims should come from measured benchmark
and application traces rather than from a fixed number that may not match another platform.

## A Modal Above the Whole Application

A dialog should not be constrained by the page, route, or container that opened it. It needs a
viewport-wide barrier, must draw above the current application, and must prevent pointer and scroll
events from reaching widgets behind it.

Aimer now installs a `ModalHost` around the application root automatically. The host remains inside the
single widget and render tree, so modal content uses the same drawing, resize, scale, event, headless,
and reconciliation paths as the rest of the app. Applications do not need to add a `Stack` to every
screen or model a dialog as a navigation route.

The public `Modal` builder follows Aimer's child-last convention:

```rust
use std::time::Duration;

use aimer::style::{Color, TextAlign, TextStyle};
use aimer::{Container, Modal, ModalAnimation, Text};

let dialog = Container::new()
    .width(420)
    .height(220)
    .child(
        Text::new("Changes saved")
            .text_align(TextAlign::MidCenter)
            .text_style(TextStyle::new().font_size(20)),
    );

let handle = Modal::new()
    .barrier_color(Color::BLACK.with_opacity(115))
    .animation(
        ModalAnimation::new()
            .enter_duration(Duration::from_millis(240))
            .exit_duration(Duration::from_millis(160)),
    )
    .child(dialog)
    .show();
```

`show()` presents through the framework-level host immediately and returns a `ModalHandle`. A call made
before the first application frame is queued until the root host is built, which makes startup modals
possible without a special mount callback.

When the operation completes, dismiss that specific entry through its handle:

```rust
handle.dismiss();
```

Dismissal is idempotent. The first call begins dismissal and later calls are harmless. For flows that
do not retain a handle, `ModalController::dismiss_top()` targets the topmost modal.

## Barrier, Keyboard, and Event Behavior

By default, a modal is centered over a 45%-opaque black barrier. Pressing the barrier or the Escape key
dismisses the top entry. Both policies are configurable:

```rust
let handle = Modal::new()
    .barrier_dismissible(false)
    .escape_dismissible(false)
    .child(dialog)
    .show();
```

A non-dismissible barrier still blocks input. Dismissibility controls lifecycle behavior; it does not
allow background widgets to receive events. Content is visited before the barrier, while stacked modal
entries are processed topmost-first. Opening a modal also cancels an interaction already captured by
the background tree, preventing a drag or press from continuing through the new overlay.

Modal alignment is configurable with the same viewport alignment values used by Aimer containers. The
default is `Alignment::MidCenter`, while values such as `TopCenter` or `BotRight` can place sheets and
other overlay styles without changing the host architecture.

## Paint-Only Enter and Exit Animation

`ModalAnimation` provides a subtle fade-and-scale transition. Its defaults are a 200 millisecond
`EaseOut` entrance, a 150 millisecond `EaseIn` exit, and content scaling from `0.96` to `1.0`.

The animation changes painting, not layout. The barrier fades, and the content fades and scales around
its center while retaining its final measured bounds. Stable bounds keep hit testing predictable
throughout both entrance and exit. The modal also remains in the overlay and continues blocking
background input until its exit animation finishes.

Callers can configure durations, curves, and the initial content scale:

```rust
use aimer::animation::Curve;

let animation = ModalAnimation::new()
    .enter_curve(Curve::FastOutSlowIn)
    .exit_curve(Curve::EaseIn)
    .content_scale_from(0.9);
```

Animation is opt-in. Leaving `.animation(...)` out presents and removes the modal without a transition.

## Current Scope and Next Steps

The first framework-level modal API focuses on presentation: viewport-wide composition, ordered modal
entries, input blocking, barrier and Escape dismissal, stable handles, and reversible enter and exit
animation. Native and headless application startup both install the host, which keeps testing aligned
with production behavior.

Typed values returned from dialogs and full focus trapping or restoration are not part of this first
version. Those features need explicit lifecycle contracts rather than being hidden inside the visual
widget. The current separation gives them a clean place to grow: `Modal` describes appearance,
`ModalHost` owns the global overlay, and `ModalHandle` controls the lifetime of one presented entry.

Together, the text and modal updates make common interface code more declarative. Text alignment now
matches the lines users actually see, and modal content can sit above the whole application without
forcing each screen to recreate framework-level overlay behavior.