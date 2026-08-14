# Aimer

Aimer (pronounced "aa·may" it's mean "to love" in french) is a cross-platform UI framework built with Rust, inspired by Flutter's widget model. Aimer lets you build native user
interfaces from a single codebase using a declarative, composable widget tree.

```rust
#[aimer::main]
fn main() {
  AimerApp::start(
    Container::new()
      .child(
        Text::new("Hello World!")
          .text_align(TextAlign::MidCenter)
          .text_style(TextStyle::new().color(Color::WHITE))
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

- **Declarative UI** — Build interfaces with a composable widget tree using a fluent builder pattern
  (`Container::new().child(...)`, `Row::new().children(...)`, `Text::new("...")`, etc.).
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
  - example : Aimer version = 1.97.1, Rust version also required to 1.97.1

- Platform-specific dependencies:
    - **macOS / iOS**: Xcode and Metal-compatible hardware
    - **Android**: Android NDK
    - **Web**: `trunk`


### Installation

```bash
cargo install --git https://github.com/Cottons29/aimer.git aimer_cli --branch 1.97.1-alpha-2
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
    - [x] project scaffolding
    - [x] auto restart app
    - [ ] widget inspector `⛔️ Very Unstable`
- [x] Core widget system
    - [x] `StatefulWidget` / `State` pattern
    - [x] `Element` tree and `BuildContext`
    - [x] `StateUpdater` for reactive rebuilds
    - [x] Widget builder pattern (`Container::new()`, `Row::new()`, `Column::new()`, `Text::new()`, `Button::new()`, etc.)
- [x] Layout engine
    - [x] `Row` and `Column` (flexbox-inspired)
    - [x] `Container` with padding, margin, and decoration
    - [x] `Scrollable` with scroll bar support 
    - [x] `Spacing` and `LayoutSpacing` attributes
    - [x] `BoxAlignment` (start, center, end, stretch)
- [x] Basic controls
    - [x] `Button` with press handler and hover/style variants
    - [x] `GestureDetector`
    - [x] `TextField` / `TextArea`
    - [ ] `Checkbox`
    - [ ] `Switch` / `Toggle`
    - [ ] `Slider`
    - [x] `DropdownMenu` / `Select`
    - [ ] `Radio` button
- [x] Text
    - [x] `Text` widget with `TextStyle` (font size, color)
    - [x] `TextAlign` (left, center, right)
    - [x] Rich text (inline spans, mixed styles)
    - [x] Custom font loading
- [x] Color system
    - [x] Named color palettes (`Color::BLUE`, `Color::GRAY`, etc.)
- [x] Animation framework
    - [x] `AnimationController` (forward, reverse, repeat, auto-reverse)
    - [x] Curves (`EaseIn`, `EaseOut`, `EaseInOut`, `Bounce`, `Linear`, etc.) 
    - [x] `Animated` widget with effects (`Opacity`, `Scale`, `Translate`, `Rotate`, `SlideX`, `SlideY`)
    - [x] Enter and exit (delete) transitions
- [x] Cross-platform support
    - [x] macOS (Cupid) 
    - [x] iOS (Cupid)
    - [x] Android (Cupid) 
    - [x] Web / WASM (Cupid)
    - [x] Windows (Cupid) 
    - [x] Linux (Cupid)

- [ ] Gesture system
    - [x] Tap, double-tap
    - [x] Drag and pan `⚠️ Unstable`
    - [ ] Swipe
    - [x] Long press `⚠️ Unstable`
- [ ] Drag and drop
    - [x] `Draggable` / `DragTarget<T>` with typed payloads — see `jaime/src/drag_and_drop.rs`
    - [x] Feedback painted above every clip boundary, with spring-back on a refused drop `⚠️ Unstable`
    - [x] `DropZone` for files dragged in from the desktop — see `jaime/src/file_drop_zone.rs`
    - [ ] Auto-scroll when dragging near the edge of a `Scrollable`
    - [ ] Reorderable lists
    - [ ] File drop on the web (winit's web backend emits no file-drag events)
- [x] Navigation and routing
    - [x] Navigator / route stack
    - [x] Named routes (typed path + query parameters)
    - [x] Redirects & guards
    - [x] Nested & Shell routes (`Shell` / `Outlet`) 
    - [x] StatefulShellRoute (per-branch history stacks) 
    - [x] Page transitions
- [x] Theming and dark mode
    - [x] Theme data (colors, typography, spacing)
    - [x] Dark / light mode switching
    - [x] Custom theme support
- [ ] Accessibility
  - [ ] **Semantic structure** 
  - [ ] **Keyboard / focus navigability**
  - [ ] **Contrast & non-color cues** 
  - [ ] **Text scaling**
  - [ ] **Dynamic content announcements**
  - [ ] **Respect system preferences** 
  - [ ] **Touch target sizing**

> ⚠️ Unstable — feature is implemented but may have breaking changes or incomplete edge cases.
>
> ⛔️ Very Unstable – feature is implemented, but the functionality is not stable and has some critical bug that can
> break the app.
>
> ❌ Not Implement Yet — feature is not implemented but may implement in the future.
>
