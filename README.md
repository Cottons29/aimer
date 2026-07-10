# Aimer

A cross-platform UI framework built with Rust, inspired by Flutter's widget model. Aimer lets you build native user
interfaces from a single codebase using a declarative, composable widget tree.

```rust
#[aimer::main]
pub fn start_app() {
    AimerApp::start(
        Container!(
            child: Text!(
                "Hello World!",
                text_align: text::TextAlign::MidCenter,
                text_style: TextStyle!(
                    color: Colors::Black,
                )
            )
        )
    );
}
```

## Cupid

Cupid is Aimer's high-performance, cross-platform 2D rendering engine. It provides the foundation for drawing the widget
tree on native platforms.

- **WGPU-powered** — Uses `wgpu` to provide a consistent rendering API across Metal, Vulkan, and DirectX.
- **Batched Rendering** — Automatically batches draw calls (rectangles, text, images) to minimize GPU overhead.
- **Hardware Acceleration** — Fully utilizes the GPU for effects like rounded corners, borders, and complex clipping.
- **Canvas-like API** — Simple and intuitive `CupidCanvas` API for lower-level drawing operations.
- **High-Quality Typography** — Integrated text layout and glyph rasterization for crisp text at any scale.

## Features

- **Declarative UI** — Build interfaces with a composable widget tree using macros (`Container!`, `Row!`, `Column!`,
  `Text!`, `Button!`, etc.).
- **Stateful Widgets** — Flutter-style `StatefulWidget` / `State` pattern with `StateUpdater` for reactive rebuilds.
- **Animation System** — `AnimationController` with configurable duration, curves (`EaseIn`, `EaseOut`, `Bounce`, etc.),
  and effects (`Opacity`, `Scale`, `Translate`, `Rotate`, `SlideX`, `SlideY`). `⚠️ Unstable `
- **Layout Engine** — Flexbox-inspired layout with `Row`, `Column`, `Scrollable`...
- **Cross-Platform Rendering** — Cupid on native platforms (Metal on Apple, Dx3D on Windoes and Vulkan/OpenGl for Linux
  and Android) and WebGpu/WebGl on the web.
- **CLI Tooling** — `Aimer` a cli tool for creating running and builds projects.

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) version based on the Aimer version:
  - example : Aimer version = 1.91.0, Rust version also required to 1.91.1

- Platform-specific dependencies:
    - **macOS / iOS**: Xcode and Metal-compatible hardware
    - **Android**: Android NDK
    - **Web**: `trunk`


### Installation

```bash
cargo install --git https://github.com/Cottons29/aimer.git aimer_cli --branch nightly-0.0.1
````

### Create a New Project

```bash
aimer create my_aimer
```

### Running the App

```bash
cd my_aimer && Aimer run
```

## Milestone

- [x] Aimer CLI
    - [x] `create` to create a new project
    - [x] `run` to run the project
    - [x] `assemble` to build platform artifact like app, apk...
    - [x] `clean` to clean the artifact and builds.
    - [x] `migrate` migrate the scaffold from a low version to a high version
    - [x] `doctor` for checking the development environment.
    - [x] `build` to build the project
    - [x] shell completion
    - [x] project scaffolding (`Aimer new`)
    - [x] auto restart app
    - [ ] widget inspector `⛔️ Very Unstable`
- [x] Core widget system
    - [x] `StatefulWidget` / `State` pattern
    - [x] `Element` tree and `BuildContext`
    - [x] `StateUpdater` for reactive rebuilds
    - [x] Widget macros (`Container!`, `Row!`, `Column!`, `Text!`, `Button!`, etc.)
- [x] Layout engine
    - [x] `Row` and `Column` (flexbox-inspired)
    - [x] `Container` with padding, margin, and decoration
    - [x] `Scrollable` with scroll bar support `⚠️ Unstable`
    - [x] `Spacing` and `LayoutSpacing` attributes `⚠️ Unstable`
    - [x] `BoxAlignment` (start, center, end, stretch) `⚠️ Unstable`
- [x] Basic controls
    - [x] `Button` with press handler and hover/style variants
    - [x] `GestureDetector` `⚠️ Unstable`
    - [x] `InputField` (text field) `⚠️ Unstable`
    - [ ] `Checkbox`
    - [ ] `Switch` / `Toggle`
    - [ ] `Slider`
    - [ ] `DropdownMenu` / `Select`
    - [ ] `Radio` button
- [x] Text
    - [x] `Text` widget with `TextStyle` (font size, color)
    - [x] `TextAlign` (left, center, right)
    - [ ] Rich text (inline spans, mixed styles)
    - [ ] Custom font loading
- [x] Color system
    - [x] Named color palettes (`Color::BLUE`, `Color::GRAY`, etc.)
- [x] Animation framework (Experiemental)
    - [ ] `AnimationController` (forward, reverse, repeat, auto-reverse) `⛔️ Very Unstable`
    - [ ] Curves (`EaseIn`, `EaseOut`, `EaseInOut`, `Bounce`, `Linear`, etc.) `⛔️ Very Unstable`
    - [ ] `Animated` widget with effects (`Opacity`, `Scale`, `Translate`, `Rotate`, `SlideX`, `SlideY`)
      `⛔️ Very Unstable`
    - [ ] Enter and exit (delete) transitions `⛔️ Very Unstable`
- [x] Cross-platform support
    - [x] macOS (Cupid) `⚠️ Unstable`
    - [x] iOS (Cupid) `⚠️ Unstable`
    - [x] Android (Cupid) `⚠️ Unstable`
    - [x] Web / WASM (Cupid) `⚠️ Unstable`
    - [ ] Windows (Cupid)
    - [ ] Linux (Cupid)

- [ ] Gesture system
    - [x] Tap, double-tap `⚠️ Unstable`
    - [ ] Drag and pan
    - [ ] Swipe
    - [x] Long press `⚠️ Unstable`
- [ ] Navigation and routing
    - [x] Navigator / route stack `⚠️ Unstable`
    - [x] Named routes (typed path + query parameters) `⚠️ Unstable`
    - [x] Redirects & guards `⚠️ Unstable`
    - [x] Nested & Shell routes (`Shell` / `Outlet`) `⚠️ Unstable`
    - [x] StatefulShellRoute (per-branch history stacks) `⚠️ Unstable`
    - [ ] Page transitions
- [ ] Theming and dark mode
    - [ ] Theme data (colors, typography, spacing)
    - [ ] Dark / light mode switching
    - [ ] Custom theme support

> ⚠️ Unstable — feature is implemented but may have breaking changes or incomplete edge cases.
>
> ⛔️ Very Unstable – feature is implemented, but the functionality is not stable and has some critical bug that can
> break the app.
>
> ❌ Not Implement Yet — feature is not implemented but may implement in the future.
>
