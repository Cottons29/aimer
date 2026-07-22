pub mod raw_text;

use std::rc::Rc;
use std::sync::Mutex;

use aimer_style::{TextAlign, TextOverflow, TextStyle};
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, LayoutCache, Widget};

use crate::text::raw_text::RawTextWidget;

/// Displays a single run of styled text.
///
/// Text uses [`TextStyle::default`] and [`TextAlign::default`] unless replaced. Overflow behavior
/// comes from the active style; use [`Text::wrapped`] or [`Text::ellipsis`] for the common modes.
/// Unlike [`crate::RichText`], this widget does not provide spans, links, or selection.
///
/// # Example
///
/// ```
/// use aimer_style::{TextAlign, TextStyle};
/// use aimer_text::Text;
///
/// let title = Text::new("Aimer")
///     .text_align(TextAlign::MidCenter)
///     .text_style(TextStyle::default())
///     .wrapped();
/// ```
#[allow(dead_code)]
pub struct Text {
    text: Rc<str>,
    text_align: TextAlign,
    text_style: TextStyle,
}

impl Text {
    /// Creates text containing `text` with default style and alignment.
    pub fn new(text: impl Into<Rc<str>>) -> Self {
        Self {
            text: text.into(),
            text_align: TextAlign::default(),
            text_style: TextStyle::default(),
        }
    }

    /// Replaces the displayed string while preserving style and alignment.
    pub fn text(mut self, text: impl Into<Rc<str>>) -> Self {
        self.text = text.into();
        self
    }

    /// Sets how laid-out text is aligned within its available width.
    pub fn text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    /// Replaces the complete style used for shaping, layout, and painting.
    ///
    /// This includes font attributes, color, decoration, and overflow behavior.
    pub fn text_style(mut self, text_style: TextStyle) -> Self {
        self.text_style = text_style;
        self
    }
    /// Sets overflow behavior on the current style.
    ///
    /// Prefer configuring [`TextStyle::text_overflow`] before passing the style to
    /// [`Text::text_style`].
    #[deprecated(note = "set TextStyle::text_overflow and pass it to Text::text_style")]
    pub fn text_overflow(mut self, text_overflow: TextOverflow) -> Self {
        self.text_style.text_overflow = text_overflow;
        self
    }

    /// Configures text to wrap onto additional lines when width is constrained.
    #[allow(deprecated)]
    pub fn wrapped(self) -> Self {
        self.text_overflow(TextOverflow::Wrap)
    }

    /// Configures overflowing text to be truncated with an ellipsis.
    #[allow(deprecated)]
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
