use crate::input_field::controller::TextFieldController;
use crate::input_field::raw_fields::{
    Cursor, ExpandDirection, InputType, RawTextField, TextFieldStyle,
};
use std::cell::UnsafeCell;
use widget::base::{BuildContext, Colors};
use widget::text::TextAlign;
use widget::{Element, TextStyle, Widget, WidgetConstructor};

#[allow(dead_code)]
#[derive(WidgetConstructor)]
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
    #[constructor(default)]
    pub style: TextFieldStyle,
    #[constructor(default)]
    pub hover_style: Option<TextFieldStyle>,
    #[constructor(default)]
    pub focus_style: Option<TextFieldStyle>,
    #[constructor(default)]
    pub disabled_style: Option<TextFieldStyle>,
    #[constructor(default)]
    pub cursor_color: Colors,
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
            style: TextFieldStyle {
                background_color: self.style.background_color,
                border: self.style.border,
                padding: self.style.padding,
                outline: self.style.outline,
            },
            hover_style: self.hover_style.clone(),
            focus_style: self.focus_style.clone(),
            disabled_style: self.disabled_style.clone(),
            focused: UnsafeCell::new(self.auto_focus),
            hovered: UnsafeCell::new(false),
            cached_bounds: UnsafeCell::new(None),
        })
    }
}
