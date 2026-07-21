# First Blog: Introducing Aimer

**Aimer** is a cross-platform GUI framework for Rust with a declarative widget model and hardware-accelerated rendering. Building GUIs in Rust has traditionally meant choosing between mature but heavyweight bindings to existing toolkits, or newer frameworks that trade off performance and control for ease of use. Aimer aims to close that gap — designed to feel idiomatic to Rust while staying fast enough for demanding UIs.

### Why Aimer?

- **Declarative by design** — UIs are composed through chained builder methods, keeping layout and styling readable and close to the structure of the interface itself.
- **Hardware-accelerated** — rendering happens through the GPU, so even complex, deeply nested UIs stay smooth.
- **Cross-platform** — write once, run on desktop and beyond.
- **Rust-native** — no bindings, no FFI overhead, just Rust all the way down.

### Example Snippet

```rust
#[aimer::main]
pub fn start_app() {
  AimerApp::start(
    Container::new()
      .child(
        Text::new("Hello World!")
          .text_align(TextAlign::MidCenter)
          .text_style(TextStyle::new().color(Color::BLACK))
      )
  );
}
```

This short example renders a centered "Hello World!" label, but the same patterns — containers, children, and chainable style methods — scale up to full applications.

