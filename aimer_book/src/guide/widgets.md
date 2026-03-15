# Widgets

In Aimer, everything is a widget. The entire user interface is built as a declarative, composable widget tree using powerful Rust macros. This approach makes code concise and simple to read.

## Core Controls

### Text Widget

The `Text` struct represents a string of text with customizable styling.

**Struct Fields:**
```rust
pub struct Text {
    pub text: String,
    pub text_align: TextAlign,
    pub text_style: TextStyle,
}
```

The `Text!` macro displays a string of text with customizable styling.

**Macro Usage:**

```rust
Text!(
    "Hello World!",
    text_align: text::TextAlign::MidCenter,
    text_style: TextStyle!(
        color: Colors::Black,
        font_size: 24.0,
    )
)
```

**Attributes:**
- `text_align`: Configures alignment (`Left`, `MidCenter`, `Right`).
- `text_style`: Adjusts properties like color and font size using the `TextStyle!` macro.

### Button Widget

The `Button` struct represents a clickable element.

**Struct Fields:**
```rust
pub struct ButtonStyle {
    pub color: Colors,
    pub height: Dimension,
    pub width: Dimension,
    pub border: BoxBorder,
    pub outline: BoxOutline,
}

pub struct Button<W: Widget> {
    pub on_press: CallbackHolder,
    pub on_long_press: CallbackHolder,
    pub style: ButtonStyle,
    pub hover_style: ButtonStyle,
    pub is_disabled: bool,
    pub pressed_style: ButtonStyle,
    pub disabled_style: ButtonStyle,
    pub child: W,
}
```

A `Button!` macro creates a clickable element. It supports an `on_press` handler, allowing you to trigger application logic when clicked. It also features hover and style variants.

**Macro Usage:**

```rust
Button!(
    child: Text!("Click Me!"),
    on_press: || {
        println!("Button was clicked!");
    }
)
```

## Colors System

Aimer provides a built-in color system via `Colors`.
- **Named Palettes:** Access colors directly, e.g., `Colors::Blue`, `Colors::Gray`.
- **Opacity Indexing:** Access specific opacity level like `Colors::Blue[100]`.

## Input Controls (Experimental)

Several input controls are currently in development:
- **InputField**: For text input. `⛔️ Very Unstable`
- **GestureDetector**: For capturing touch/click events (Tap, Long Press). `⚠️ Unstable`
- **Upcoming**: Checkbox, Switch, Slider, DropdownMenu, Radio.

## The Widget Tree

Widgets are mounted and nested within each other. The `Element` tree holds the instantiated views, and `BuildContext` passes down necessary structural details during the rendering pass.
