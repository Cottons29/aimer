use crate::input_field::controller::TextFieldController;
use crate::input_field::raw_fields::{
    Cursor, ExpandDirection, InputType, RawTextField, TextFieldCallback,
};
use widget::style::box_decoration::BoxDecoration;
use std::cell::UnsafeCell;
use attribute::CacheBounds;
use widget::base::{BuildContext, Colors};
use widget::text::TextAlign;
use widget::{Element, LayoutSpacing, Spacing, TextStyle, Widget, WidgetConstructor};


#[allow(dead_code)]
#[derive(WidgetConstructor)]
///
/// A configurable `TextField` widget struct that provides input capabilities
/// with an array of customizable properties for text input, styling, behavior,
/// and event handling.
///
/// # Fields
///
/// * `controller` - The `TextFieldController` instance to control the `TextField` widget.
///   Defaults to the `TextFieldController` implementation.
///
/// * `input_type` - Specifies the type of input allowed (e.g., text, number, password).
///   Defaults to a default implementation of `InputType`.
///
/// * `prompt` - The text prompt displayed when the `TextField` is empty. This field
///   can be initialized using types that implement `Into<String>`.
///
/// * `hint` - Hint text displayed within the `TextField` to provide user guidance.
///   Can be initialized using types implementing `Into<String>`.
///
/// * `hint_style` - Styling applied to the hint text. Defaults to a `TextStyle` implementation.
///
/// * `text_style` - Styling applied to the user-inputted text. Defaults to a `TextStyle` implementation.
///
/// * `prompt_style` - Styling for the text prompt. Defaults to a `TextStyle` implementation.
///
/// * `text_align` - The alignment of the text within the `TextField`. Defaults to a default implementation of `TextAlign`.
///
/// * `auto_focus` - Boolean indicating if the field should be automatically focused upon rendering. Defaults to `false`.
///
/// * `max_lines` - An optional maximum number of lines allowed for the text input. Defaults to `None`.
///
/// * `min_lines` - An optional minimum number of lines for the text input. Defaults to `None`.
///
/// * `max_length` - An optional maximum number of characters allowed in the input. Defaults to `None`.
///
/// * `enable` - Indicates whether the `TextField` is enabled for interaction.
///   Defaults to `true`.
///
/// * `expand` - Determines the expansion direction of the `TextField`.
///   Defaults to a default implementation of `ExpandDirection`.
///
/// * `decoration` - The default decoration applied to the `TextField`. Defaults to `BoxDecoration`.
///
/// * `hover_decoration` - The decoration applied to the `TextField` when hovered. Defaults to `None`.
///
/// * `focus_decoration` - The decoration applied to the `TextField` when it gains focus. Defaults to `None`.
///
/// * `disabled_decoration` - The decoration applied to the `TextField` when it is disabled. Defaults to `None`.
///
/// * `cursor_color` - Color of the text cursor. Defaults to a default `Colors` implementation.
///
/// * `on_changed` - Callback triggered when the input text changes. Accepts a `TextFieldCallback`
///   which is wrapped with an `AsyncTextFieldCallback`.
///
/// * `on_submitted` - Callback triggered when the user submits the input (e.g., pressing Enter).
///   Accepts a `TextFieldCallback` which is wrapped with an `AsyncTextFieldCallback`.
///
///
pub struct TextField {
    #[constructor(default)]
    controller: TextFieldController,
    #[constructor(default)]
    pub input_type: InputType,
    #[constructor(default, into)]
    pub prompt: String,
    #[constructor(default, into)]
    pub hint: String,
    #[constructor(default)]
    pub hint_style: TextStyle,
    #[constructor(default)]
    pub text_style: TextStyle,
    #[constructor(default)]
    pub prompt_style: TextStyle,
    #[constructor(default)]
    pub text_align: TextAlign,
    #[constructor(default)]
    pub auto_focus: bool,
    #[constructor(default)]
    pub max_lines: Option<usize>,
    #[constructor(default)]
    pub min_lines: Option<usize>,
    #[constructor(default)]
    pub max_length: Option<usize>,
    #[constructor(default = true)]
    pub enable: bool,
    #[constructor(default)]
    pub expand: ExpandDirection,
    #[constructor(default = BoxDecoration { background_color: Some(Colors::White.into()), ..Default::default() })]
    pub decoration: BoxDecoration,
    #[constructor(default)]
    pub hover_decoration: Option<BoxDecoration>,
    #[constructor(default)]
    pub focus_decoration: Option<BoxDecoration>,
    #[constructor(default)]
    pub disabled_decoration: Option<BoxDecoration>,
    #[constructor(default)]
    pub cursor_color: Colors,
    #[constructor(default, into, async_wrapper = "AsyncTextFieldCallback")]
    pub on_changed: TextFieldCallback,
    #[constructor(default, into, async_wrapper = "AsyncTextFieldCallback")]
    pub on_submitted: TextFieldCallback,
    #[constructor(default = TextField::DEFAULT_PADDING)]
    pub padding: LayoutSpacing,
}

impl Widget for TextField {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawTextField {
            input_type:  self.input_type,
            controller: self.controller.clone(),
            prompt: self.prompt.clone(),
            hint: self.hint.clone(),
            hint_style: self.hint_style.clone(),
            text_style: self.text_style.clone(),
            prompt_style: self.prompt_style.clone(),
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
            disabled_decoration: self.disabled_decoration.clone(),
            focused: UnsafeCell::new(self.auto_focus),
            hovered: UnsafeCell::new(false),
            cached_bounds: CacheBounds::new(),
            on_changed: self.on_changed.clone(),
            on_submitted: self.on_submitted.clone(),
            padding: self.padding
        })
    }
}


impl TextField {
    pub const DEFAULT_PADDING: LayoutSpacing = LayoutSpacing::all(Spacing::Px(4));
}