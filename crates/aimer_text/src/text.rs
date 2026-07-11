pub mod raw_text;

use std::rc::Rc;
use std::sync::Mutex;
use crate::text::raw_text::RawTextWidget;
use aimer_macro::WidgetConstructor;
use aimer_style::{TextAlign, TextOverflow, TextStyle};
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, LayoutCache, Widget};

/// this is a widget for creating the text
#[allow(dead_code)]
#[derive(WidgetConstructor)]
pub struct Text {
    #[constructor(into, first)]
    text: Rc<str>,
    #[constructor(default)]
    text_align: TextAlign,
    #[constructor(default)]
    text_style: TextStyle,
}

impl Text {
    pub fn text_overflow(mut self, text_overflow: TextOverflow) -> Self {
        self.text_style.text_overflow = text_overflow;
        self
    }

    pub fn wrapped(self) -> Self {
        self.text_overflow(TextOverflow::Wrap)
    }

    pub fn ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }
}

impl Widget for Text {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        // println!("Creating text widget : {:?}", self.text);
        RawTextWidget {
            text: self.text.clone(),
            text_style: self.text_style,
            text_align: self.text_align,
            cache: LayoutCache::new(),
            _typeface: Mutex::new(None),
        }.boxed()
    }
    //
    // fn text_content(&self) -> Option<&str> {
    //     Some(&self.text)
    // }
}
