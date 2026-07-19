# First Blog: Introducing Aimer

**Aimer** is a cross-platform GUI framework for Rust with a declarative widget model and hardware-accelerated rendering.

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

