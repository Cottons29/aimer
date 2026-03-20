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

### TextField Widget `⛔️ Very Unstable`

The `TextField` struct provides a text input field with customizable styling, input types, and event callbacks. It supports Desktop, iOS, Android, and WASM platforms.

**Struct Fields:**
```rust
pub struct TextField {
    pub controller: TextFieldController,
    pub input_type: InputType,        // Text, Number, Obscure
    pub prompt: String,
    pub hint: String,
    pub text_style: TextStyle,
    pub hint_style: TextStyle,
    pub prompt_style: TextStyle,
    pub text_align: TextAlign,
    pub auto_focus: bool,
    pub max_lines: Option<usize>,
    pub min_lines: Option<usize>,
    pub max_length: Option<usize>,
    pub enable: bool,
    pub expand: ExpandDirection,
    pub style: TextFieldStyle,
    pub hover_style: Option<TextFieldStyle>,
    pub focus_style: Option<TextFieldStyle>,
    pub disabled_style: Option<TextFieldStyle>,
    pub cursor_color: Colors,
    pub on_changed: TextFieldCallback,
    pub on_submitted: TextFieldCallback,
}
```

**Basic Usage:**

```rust
TextField!(
    controller: my_controller.clone(),
    hint: "Enter your name",
    input_type: InputType::Text,
)
```

**Event Callbacks:**

The `on_changed` callback fires whenever the text content changes (character insertion, deletion). The `on_submitted` callback fires when the user presses Enter.

Synchronous closures:
```rust
TextField!(
    controller: my_controller.clone(),
    on_changed: {
        move |item| {
            println!("Input changed: {}", item);
        }
    },
    on_submitted: {
        move |text| {
            println!("Submitted: {}", text);
        }
    },
)
```

Async closures (auto-wrapped via the macro):
```rust
TextField!(
    controller: my_controller.clone(),
    on_changed: async move |item| {
        println!("Input changed: {}", item);
    },
)
```

For async closures that need to capture state from surrounding scope, use the `AsyncTextFieldCallback` wrapper:
```rust
TextField!(
    controller: my_controller.clone(),
    on_changed: {
        let is_cooldown = self.is_cooldown;
        AsyncTextFieldCallback(move |item: String| async move {
            if !is_cooldown {
                println!("Input changed: {}", item);
            }
        })
    },
)
```

**Attributes:**
- `controller`: A `TextFieldController` for reading/writing the text value programmatically.
- `input_type`: The type of input (`Text`, `Number`, `Obscure`).
- `hint`: Placeholder text shown when the field is empty.
- `prompt`: Text displayed before the input area.
- `auto_focus`: Whether the field is focused on mount.
- `enable`: Whether the field accepts input (default: `true`).
- `style` / `hover_style` / `focus_style` / `disabled_style`: Visual style variants.
- `on_changed`: Callback invoked with the current text on every change.
- `on_submitted`: Callback invoked with the current text when Enter is pressed.

### GestureDetector Widget `⚠️ Unstable`

For capturing touch/click events (Tap, Long Press).

### Upcoming

Checkbox, Switch, Slider, DropdownMenu, Radio.

## The Widget Tree

Widgets are mounted and nested within each other. The `Element` tree holds the instantiated views, and `BuildContext` passes down necessary structural details during the rendering pass.
