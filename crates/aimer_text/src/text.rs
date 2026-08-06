pub mod raw_text;
pub mod selectable_text;

use std::rc::Rc;
use std::sync::Mutex;

use aimer_style::{TextAlign, TextOverflow, TextStyle};
use aimer_widget::base::BuildContext;
use aimer_widget::{AnyElement, Element, LayoutCache, Widget};

use crate::selection::selectable::SelectionScope;
use crate::text::raw_text::RawTextWidget;
use crate::text::selectable_text::RawSelectableText;

/// The highlight color a selectable `Text` falls back to when it somehow sits
/// outside a region; regions always supply their own.
const DEFAULT_SELECTION_COLOR: aimer_widget::base::Color =
    aimer_widget::base::Color::Rgba(51, 153, 255, 96);

/// Displays a single run of styled text.
///
/// Text uses [`TextStyle::default`] and [`TextAlign::default`] unless replaced.
/// Overflow behavior comes from the active style; use [`Text::wrapped`] or
/// [`Text::ellipsis`] for the common modes. Unlike [`crate::RichText`], this
/// widget does not provide spans, links, or selection.
///
/// # Example
///
/// ```
/// use aimer_style::{TextAlign, TextStyle};
/// use aimer_text::Text;
///
/// let title = Text::new("Aimer").text_align(TextAlign::MidCenter)
///                               .text_style(TextStyle::default())
///                               .wrapped();
/// ```
#[allow(dead_code)]
pub struct Text {
    text: Rc<str>,
    text_align: TextAlign,
    text_style: TextStyle,
}

impl Text {
    /// Creates text containing `text` with default style and alignment.
    #[inline]
    pub fn new(text: impl Into<Rc<str>>) -> Self {
        Self {
            text: text.into(),
            text_align: TextAlign::default(),
            text_style: TextStyle::default(),
        }
    }

    /// Replaces the displayed string while preserving style and alignment.
    #[inline]
    pub fn text(mut self, text: impl Into<Rc<str>>) -> Self {
        self.text = text.into();
        self
    }

    /// Sets how laid-out text is aligned within its available width.
    #[inline]
    pub fn text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    /// Replaces the complete style used for shaping, layout, and painting.
    ///
    /// This includes font attributes, color, decoration, and overflow behavior.
    #[inline]
    pub fn text_style(mut self, text_style: TextStyle) -> Self {
        self.text_style = text_style;
        self
    }
    /// Sets overflow behavior on the current style.
    ///
    /// Prefer configuring [`TextStyle::text_overflow`] before passing the style
    /// to [`Text::text_style`].
    #[deprecated(note = "set TextStyle::text_overflow and pass it to Text::text_style")]
    #[inline]
    pub fn text_overflow(mut self, text_overflow: TextOverflow) -> Self {
        self.text_style.text_overflow = text_overflow;
        self
    }

    /// Configures text to wrap onto additional lines when width is constrained.
    #[allow(deprecated)]
    #[inline]
    pub fn wrapped(self) -> Self {
        self.text_overflow(TextOverflow::Wrap)
    }

    /// Configures overflowing text to be truncated with an ellipsis.
    #[allow(deprecated)]
    #[inline]
    pub fn ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }
}

impl Widget for Text {
    /// Emits the paragraph-backed selectable element inside a
    /// [`SelectionArea`](crate::SelectionArea) and the plain fast path
    /// everywhere else.
    ///
    /// The lookup is a single `TypeId` probe, so a tree without a region pays
    /// nothing for selection.
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        if ctx.get_state::<SelectionScope>().is_some() {
            return RawSelectableText::new(
                ctx,
                self.text.clone(),
                self.text_style,
                self.text_align,
                DEFAULT_SELECTION_COLOR,
            )
            .boxed();
        }
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

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use aimer_attribute::{ResolvedSize, Vec2d};
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_widget::base::{BuildContext, WindowHandle};
    use aimer_widget::Widget;

    use super::Text;
    use crate::selection::selectable::{SelectionCoordinator, SelectionScope};
    use crate::selection::session::SelectionSession;

    /// The debug name of the non-selectable fast path.
    const RAW_TEXT_NAME: &str = "RawTextWidget";

    fn context<'a>(canvas: Canvas<'a>, runtime: &'a tokio::runtime::Runtime) -> BuildContext<'a> {
        BuildContext::new(
            canvas,
            ResolvedSize {
                width: 200.0,
                height: 100.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
            runtime.handle().clone(),
        )
    }

    #[test]
    fn text_outside_a_region_stays_on_the_non_selectable_fast_path() {
        let inner = InnerCanvas::new();
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let ctx = context(Canvas::new(&inner), &runtime);

        let element = Text::new("plain").to_element(&ctx);

        assert_eq!(element.debug_name(), RAW_TEXT_NAME);
    }

    #[test]
    fn text_inside_a_region_becomes_selectable() {
        let inner = InnerCanvas::new();
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let ctx = context(Canvas::new(&inner), &runtime);
        let session = SelectionSession::new(
            ctx.window.clone(),
            Rc::new(SelectionCoordinator::default()),
            super::DEFAULT_SELECTION_COLOR,
        );

        let element = ctx.with_state(SelectionScope(Rc::clone(&session)), |ctx| {
            Text::new("selectable").to_element(ctx)
        });

        assert_eq!(element.debug_name(), "SelectableText");
        session.select_all();
        assert_eq!(session.selected_text(), "selectable");
    }

    #[test]
    fn toggling_a_region_around_a_text_swaps_the_element_without_panicking() {
        let inner = InnerCanvas::new();
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let ctx = context(Canvas::new(&inner), &runtime);
        let session = SelectionSession::new(
            ctx.window.clone(),
            Rc::new(SelectionCoordinator::default()),
            super::DEFAULT_SELECTION_COLOR,
        );

        let inside = ctx.with_state(SelectionScope(Rc::clone(&session)), |ctx| {
            Text::new("toggled").to_element(ctx)
        });
        let outside = Text::new("toggled").to_element(&ctx);
        outside.adopt_runtime_state_from(inside.as_ref());

        assert_eq!(outside.debug_name(), RAW_TEXT_NAME);
    }
}
