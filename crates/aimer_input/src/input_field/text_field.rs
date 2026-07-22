use std::cell::Cell;
use std::sync::Arc;

use aimer_animation::AnimInstant;
use aimer_attribute::CacheBounds;
use aimer_style::{BoxDecoration, LayoutSpacing, Spacing, TextAlign, TextStyle};
use aimer_widget::base::{BuildContext, Color, Colors};
use aimer_widget::{Element, Widget};

use crate::input_field::controller::TextFieldController;
use crate::input_field::raw_fields::{
    Cursor, ExpandDirection, InputType, RawTextField, TextFieldCallback,
};

#[allow(dead_code)]
///
/// A configurable `TextField` widget struct that provides input capabilities
/// with an array of customizable properties for text input, styling, behavior,
/// and event handling.
///
/// # Fields
///
/// * `controller` - The `TextFieldController` instance to control the
///   `TextField` widget. Defaults to the `TextFieldController` implementation.
///
/// * `input_type` - Specifies the type of input allowed (e.g., text, number,
///   password). Defaults to a default implementation of `InputType`.
///
/// * `prompt` - The text prompt displayed when the `TextField` is empty. This
///   field can be initialized using types that implement `Into<String>`.
///
/// * `hint` - Hint text displayed within the `TextField` to provide user
///   guidance. Can be initialized using types implementing `Into<String>`.
///
/// * `hint_style` - Styling applied to the hint text. Defaults to a `TextStyle`
///   implementation.
///
/// * `text_style` - Styling applied to the user-inputted text. Defaults to a
///   `TextStyle` implementation.
///
/// * `prompt_style` - Styling for the text prompt. Defaults to a `TextStyle`
///   implementation.
///
/// * `text_align` - The alignment of the text within the `TextField`. Defaults
///   to a default implementation of `TextAlign`.
///
/// * `auto_focus` - Boolean indicating if the field should be automatically
///   focused upon rendering. Defaults to `false`.
///
/// * `max_lines` - An optional maximum number of lines allowed for the text
///   input. Defaults to `None`.
///
/// * `min_lines` - An optional minimum number of lines for the text input.
///   Defaults to `None`.
///
/// * `max_length` - An optional maximum number of characters allowed in the
///   input. Defaults to `None`.
///
/// * `enable` - Indicates whether the `TextField` is enabled for interaction.
///   Defaults to `true`.
///
/// * `expand` - Determines the expansion direction of the `TextField`. Defaults
///   to a default implementation of `ExpandDirection`.
///
/// * `decoration` - The default decoration applied to the `TextField`. Defaults
///   to `BoxDecoration`.
///
/// * `hover_decoration` - The decoration applied to the `TextField` when
///   hovered. Defaults to `None`.
///
/// * `focus_decoration` - The decoration applied to the `TextField` when it
///   gains focus. Defaults to `None`.
///
/// * `disabled_decoration` - The decoration applied to the `TextField` when it
///   is disabled. Defaults to `None`.
///
/// * `cursor_color` - Color of the text cursor. Defaults to a default `Colors`
///   implementation.
///
/// * `on_changed` - Callback triggered when the input text changes. Accepts a
///   `TextFieldCallback` which is wrapped with an `AsyncTextFieldCallback`.
///
/// * `on_submitted` - Callback triggered when the user submits the input (e.g.,
///   pressing Enter). Accepts a `TextFieldCallback` which is wrapped with an
///   `AsyncTextFieldCallback`.
///
/// * `on_focus` - Callback triggered when the field gains focus. Accepts a
///   `TextFieldCallback` which is wrapped with an `AsyncTextFieldCallback`.
///
/// * `on_blur` - Callback triggered when the field loses focus. Accepts a
///   `TextFieldCallback` which is wrapped with an `AsyncTextFieldCallback`.
///
/// * `read_only` - When `true`, text cannot be modified via keyboard input.
///   Selection, copy, and cursor movement still work. Defaults to `false`.
///
/// Decorations are selected in disabled, focused, hovered, then normal priority. The field starts
/// empty and enabled with a white background, four logical pixels of padding, and no line or length
/// limits. [`TextField::auto_focus`] controls the initial focus state when the element is created.
///
/// # Example
///
/// ```
/// use aimer_input::input::{InputType, TextField, TextFieldController};
///
/// let controller = TextFieldController::with_initial("hello");
/// let field = TextField::new()
///     .controller(controller)
///     .input_type(InputType::Text)
///     .hint("Message")
///     .max_length(Some(200))
///     .on_changed(|text| println!("changed to {text}"));
/// ```
pub struct TextField {
    controller: TextFieldController,
    pub input_type: InputType,
    pub prompt: Arc<str>,
    pub hint: Arc<str>,
    pub hint_style: TextStyle,
    pub text_style: TextStyle,
    pub prompt_style: TextStyle,
    pub text_align: TextAlign,
    pub auto_focus: bool,
    pub max_lines: Option<usize>,
    pub min_lines: Option<usize>,
    pub max_length: Option<usize>,
    pub enable: bool,
    pub expand: ExpandDirection,
    pub decoration: BoxDecoration,
    pub hover_decoration: Option<BoxDecoration>,
    pub focus_decoration: Option<BoxDecoration>,
    pub disabled_decoration: Option<BoxDecoration>,
    pub selection_color: Color,
    pub cursor_color: Colors,
    pub on_changed: TextFieldCallback,
    pub on_submitted: TextFieldCallback,
    pub on_focus: TextFieldCallback,
    pub on_blur: TextFieldCallback,
    pub read_only: bool,
    pub padding: LayoutSpacing,
}

impl Widget for TextField {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawTextField {
            input_type: self.input_type,
            controller: self.controller.clone(),
            prompt: self.prompt.clone(),
            hint: self.hint.clone(),
            hint_style: self.hint_style,
            text_style: self.text_style,
            prompt_style: self.prompt_style,
            text_align: self.text_align,
            auto_focus: self.auto_focus,
            max_lines: self.max_lines,
            min_lines: self.min_lines,
            max_length: self.max_length,
            enable: self.enable,
            expand: self.expand,
            cursor: Cursor::new(self.cursor_color),
            decoration: self.decoration.clone(),
            hover_decoration: self.hover_decoration.clone(),
            focus_decoration: self.focus_decoration.clone(),
            disabled_decoration: self
                .disabled_decoration
                .clone(),
            selection_color: self.selection_color,
            focused: Cell::new(self.auto_focus),
            hovered: Cell::new(false),
            cached_bounds: CacheBounds::new(),
            on_changed: self.on_changed.clone(),
            on_submitted: self.on_submitted.clone(),
            on_focus: self.on_focus.clone(),
            on_blur: self.on_blur.clone(),
            read_only: self.read_only,
            mouse_held: Cell::new(false),
            last_click_time: Cell::new(AnimInstant::now()),
            click_count: Cell::new(0),
            pending_click: Cell::new(None),
            scroll_x: Cell::new(0.0),
            preedit_text: Cell::new(String::new()),
            preedit_cursor: Cell::new(None),
            blink_scheduled: Cell::new(false),
            padding: self.padding,
        })
    }
}

impl TextField {
    /// Default padding applied between the decoration and editable content.
    pub const DEFAULT_PADDING: LayoutSpacing = LayoutSpacing::all(Spacing::Px(4));

    /// Creates an empty, enabled, editable field with default styling and no-op callbacks.
    pub fn new() -> Self {
        Self {
            controller: TextFieldController::default(),
            input_type: InputType::default(),
            prompt: Arc::default(),
            hint: Arc::default(),
            hint_style: TextStyle::default(),
            text_style: TextStyle::default(),
            prompt_style: TextStyle::default(),
            text_align: TextAlign::default(),
            auto_focus: false,
            max_lines: None,
            min_lines: None,
            max_length: None,
            enable: true,
            expand: ExpandDirection::default(),
            decoration: BoxDecoration {
                background_color: Some(Colors::White.into()),
                ..Default::default()
            },
            hover_decoration: None,
            focus_decoration: None,
            disabled_decoration: None,
            selection_color: Color::Rgba(66, 133, 244, 100),
            cursor_color: Colors::default(),
            on_changed: TextFieldCallback::default(),
            on_submitted: TextFieldCallback::default(),
            on_focus: TextFieldCallback::default(),
            on_blur: TextFieldCallback::default(),
            read_only: false,
            padding: Self::DEFAULT_PADDING,
        }
    }

    /// Uses `controller` as the field's shared text and selection-history owner.
    ///
    /// Clones of the controller observe the same text, undo stack, and redo stack.
    pub fn controller(mut self, controller: TextFieldController) -> Self {
        self.controller = controller;
        self
    }

    /// Sets the accepted input mode, including plain text, numeric, and obscured password input.
    pub fn input_type(mut self, input_type: InputType) -> Self {
        self.input_type = input_type;
        self
    }

    /// Sets the prompt drawn when the field is empty and focused.
    pub fn prompt(mut self, prompt: impl Into<Arc<str>>) -> Self {
        self.prompt = prompt.into();
        self
    }

    /// Sets the hint drawn when the field is empty and not showing its prompt.
    pub fn hint(mut self, hint: impl Into<Arc<str>>) -> Self {
        self.hint = hint.into();
        self
    }

    /// Replaces the style used to lay out and paint the hint.
    pub fn hint_style(mut self, hint_style: TextStyle) -> Self {
        self.hint_style = hint_style;
        self
    }

    /// Replaces the style used to lay out and paint entered text.
    pub fn text_style(mut self, text_style: TextStyle) -> Self {
        self.text_style = text_style;
        self
    }

    /// Replaces the style used to lay out and paint the focused empty prompt.
    pub fn prompt_style(mut self, prompt_style: TextStyle) -> Self {
        self.prompt_style = prompt_style;
        self
    }

    /// Sets the alignment of text within the field's content area.
    pub fn text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    /// Sets whether a newly created field starts focused.
    ///
    /// This initializes focus when the widget becomes an element; it is not an imperative request
    /// to focus an already mounted field.
    pub fn auto_focus(mut self, auto_focus: bool) -> Self {
        self.auto_focus = auto_focus;
        self
    }

    /// Sets the optional maximum number of laid-out input lines.
    ///
    /// `None` removes the limit. A value of `Some(1)` produces single-line submission behavior.
    pub fn max_lines(mut self, max_lines: Option<usize>) -> Self {
        self.max_lines = max_lines;
        self
    }

    /// Sets the optional minimum number of lines reserved by layout.
    ///
    /// `None` reserves only the space required by the current content.
    pub fn min_lines(mut self, min_lines: Option<usize>) -> Self {
        self.min_lines = min_lines;
        self
    }

    /// Sets the optional maximum input length in Unicode scalar values.
    ///
    /// `None` removes the limit. Input beyond the limit is not inserted.
    pub fn max_length(mut self, max_length: Option<usize>) -> Self {
        self.max_length = max_length;
        self
    }

    /// Enables or disables focus, editing, selection, and input callbacks.
    ///
    /// A disabled field uses its configured disabled decoration when present.
    pub fn enable(mut self, enable: bool) -> Self {
        self.enable = enable;
        self
    }

    /// Sets the directions in which the field expands to consume available layout space.
    pub fn expand(mut self, expand: ExpandDirection) -> Self {
        self.expand = expand;
        self
    }

    /// Replaces the normal field decoration.
    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = decoration;
        self
    }

    /// Sets the decoration used while an enabled, unfocused field is hovered.
    pub fn hover_decoration(mut self, hover_decoration: BoxDecoration) -> Self {
        self.hover_decoration = Some(hover_decoration);
        self
    }

    /// Sets the decoration used while the enabled field is focused.
    ///
    /// Focus decoration takes precedence over hover decoration.
    pub fn focus_decoration(mut self, focus_decoration: BoxDecoration) -> Self {
        self.focus_decoration = Some(focus_decoration);
        self
    }

    /// Sets the decoration used while the field is disabled.
    ///
    /// Disabled decoration takes precedence over focus and hover decorations.
    pub fn disabled_decoration(mut self, disabled_decoration: BoxDecoration) -> Self {
        self.disabled_decoration = Some(disabled_decoration);
        self
    }

    /// Sets the color painted behind selected text.
    pub fn selection_color(mut self, selection_color: impl Into<Color>) -> Self {
        self.selection_color = selection_color.into();
        self
    }

    /// Sets the color of the insertion cursor.
    pub fn cursor_color(mut self, cursor_color: Colors) -> Self {
        self.cursor_color = cursor_color;
        self
    }

    /// Sets the callback invoked after a user edit changes the text.
    ///
    /// The callback receives the complete updated string. Programmatic controller mutations do not
    /// themselves dispatch widget callbacks.
    pub fn on_changed(mut self, on_changed: impl Into<TextFieldCallback>) -> Self {
        self.on_changed = on_changed.into();
        self
    }

    /// Sets the callback invoked when the user submits the field.
    ///
    /// The callback receives the current complete string.
    pub fn on_submitted(mut self, on_submitted: impl Into<TextFieldCallback>) -> Self {
        self.on_submitted = on_submitted.into();
        self
    }

    /// Sets the callback invoked when the field gains focus.
    ///
    /// The callback receives the current complete string.
    pub fn on_focus(mut self, on_focus: impl Into<TextFieldCallback>) -> Self {
        self.on_focus = on_focus.into();
        self
    }

    /// Sets the callback invoked when the field loses focus.
    ///
    /// The callback receives the current complete string.
    pub fn on_blur(mut self, on_blur: impl Into<TextFieldCallback>) -> Self {
        self.on_blur = on_blur.into();
        self
    }

    /// Sets whether user editing is blocked while focus, selection, copy, and navigation remain.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Sets the spacing between the field decoration and its text content.
    pub fn padding(mut self, padding: impl Into<LayoutSpacing>) -> Self {
        self.padding = padding.into();
        self
    }
}
