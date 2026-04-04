
#[cfg(target_arch = "wasm32")]
pub mod wasm_raw_text;
#[cfg(not(target_arch = "wasm32"))]
pub mod raw_text;

#[cfg(not(target_arch = "wasm32"))]
use crate::text::raw_text::RawTextWidget;
#[cfg(target_arch = "wasm32")]
use crate::text::wasm_raw_text::RawTextWidget;
use aimer_macro::WidgetConstructor;
use std::sync::Mutex;
use aimer_style::{TextAlign, TextStyle};
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, LayoutCache, Widget};

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
            typeface: Mutex::new(None),
        })
    }
}

