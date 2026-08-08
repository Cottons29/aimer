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
pub struct Button<W: Widget> {
    pub on_press: CallbackHolder,
    pub on_long_press: CallbackHolder,
    pub width: Dimension,
    pub height: Dimension,
    pub decoration: BoxDecoration,
    pub hover_decoration: BoxDecoration,
    pub is_disabled: bool,
    pub pressed_decoration: BoxDecoration,
    pub disabled_decoration: BoxDecoration,
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

## Input Controls

`TextField` and `TextArea` share `TextEditingController`, Unicode-safe selection,
undo/redo, IME composition, clipboard behavior, and `FocusNode` ownership on
desktop, web, iOS, and Android.

### TextField

`TextField` is strictly single-line. Return invokes `on_submitted`; pasted and
committed line separators become spaces. `InputType::Number` selects a numeric
software keyboard but does not validate the value. `InputType::Obscure` masks
text and prevents copying or cutting it.

```rust
use aimer::{FocusNode, InputType, TextEditingController, TextField};

let controller = TextEditingController::with_text("Aimer");
let focus = FocusNode::new();
let field = TextField::new()
    .controller(controller.clone())
    .focus_node(focus.clone())
    .input_type(InputType::Text)
    .hint("Your name")
    .max_length(Some(80))
    .on_changed(|text| println!("Changed: {text}"))
    .on_submitted(|text| println!("Submitted: {text}"));

focus.request_focus();
controller.set_text("Updated programmatically");
```

### TextArea

`TextArea` accepts hard newlines, wraps long visual lines, and scrolls
vertically. It starts at three lines and grows with its content unless
`max_lines` or `expand(true)` constrains that behavior.

```rust
use aimer::{TextArea, TextEditingController};

let controller = TextEditingController::new();
let area = TextArea::new()
    .controller(controller)
    .hint("Write a message")
    .min_lines(4)
    .max_lines(Some(10));
```

`TextEditingValue` stores text, selection, and the active composing range as an
immutable snapshot. `TextSelection` and `TextRange` use UTF-8 byte offsets that
are normalized to extended-grapheme boundaries. Use `value()`, `set_value()`,
`set_text()`, `clear()`, `undo()`, and `redo()` for programmatic editing.

### Migration from the unstable input field

- Replace `TextFieldController` with `TextEditingController`.
- Replace `with_initial(text)` with `with_text(text)`.
- Read text through `controller.value().text()` rather than a borrowed
  `controller.text()`.
- Move `min_lines`, `max_lines`, and expansion configuration from `TextField`
  to `TextArea`; `TextField` can no longer hold multiline text.
- Existing mobile projects must refresh their native text bridge:

```text
aimer migrate ios
aimer migrate android
```

Runnable versions are available in `examples/text_field.rs` and
`examples/text_area.rs`.

### GestureDetector Widget `⚠️ Unstable`

For capturing touch/click events (Tap, Long Press).

### Upcoming

Checkbox, Switch, Slider, DropdownMenu, Radio.

## The Widget Tree

Widgets are mounted and nested within each other. The `Element` tree holds the instantiated views, and `BuildContext` passes down necessary structural details during the rendering pass.
