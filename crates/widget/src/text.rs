#[cfg(not(target_arch = "wasm32"))]
mod raw_text;
#[cfg(target_arch = "wasm32")]
pub mod wasm_raw_text;

use crate::base::BuildContext;
pub use crate::style::text_style::{FontStyle, FontWeight, TextAlign, TextOverflow, TextStyle};
use crate::{Element, LayoutCache, Widget};
use constructor::Constructor;
use std::sync::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use crate::text::raw_text::RawTextWidget;
#[cfg(target_arch = "wasm32")]
use crate::text::wasm_raw_text::RawTextWidget;

/// this is a widget for creating the text
#[allow(dead_code)]
#[derive(Constructor)]
pub struct Text {
    #[constructor(into, first)]
    text: String, 
    #[constructor(default)]
    text_align: Option<TextAlign>,
    #[constructor(default)]
    text_style: Option<TextStyle>,
}

impl Widget for Text {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        let text_style = self.text_style.clone().unwrap_or_default();
        let text_align = self.text_align.unwrap_or(TextAlign::TopLeft);
        
        Box::new(RawTextWidget {
            text: self.text.clone(),
            text_style,
            text_align,
            cache: LayoutCache::new(),
            typeface: Mutex::new(None),
        })
    }
}

