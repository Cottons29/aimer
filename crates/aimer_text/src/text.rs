pub mod raw_text;

use crate::text::raw_text::RawTextWidget;
use aimer_macro::WidgetConstructor;
use aimer_style::{TextAlign, TextStyle};
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, LayoutCache, Widget};
use std::sync::Mutex;

/// this is a widget for creating the text
#[allow(dead_code)]
#[derive(WidgetConstructor)]
pub struct Text {
    #[constructor(into, first)]
    text: String,
    #[constructor(default)]
    text_align: TextAlign,
    #[constructor(default)]
    text_style: TextStyle,
}

impl Widget for Text {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawTextWidget {
            text: self.text.clone(),
            text_style: self.text_style.clone(),
            text_align: self.text_align,
            cache: LayoutCache::new(),
            _typeface: Mutex::new(None),
        })
    }
}
