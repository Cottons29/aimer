# Introduction

Welcome to the documentation for **Oxidize**, a cross-platform UI framework built with Rust. 

Oxidize is inspired by Flutter's widget model, allowing you to build native user interfaces from a single codebase using a declarative, composable widget tree.

## Why Oxidize?

Rust provides exceptional performance, memory safety, and concurrency. Combining these strengths with a declarative UI model similar to Flutter makes it easy to write fast, reliable, and predictable user interfaces. 

## Key Features

- **Declarative UI** — Build interfaces with a composable widget tree using powerful macros like `Container!`, `Row!`, `Column!`, `Text!`, and `Button!`.
- **Stateful Widgets** — Manage state using the Flutter-style `StatefulWidget` / `State` pattern, with a `StateUpdater` for targeted, reactive rebuilds.
- **Layout Engine** — Flexbox-inspired layout system featuring `Row`, `Column`, `Container`, `Spacing`, and `BoxAlignment`.
- **Cross-Platform Rendering** — Powered by Skia on native platforms (Metal on Apple, Vulkan/OpenGL on Android/Linux, Dx3D on Windows) and Canvas 2D on the Web (WASM).
- **CLI Tooling** — Comes with `oxidize`, a built-in CLI tool for scaffolding, running, and building projects effortlessly.

## Supported Platforms

| Platform   | Rendering Backend    | Status            |
|------------|----------------------|-------------------|
| macOS      | Skia (Metal)         | ✅ Supported       |
| iOS        | Skia (Metal)         | ✅ Supported       |
| Android    | Skia (Vulkan/OpenGL) | ✅ Supported       |
| Web (WASM) | Canvas 2D            | ✅ Supported       |
| Windows    | Skia (Dx13)          | ❌ Coming Soon     |
| Linux      | Skia (Vulkan/OpenGL) | ❌ Coming Soon     |

> **Note on Stability**: Oxidize is rapidly evolving. Some features like the Animation System and complex Gestures are currently marked as `⚠️ Unstable` or `⛔️ Very Unstable` and may undergo breaking changes.
