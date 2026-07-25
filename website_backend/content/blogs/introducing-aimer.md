# First Blog: Introducing Aimer

**Aimer** is a cross-platform GUI framework for Rust with a declarative widget model and hardware-accelerated rendering. Building GUIs in Rust has traditionally meant choosing between mature but heavyweight bindings to existing toolkits, or newer frameworks that trade off performance and control for ease of use. Aimer aims to close that gap — designed to feel idiomatic to Rust while staying fast enough for demanding UIs.

### Why Aimer?

- **Declarative by design** — UIs are composed through chained builder methods, keeping layout and styling readable and close to the structure of the interface itself.
- **Hardware-accelerated** — rendering happens through the GPU, so even complex, deeply nested UIs stay smooth.
- **Cross-platform** — write once, run on desktop mobile and web(wasm).
- **Rust-native** — no bindings, no FFI overhead, just Rust all the way down.

> This website is building using **Aimer** source code available at [Github](https://github.com/Cottons29/aimer)

### The Screenshot 

![Aimer App](assets/first-aimer-screenshot.png)

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

### Why Not Macro Based?

Previously **Aimer** is using macro-based for building UI like this : 

```rust
#[aimer::main]
pub fn start_app() {
    AimerApp::start(
        Container!(
            child: Text!(
                "Hello World!",
                text_align: TextAlign::MidCenter,
                text_style: TextStyle!(
                    color: Colors::Black,
                )
            )
        )
    );
}
```
> It's look clean but the code completion is nightmare.

Now Aimer is using builder-pattern for build the UI like showed in the example snippet

> More examples inside the `jaime` crate in [Github](https://github.com/Cottons29/aimer)
