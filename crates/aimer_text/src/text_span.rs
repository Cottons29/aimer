use std::ops::Range;
use std::rc::Rc;

use aimer_style::{FontFamily, FontStyle, FontWeight, TextDecoration, TextStyle};
use aimer_widget::base::Color;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Default)]
pub struct SpanStyle {
    pub font_size: Option<u32>,
    pub font_family: Option<FontFamily>,
    pub font_style: Option<FontStyle>,
    pub font_weight: Option<FontWeight>,
    pub color: Option<Color>,
    pub background_color: Option<Color>,
    pub text_decoration: Option<TextDecoration>,
}

impl SpanStyle {
    pub const fn new() -> Self {
        Self {
            font_size: None,
            font_family: None,
            font_style: None,
            font_weight: None,
            color: None,
            background_color: None,
            text_decoration: None,
        }
    }

    pub const fn font_size(mut self, font_size: u32) -> Self {
        self.font_size = Some(font_size);
        self
    }

    pub const fn font_family(mut self, font_family: FontFamily) -> Self {
        self.font_family = Some(font_family);
        self
    }

    pub const fn font_style(mut self, font_style: FontStyle) -> Self {
        self.font_style = Some(font_style);
        self
    }

    pub const fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = Some(font_weight);
        self
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Overrides the inherited inline background without affecting layout.
    pub const fn background_color(mut self, background_color: Color) -> Self {
        self.background_color = Some(background_color);
        self
    }

    pub const fn text_decoration(mut self, text_decoration: TextDecoration) -> Self {
        self.text_decoration = Some(text_decoration);
        self
    }

    fn resolve(self, inherited: TextStyle) -> TextStyle {
        TextStyle {
            font_size: self
                .font_size
                .unwrap_or(inherited.font_size),
            font_family: self
                .font_family
                .unwrap_or(inherited.font_family),
            font_style: self
                .font_style
                .unwrap_or(inherited.font_style),
            font_weight: self
                .font_weight
                .unwrap_or(inherited.font_weight),
            color: self
                .color
                .unwrap_or(inherited.color),
            background_color: self
                .background_color
                .or(inherited.background_color),
            text_overflow: inherited.text_overflow,
            text_decoration: self
                .text_decoration
                .unwrap_or(inherited.text_decoration),
        }
    }
}

#[derive(Clone)]
pub struct TextSpan {
    pub text: Rc<str>,
    pub style: SpanStyle,
    pub children: Vec<TextSpan>,
    pub link: Option<Rc<str>>,
}

impl TextSpan {
    pub fn new(text: impl Into<Rc<str>>) -> Self {
        Self { text: text.into(), style: SpanStyle::new(), children: Vec::new(), link: None }
    }

    pub fn root(children: impl IntoIterator<Item = TextSpan>) -> Self {
        Self::new("").children(children)
    }

    pub fn style(mut self, style: SpanStyle) -> Self {
        self.style = style;
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = TextSpan>) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    pub fn child(mut self, child: TextSpan) -> Self {
        self.children.push(child);
        self
    }

    pub fn link(mut self, target: impl Into<Rc<str>>) -> Self {
        self.link = Some(target.into());
        self
    }

    pub fn flatten(&self, base_style: &TextStyle) -> Vec<ResolvedTextSpan> {
        let mut result = Vec::new();
        self.flatten_into(*base_style, None, &mut result);
        result
    }

    fn flatten_into(
        &self,
        inherited_style: TextStyle,
        inherited_link: Option<Rc<str>>,
        result: &mut Vec<ResolvedTextSpan>,
    ) {
        let style = self.style.resolve(inherited_style);
        let link = self
            .link
            .clone()
            .or(inherited_link);
        if !self.text.is_empty() {
            result.push(ResolvedTextSpan { text: self.text.clone(), style, link: link.clone() });
        }
        for child in &self.children {
            child.flatten_into(style, link.clone(), result);
        }
    }
}

#[derive(Clone)]
pub struct ResolvedTextSpan {
    pub text: Rc<str>,
    pub style: TextStyle,
    pub link: Option<Rc<str>>,
}

impl ResolvedTextSpan {
    pub fn plain(text: Rc<str>, style: TextStyle) -> Self {
        Self { text, style, link: None }
    }
}

pub(crate) struct SpanLayout {
    pub fragments: Vec<SpanLayoutFragment>,
    pub line_count: usize,
}

pub(crate) struct SpanLayoutFragment {
    pub span_index: usize,
    pub text: String,
    pub source_range: Option<Range<usize>>,
    pub line: usize,
    pub x: f32,
    pub width: f32,
}

pub(crate) fn layout_resolved_spans(
    spans: &[ResolvedTextSpan],
    max_width: f32,
    mut measure: impl FnMut(&str, &TextStyle) -> f32,
) -> SpanLayout {
    let mut fragments: Vec<SpanLayoutFragment> = Vec::new();
    let mut line = 0;
    let mut x = 0.0;
    let mut span_start = 0;

    for (span_index, span) in spans.iter().enumerate() {
        for (grapheme_start, grapheme) in span.text.grapheme_indices(true) {
            let source_range =
                span_start + grapheme_start..span_start + grapheme_start + grapheme.len();
            if grapheme == "\n" {
                line += 1;
                x = 0.0;
                continue;
            }

            let width = measure(grapheme, &span.style);
            if max_width > 0.0 && x > 0.0 && x + width > max_width {
                line += 1;
                x = 0.0;
            }

            if let Some(last) = fragments.last_mut()
                && last.span_index == span_index
                && last.line == line
            {
                last.text.push_str(grapheme);
                last.width += width;
                last.source_range
                    .as_mut()
                    .expect("source text fragments have a range")
                    .end = source_range.end;
            } else {
                fragments.push(SpanLayoutFragment {
                    span_index,
                    text: grapheme.to_owned(),
                    source_range: Some(source_range),
                    line,
                    x,
                    width,
                });
            }
            x += width;
        }
        span_start += span.text.len();
    }

    SpanLayout { fragments, line_count: line + 1 }
}

pub(crate) fn ellipsize_first_line(
    layout: &mut SpanLayout,
    spans: &[ResolvedTextSpan],
    max_width: f32,
    mut measure: impl FnMut(&str, &TextStyle) -> f32,
) {
    if layout.line_count <= 1 || spans.is_empty() {
        return;
    }

    layout
        .fragments
        .retain(|fragment| fragment.line == 0);
    let span_index = layout
        .fragments
        .last()
        .map(|fragment| fragment.span_index)
        .unwrap_or(0);
    let ellipsis_width = measure("…", &spans[span_index].style);

    while layout
        .fragments
        .last()
        .is_some_and(|fragment| fragment.x + fragment.width + ellipsis_width > max_width)
    {
        let last = layout
            .fragments
            .last_mut()
            .expect("a fragment exists");
        if let Some((start, grapheme)) = last
            .text
            .grapheme_indices(true)
            .next_back()
            .map(|(start, grapheme)| (start, grapheme.to_owned()))
        {
            last.text.truncate(start);
            last.width -= measure(&grapheme, &spans[last.span_index].style);
            if let Some(source_range) = &mut last.source_range {
                source_range.end -= grapheme.len();
            }
        }
        if last.text.is_empty() {
            layout.fragments.pop();
        }
    }

    if let Some(last) = layout.fragments.last_mut() {
        last.text.push('…');
        last.width += ellipsis_width;
    } else {
        layout
            .fragments
            .push(SpanLayoutFragment {
                span_index,
                text: "…".to_owned(),
                source_range: None,
                line: 0,
                x: 0.0,
                width: ellipsis_width,
            });
    }
    layout.line_count = 1;
}

#[cfg(test)]
mod tests {
    use aimer_style::{FontFamily, FontWeight, TextStyle};
    use aimer_widget::base::Color;

    use super::*;

    #[test]
    fn nested_spans_inherit_and_override_parent_style() {
        let root = TextSpan::new("prefix ")
            .style(
                SpanStyle::new()
                    .font_weight(FontWeight::Bold)
                    .color(Color::RED),
            )
            .children([
                TextSpan::new("inherited"),
                TextSpan::new(" overridden").style(SpanStyle::new().color(Color::BLUE)),
            ]);

        let flattened = root.flatten(&TextStyle::new().font_size(18));

        assert_eq!(flattened.len(), 3);
        assert_eq!(&*flattened[1].text, "inherited");
        assert_eq!(flattened[1].style.font_size, 18);
        assert_eq!(
            flattened[1]
                .style
                .font_weight
                .numeric(),
            FontWeight::Bold.numeric()
        );
        assert_eq!(flattened[1].style.color, Color::RED);
        assert_eq!(flattened[2].style.color, Color::BLUE);
    }

    #[test]
    fn nested_spans_inherit_and_override_font_family() {
        let custom = FontFamily::MONOSPACE;
        let flattened = TextSpan::new("parent")
            .style(SpanStyle::new().font_family(custom))
            .children([
                TextSpan::new(" inherited"),
                TextSpan::new(" sans").style(SpanStyle::new().font_family(FontFamily::SANS_SERIF)),
            ])
            .flatten(&TextStyle::default());

        assert_eq!(flattened[0].style.font_family, custom);
        assert_eq!(flattened[1].style.font_family, custom);
        assert_eq!(flattened[2].style.font_family, FontFamily::SANS_SERIF);
    }

    #[test]
    fn nested_spans_inherit_and_override_background_color() {
        let flattened = TextSpan::new("parent")
            .style(SpanStyle::new().background_color(Color::RED))
            .children([
                TextSpan::new(" inherited"),
                TextSpan::new(" blue").style(SpanStyle::new().background_color(Color::BLUE)),
            ])
            .flatten(&TextStyle::default());

        assert_eq!(flattened[0].style.background_color, Some(Color::RED));
        assert_eq!(flattened[1].style.background_color, Some(Color::RED));
        assert_eq!(flattened[2].style.background_color, Some(Color::BLUE));
        assert_eq!(TextStyle::default().background_color, None);
    }

    #[test]
    fn link_target_is_inherited_by_nested_text() {
        let root = TextSpan::root([TextSpan::new("")
            .link("https://aimer.dev")
            .children([
                TextSpan::new("Aimer "),
                TextSpan::new("docs").style(SpanStyle::new().font_weight(FontWeight::Bold)),
            ])]);

        let flattened = root.flatten(&TextStyle::default());

        assert_eq!(flattened.len(), 2);
        assert!(
            flattened
                .iter()
                .all(|span| span.link.as_deref() == Some("https://aimer.dev"))
        );
    }

    #[test]
    fn layout_fragments_retain_global_unicode_source_ranges_across_spans() {
        let spans = vec![
            ResolvedTextSpan::plain(Rc::from("aé"), TextStyle::default()),
            ResolvedTextSpan::plain(Rc::from("👩‍💻b"), TextStyle::default()),
        ];

        let layout = layout_resolved_spans(&spans, 2.0, |_, _| 1.0);

        assert_eq!(layout.fragments.len(), 2);
        assert_eq!(layout.fragments[0].text, "aé");
        assert_eq!(layout.fragments[0].source_range, Some(0..3));
        assert_eq!(layout.fragments[1].text, "👩‍💻b");
        assert_eq!(layout.fragments[1].source_range, Some(3..15));
    }

    #[test]
    fn source_ranges_include_explicit_newlines_between_visible_fragments() {
        let spans = vec![ResolvedTextSpan::plain(Rc::from("first\nsecond"), TextStyle::default())];

        let layout = layout_resolved_spans(&spans, 0.0, |text, _| text.len() as f32);

        assert_eq!(layout.fragments.len(), 2);
        assert_eq!(layout.fragments[0].source_range, Some(0..5));
        assert_eq!(layout.fragments[1].source_range, Some(6..12));
    }

    #[test]
    fn ellipsis_keeps_one_line_and_fits_the_available_width() {
        let style = TextStyle::new().font_size(10);
        let spans = vec![ResolvedTextSpan::plain(Rc::from("abcdef"), style)];
        let mut layout =
            layout_resolved_spans(&spans, 20.0, |text, _| text.chars().count() as f32 * 5.0);

        ellipsize_first_line(&mut layout, &spans, 20.0, |text, _| {
            text.chars().count() as f32 * 5.0
        });

        assert_eq!(layout.line_count, 1);
        assert_eq!(
            layout
                .fragments
                .iter()
                .map(|fragment| fragment.text.as_str())
                .collect::<String>(),
            "abc…"
        );
        assert!(
            layout
                .fragments
                .iter()
                .map(|fragment| fragment.width)
                .sum::<f32>()
                <= 20.0
        );
    }
}
