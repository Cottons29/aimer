pub mod raw_text;

use crate::text::raw_text::RawTextWidget;
use aimer_style::{TextAlign, TextOverflow, TextStyle};
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, LayoutCache, Widget};
use std::rc::Rc;
use std::sync::Mutex;

/// this is a widget for creating the text
#[allow(dead_code)]
pub struct Text {
    text: Rc<str>,
    text_align: TextAlign,
    text_style: TextStyle,
}

impl Text {
    pub fn new(text: impl Into<Rc<str>>) -> Self {
        Self {
            text: text.into(),
            text_align: TextAlign::default(),
            text_style: TextStyle::default(),
        }
    }

    pub fn text(mut self, text: impl Into<Rc<str>>) -> Self {
        self.text = text.into();
        self
    }

    pub fn text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    pub fn text_style(mut self, text_style: TextStyle) -> Self {
        self.text_style = text_style;
        self
    }

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
        }
        .boxed()
    }
}
