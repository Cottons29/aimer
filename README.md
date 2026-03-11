# Oxidize

A cross-platform UI framework built with Rust, inspired by Flutter's widget model. Oxidize lets you build native user interfaces from a single codebase using a declarative, composable widget tree.

```
#[oxidize::main]
pub fn start_app() {
    OxidizeApp::start(
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

## Supported Platforms

| Platform   | Rendering Backend    | Status            |
|------------|----------------------|-------------------|
| macOS      | Skia (Metal)         | ✅ Supported       |
| iOS        | Skia (Metal)         | ✅ Supported       |
| Android    | Skia (Vuklan/Opengl) | ✅ Supported       |
| Windows    | Skia (Dx13)          | ❌ Not Support yet |
| Linux      | Skia (Vuklan/Opengl) | ❌ Not Support yet |
| Web (WASM) | Canvas 2D            | ✅ Supported       |


## Features

- **Declarative UI** — Build interfaces with a composable widget tree using macros (`Container!`, `Row!`, `Column!`, `Text!`, `Button!`, etc.).
- **Stateful Widgets** — Flutter-style `StatefulWidget` / `State` pattern with `StateUpdater` for reactive rebuilds.
- **Animation System** — `AnimationController` with configurable duration, curves (`EaseIn`, `EaseOut`, `Bounce`, etc.), and effects (`Opacity`, `Scale`, `Translate`, `Rotate`, `SlideX`, `SlideY`). `⚠️ Unstable `
- **Layout Engine** — Flexbox-inspired layout with `Row`, `Column`, `Scrollable`...
- **Cross-Platform Rendering** — Skia on native platforms (Metal on Apple, Dx3D on Windoes and Vulkan/OpenGl for Linux and Android) and Canvas 2D on the web.
- **CLI Tooling** — `oxidize` a cli tool for creating running and builds projects.

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- Platform-specific dependencies:
  - **macOS / iOS**: Xcode and Metal-compatible hardware
  - **Android**: Android NDK
  - **Web**: `wasm-pack`

### Installation

```bash
# Clone the repository
git clone https://github.com/Cottons29/oxidize.git
# get into the directory
cd oxidize
# install the CLI tool
cargo install --path=./oxidize

````


### Create a New Project

```bash
oxidize new my_oxidize
```

### Running the App
```bash
cd my_oxidize

oxidize run
```

## Milestone



- [x] Oxidize CLI 
  - [x] `new project_name` to create a new project
  - [x] `run` to run the project
  - [ ] `build` to build the project
  - [ ] `test` to run tests
- [x] Oxidize Tooling
  - [x] project scaffolding (`oxidize new`)
  - [x] auto restart app
  - [ ] hot reload
- [x] Core widget system
  - [x] `StatefulWidget` / `State` pattern
  - [x] `Element` tree and `BuildContext`
  - [x] `StateUpdater` for reactive rebuilds
  - [x] Widget macros (`Container!`, `Row!`, `Column!`, `Text!`, `Button!`, etc.)
- [x] Layout engine
  - [x] `Row` and `Column` (flexbox-inspired)
  - [x] `Container` with padding, margin, and decoration
  - [x] `Scrollable` with scroll bar support `⚠️ Unstable`
  - [x] `Spacing` and `LayoutSpacing` attributes
  - [x] `BoxAlignment` (start, center, end, stretch)
- [x] Basic controls
  - [x] `Button` with press handler and hover/style variants
  - [x] `GestureDetector` `⚠️ Unstable`
  - [x] `InputField` (text field) `⛔️ Very Unstable`
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
  - [x] Named color palettes (`Colors::Blue`, `Colors::Gray`, etc.)
  - [x] Shade indexing (`Colors::Blue[100]`)
- [x] 🧪 Animation framework
  - [x] `AnimationController` (forward, reverse, repeat, auto-reverse) `⛔️ Very Unstable`
  - [x] Curves (`EaseIn`, `EaseOut`, `EaseInOut`, `Bounce`, `Linear`, etc.) `⛔️ Very Unstable`
  - [x] `Animated` widget with effects (`Opacity`, `Scale`, `Translate`, `Rotate`, `SlideX`, `SlideY`)  `⛔️ Very Unstable`
  - [x] 🧪 Enter and exit (delete) transitions `⛔️ Very Unstable`
- [x] Cross-platform support
  - [x] macOS (Skia + Metal)
  - [x] iOS (Skia + Metal)
  - [x] Android (Skia + OpenGL)
  - [x] 🧪 Web / WASM (Canvas 2D) `⚠️ Unstable`
  - [ ] Windows (Skia)
  - [ ] Linux (Skia)
  
- [ ] Gesture system
  - [x] Tap, double-tap `⚠️ Unstable`
  - [ ] Drag and pan
  - [ ] Swipe
  - [x] Long press `⚠️ Unstable`
- [ ] Navigation and routing
  - [ ] Navigator / route stack
  - [ ] Named routes
  - [ ] Page transitions
- [ ] Theming and dark mode
  - [ ] Theme data (colors, typography, spacing)
  - [ ] Dark / light mode switching
  - [ ] Custom theme support


> ⚠️ Unstable — feature is implemented but may have breaking changes or incomplete edge cases.
>
> ⛔️ Very Unstable – feature is implemented, but the functionality is not stable and has some critical bug that can break the app.
> 
> ❌ Not Implement Yet — feature is not implemented but may implement in the future.
>
