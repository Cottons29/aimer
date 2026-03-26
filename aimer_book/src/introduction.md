# Introduction

Welcome to the documentation for **Aimer**, a cross-platform UI framework built with Rust. 

Aimer is inspired by Flutter's widget model, allowing you to build native user interfaces from a single codebase using a declarative, composable widget tree.

## Why Aimer?

Rust provides exceptional performance, memory safety, and concurrency. Combining these strengths with a declarative UI model similar to Flutter makes it easy to write fast, reliable, and predictable user interfaces. 

## Key Features

- **Declarative UI** — Build interfaces with a composable widget tree using powerful macros like `Container!`, `Row!`, `Column!`, `Text!`, and `Button!`.
- **Stateful Widgets** — Manage state using the Flutter-style `StatefulWidget` / `State` pattern, with a `StateUpdater` for targeted, reactive rebuilds.
- **Layout Engine** — Flexbox-inspired layout system featuring `Row`, `Column`, `Container`, `Spacing`, and `BoxAlignment`.
- **Cross-Platform Rendering** — Powered by Cupid on native platforms (Metal on Apple, Vulkan/OpenGL on Android/Linux, Dx3D on Windows) and Canvas 2D on the Web (WASM).
- **CLI Tooling** — Comes with `Aimer`, a built-in CLI tool for scaffolding, running, and building projects effortlessly.

## Supported Platforms

| Platform   | Rendering Backend     | Status        |
|------------|-----------------------|---------------|
| macOS      | Cupid (Metal)         | ✅ Supported   |
| iOS        | Cupid (Metal)         | ✅ Supported   |
| Android    | Cupid (Vulkan/OpenGL) | ✅ Supported   |
| Web (WASM) | Canvas 2D             | ✅ Supported   |
| Windows    | Cupid (Dx13)          | ❌ Coming Soon |
| Linux      | Cupid (Vulkan/OpenGL) | ❌ Coming Soon |

> **Note on Stability**: Aimer is rapidly evolving. Some features like the Animation System and complex Gestures are currently marked as `⚠️ Unstable` or `⛔️ Very Unstable` and may undergo breaking changes.
