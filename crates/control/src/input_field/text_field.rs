use attribute::dimension::Dimension;
use std::cell::UnsafeCell;
use widget::base::{BuildContext, Colors};
use widget::text::TextAlign;
use widget::{Constructor, Element, TextStyle, Widget};

use crate::input_field::raw_fields::{
    Cursor, ExpandDirection, InputType, RawTextField, TextFieldController, TextFieldStyle,
};

#[allow(dead_code)]
#[derive(Constructor)]
pub struct TextField {
    #[constructor(default)]
    pub input_type: InputType,
    #[constructor(default, into)]
    pub text: String,
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
    #[constructor(default, into)]
    pub box_height: Dimension,
    #[constructor(default, into)]
    pub box_width: Dimension,
    #[constructor(default)]
    pub style: TextFieldStyle,
    #[constructor(default)]
    pub cursor_color: Colors,
}

impl Widget for TextField {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawTextField {
            input_type: match &self.input_type {
                InputType::Text => InputType::Text,
                InputType::Number => InputType::Number,
                InputType::Obscure => InputType::Obscure,
            },
            controller: TextFieldController::new(self.text.clone()),
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
            expand: match &self.expand {
                ExpandDirection::Horizontal => ExpandDirection::Horizontal,
                ExpandDirection::Vertical => ExpandDirection::Vertical,
                ExpandDirection::Both => ExpandDirection::Both,
                ExpandDirection::None => ExpandDirection::None,
            },
            box_height: self.box_height,
            box_width: self.box_width,
            box_constraint: widget::style::BoxConstraint::default(),
            cursor: Cursor::new(self.cursor_color),
            style: TextFieldStyle {
                background_color: self.style.background_color,
                border: self.style.border.clone(),
                padding: self.style.padding,
            },
            focused: UnsafeCell::new(self.auto_focus),
            cached_bounds: UnsafeCell::new(None),
        })
    }
}
