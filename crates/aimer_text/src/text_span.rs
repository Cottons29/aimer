use std::ops::Range;
use std::rc::Rc;

use aimer_style::{FontFamily, FontStyle, FontWeight, TextDecoration, TextStyle};
use aimer_widget::base::Color;
use unicode_linebreak::linebreaks;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Default, Debug)]
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
        Self {
            text: text.into(),
            style: SpanStyle::new(),
            children: Vec::new(),
            link: None,
        }
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
        let style = self
            .style
            .resolve(inherited_style);
        let link = self
            .link
            .clone()
            .or(inherited_link);
        if !self.text.is_empty() {
            result.push(ResolvedTextSpan {
                text: self.text.clone(),
                style,
                link: link.clone(),
            });
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
        Self {
            text,
            style,
            link: None,
        }
    }
}

pub(crate) struct SpanLayout {
    pub fragments: Vec<SpanLayoutFragment>,
    pub line_breaks: Vec<SpanLayoutLineBreak>,
    pub line_count: usize,
}

pub(crate) struct SpanLayoutLineBreak {
    pub span_index: usize,
    pub source_range: Range<usize>,
    pub line: usize,
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
    struct PendingGrapheme<'a> {
        span_index: usize,
        text: &'a str,
        source_range: Range<usize>,
    }

    let plain_text = spans
        .iter()
        .map(|span| span.text.as_ref())
        .collect::<String>();
    let break_offsets = linebreaks(&plain_text)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let mut fragments = Vec::new();
    let mut line_breaks = Vec::new();
    let mut line = 0;
    let mut x = 0.0;
    let mut span_start = 0;
    let mut unit = Vec::new();

    let place_unit = |unit: &mut Vec<PendingGrapheme<'_>>,
                      fragments: &mut Vec<SpanLayoutFragment>,
                      line: &mut usize,
                      x: &mut f32,
                      measure: &mut dyn FnMut(&str, &TextStyle) -> f32| {
        if unit.is_empty() {
            return;
        }

        let mut runs: Vec<SpanLayoutFragment> = Vec::new();
        for grapheme in unit.iter() {
            if let Some(last) = runs.last_mut()
                && last.span_index == grapheme.span_index
            {
                last.text
                    .push_str(grapheme.text);
                last.source_range
                    .as_mut()
                    .expect("source text fragments have a range")
                    .end = grapheme.source_range.end;
            } else {
                runs.push(SpanLayoutFragment {
                    span_index: grapheme.span_index,
                    text: grapheme.text.to_owned(),
                    source_range: Some(grapheme.source_range.clone()),
                    line: *line,
                    x: 0.0,
                    width: 0.0,
                });
            }
        }
        let unit_width = runs
            .iter_mut()
            .map(|run| {
                run.width = measure(&run.text, &spans[run.span_index].style);
                run.width
            })
            .sum::<f32>();

        // Measuring separate words is fast, but shaping the complete painted run can produce a
        // slightly different width. Verify the complete line only near its edge so wrapping and
        // painting agree without reshaping the growing line after every word.
        let mut verified_width = None;
        if max_width > 0.0
            && *x > 0.0
            && *x + unit_width <= max_width
            && max_width - (*x + unit_width) <= unit_width
        {
            let mut line_runs: Vec<(usize, String)> = Vec::new();
            for fragment in fragments
                .iter()
                .filter(|fragment| fragment.line == *line)
                .chain(runs.iter())
            {
                if let Some((span_index, text)) = line_runs.last_mut()
                    && *span_index == fragment.span_index
                {
                    text.push_str(&fragment.text);
                } else {
                    line_runs.push((fragment.span_index, fragment.text.clone()));
                }
            }
            verified_width = Some(
                line_runs
                    .iter()
                    .map(|(span_index, text)| measure(text, &spans[*span_index].style))
                    .sum::<f32>(),
            );
        }

        if max_width > 0.0
            && *x > 0.0
            && (*x + unit_width > max_width
                || verified_width.is_some_and(|width| width > max_width))
        {
            *line += 1;
            *x = 0.0;
            verified_width = None;
        }

        if max_width <= 0.0 || unit_width <= max_width {
            for mut run in runs {
                run.line = *line;
                run.x = *x;
                *x += run.width;
                fragments.push(run);
            }
            if let Some(width) = verified_width {
                *x = width;
            }
            unit.clear();
            return;
        }

        // A single word can be wider than the line. Grow exact shaped chunks until the next
        // grapheme would overflow, then continue that word on the following line.
        let mut chunk: Option<SpanLayoutFragment> = None;
        for grapheme in unit.drain(..) {
            let same_span = chunk
                .as_ref()
                .is_some_and(|fragment| fragment.span_index == grapheme.span_index);
            let mut candidate = if same_span {
                let mut candidate = chunk.take().unwrap();
                candidate
                    .text
                    .push_str(grapheme.text);
                candidate
                    .source_range
                    .as_mut()
                    .expect("source text fragments have a range")
                    .end = grapheme.source_range.end;
                candidate
            } else {
                if let Some(fragment) = chunk.take() {
                    *x += fragment.width;
                    fragments.push(fragment);
                }
                SpanLayoutFragment {
                    span_index: grapheme.span_index,
                    text: grapheme.text.to_owned(),
                    source_range: Some(grapheme.source_range.clone()),
                    line: *line,
                    x: *x,
                    width: 0.0,
                }
            };
            candidate.width = measure(&candidate.text, &spans[candidate.span_index].style);

            if !same_span && max_width > 0.0 && *x > 0.0 && *x + candidate.width > max_width {
                *line += 1;
                *x = 0.0;
                candidate.line = *line;
                candidate.x = 0.0;
            }

            if max_width > 0.0
                && candidate.text != grapheme.text
                && *x + candidate.width > max_width
            {
                let split_at = candidate.text.len() - grapheme.text.len();
                candidate
                    .text
                    .truncate(split_at);
                candidate
                    .source_range
                    .as_mut()
                    .expect("source text fragments have a range")
                    .end -= grapheme.text.len();
                candidate.width = measure(&candidate.text, &spans[candidate.span_index].style);
                fragments.push(candidate);
                *line += 1;
                *x = 0.0;
                chunk = Some(SpanLayoutFragment {
                    span_index: grapheme.span_index,
                    text: grapheme.text.to_owned(),
                    source_range: Some(grapheme.source_range),
                    line: *line,
                    x: 0.0,
                    width: measure(grapheme.text, &spans[grapheme.span_index].style),
                });
            } else {
                candidate.line = *line;
                candidate.x = *x;
                chunk = Some(candidate);
            }
        }
        if let Some(fragment) = chunk {
            *x += fragment.width;
            fragments.push(fragment);
        }
    };

    for (span_index, span) in spans.iter().enumerate() {
        for (grapheme_start, grapheme) in span
            .text
            .grapheme_indices(true)
        {
            let source_range =
                span_start + grapheme_start..span_start + grapheme_start + grapheme.len();
            if grapheme == "\n" || grapheme == "\r\n" {
                place_unit(&mut unit, &mut fragments, &mut line, &mut x, &mut measure);
                line_breaks.push(SpanLayoutLineBreak {
                    span_index,
                    source_range,
                    line,
                });
                line += 1;
                x = 0.0;
                continue;
            }

            let is_break = break_offsets
                .binary_search(&source_range.end)
                .is_ok();
            unit.push(PendingGrapheme {
                span_index,
                text: grapheme,
                source_range,
            });
            if is_break {
                place_unit(&mut unit, &mut fragments, &mut line, &mut x, &mut measure);
            }
        }
        span_start += span.text.len();
    }
    place_unit(&mut unit, &mut fragments, &mut line, &mut x, &mut measure);

    let mut merged: Vec<SpanLayoutFragment> = Vec::new();
    for fragment in fragments {
        if let Some(previous) = merged.last_mut()
            && previous.span_index == fragment.span_index
            && previous.line == fragment.line
        {
            previous
                .text
                .push_str(&fragment.text);
            previous
                .source_range
                .as_mut()
                .expect("source text fragments have a range")
                .end = fragment
                .source_range
                .expect("source text fragments have a range")
                .end;
        } else {
            merged.push(fragment);
        }
    }

    let mut measured_line = usize::MAX;
    let mut measured_x = 0.0;
    for fragment in &mut merged {
        if fragment.line != measured_line {
            measured_line = fragment.line;
            measured_x = 0.0;
        }
        fragment.x = measured_x;
        fragment.width = measure(&fragment.text, &spans[fragment.span_index].style);
        measured_x += fragment.width;
    }

    SpanLayout {
        fragments: merged,
        line_breaks,
        line_count: line + 1,
    }
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
    layout.line_breaks.clear();
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

        assert_eq!(
            flattened[0]
                .style
                .background_color,
            Some(Color::RED)
        );
        assert_eq!(
            flattened[1]
                .style
                .background_color,
            Some(Color::RED)
        );
        assert_eq!(
            flattened[2]
                .style
                .background_color,
            Some(Color::BLUE)
        );
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

        let layout =
            layout_resolved_spans(&spans, 2.0, |text, _| text.graphemes(true).count() as f32);

        assert_eq!(layout.fragments.len(), 2);
        assert_eq!(layout.fragments[0].text, "aé");
        assert_eq!(layout.fragments[0].source_range, Some(0..3));
        assert_eq!(layout.fragments[1].text, "👩‍💻b");
        assert_eq!(layout.fragments[1].source_range, Some(3..15));
    }

    #[test]
    fn source_ranges_include_explicit_newlines_between_visible_fragments() {
        let spans = vec![ResolvedTextSpan::plain(
            Rc::from("first\nsecond"),
            TextStyle::default(),
        )];

        let layout = layout_resolved_spans(&spans, 0.0, |text, _| text.len() as f32);

        assert_eq!(layout.fragments.len(), 2);
        assert_eq!(layout.fragments[0].source_range, Some(0..5));
        assert_eq!(layout.fragments[1].source_range, Some(6..12));
    }

    #[test]
    fn explicit_line_breaks_retain_their_source_ranges_and_styles() {
        let spans = vec![
            ResolvedTextSpan::plain(Rc::from("first\n"), TextStyle::new().font_size(14)),
            ResolvedTextSpan::plain(Rc::from("\nsecond"), TextStyle::new().font_size(20)),
        ];

        let layout = layout_resolved_spans(&spans, 0.0, |text, _| text.len() as f32);

        assert_eq!(layout.line_count, 3);
        assert_eq!(layout.line_breaks.len(), 2);
        assert_eq!(layout.line_breaks[0].span_index, 0);
        assert_eq!(layout.line_breaks[0].source_range, 5..6);
        assert_eq!(layout.line_breaks[0].line, 0);
        assert_eq!(layout.line_breaks[1].span_index, 1);
        assert_eq!(layout.line_breaks[1].source_range, 6..7);
        assert_eq!(layout.line_breaks[1].line, 1);
    }

    #[test]
    fn wrapping_prefers_word_boundaries() {
        let spans = vec![ResolvedTextSpan::plain(
            Rc::from("hello world"),
            TextStyle::default(),
        )];

        let layout = layout_resolved_spans(&spans, 8.0, |text, _| text.chars().count() as f32);

        assert_eq!(layout.line_count, 2);
        assert_eq!(layout.fragments[0].text, "hello ");
        assert_eq!(layout.fragments[0].line, 0);
        assert_eq!(layout.fragments[1].text, "world");
        assert_eq!(layout.fragments[1].line, 1);
    }

    #[test]
    fn an_overlong_word_falls_back_to_grapheme_wrapping() {
        let spans = vec![ResolvedTextSpan::plain(
            Rc::from("abcdefgh"),
            TextStyle::default(),
        )];

        let layout = layout_resolved_spans(&spans, 3.0, |text, _| text.chars().count() as f32);

        assert_eq!(layout.line_count, 3);
        assert_eq!(layout.fragments[0].text, "abc");
        assert_eq!(layout.fragments[1].text, "def");
        assert_eq!(layout.fragments[2].text, "gh");
    }

    #[test]
    fn overlong_word_wraps_when_its_style_changes_at_the_line_edge() {
        let spans = vec![
            ResolvedTextSpan::plain(Rc::from("abc"), TextStyle::default()),
            ResolvedTextSpan::plain(Rc::from("d"), TextStyle::new().font_size(18)),
        ];

        let layout = layout_resolved_spans(&spans, 3.0, |text, _| text.len() as f32);

        assert_eq!(layout.line_count, 2);
        assert_eq!(layout.fragments[0].text, "abc");
        assert_eq!(layout.fragments[0].line, 0);
        assert_eq!(layout.fragments[1].text, "d");
        assert_eq!(layout.fragments[1].line, 1);
    }

    #[test]
    fn word_wrapping_continues_across_style_span_boundaries() {
        let spans = vec![
            ResolvedTextSpan::plain(Rc::from("hel"), TextStyle::default()),
            ResolvedTextSpan::plain(Rc::from("lo world"), TextStyle::new().font_size(18)),
        ];

        let layout = layout_resolved_spans(&spans, 7.0, |text, _| text.chars().count() as f32);
        let first_line = layout
            .fragments
            .iter()
            .filter(|fragment| fragment.line == 0)
            .map(|fragment| fragment.text.as_str())
            .collect::<String>();
        let second_line = layout
            .fragments
            .iter()
            .filter(|fragment| fragment.line == 1)
            .map(|fragment| fragment.text.as_str())
            .collect::<String>();

        assert_eq!(layout.line_count, 2);
        assert_eq!(first_line, "hello ");
        assert_eq!(second_line, "world");
    }

    #[test]
    fn wrapping_uses_the_shaped_width_of_complete_runs() {
        let spans = vec![ResolvedTextSpan::plain(
            Rc::from("AV"),
            TextStyle::default(),
        )];

        let layout = layout_resolved_spans(&spans, 10.0, |text, _| match text {
            "AV" => 10.0,
            "A" | "V" => 6.0,
            _ => unreachable!("unexpected measurement: {text}"),
        });

        assert_eq!(layout.line_count, 1);
        assert_eq!(layout.fragments[0].text, "AV");
        assert_eq!(layout.fragments[0].width, 10.0);
    }

    #[test]
    fn wrapping_accounts_for_reshaping_adjacent_words() {
        let spans = vec![ResolvedTextSpan::plain(
            Rc::from("one two"),
            TextStyle::default(),
        )];

        let layout = layout_resolved_spans(&spans, 7.0, |text, _| match text {
            "one " => 4.0,
            "two" => 3.0,
            "one two" => 8.0,
            _ => text.len() as f32,
        });

        assert_eq!(layout.line_count, 2);
        assert_eq!(layout.fragments[0].text, "one ");
        assert_eq!(layout.fragments[1].text, "two");
        assert!(
            layout
                .fragments
                .iter()
                .all(|fragment| fragment.x + fragment.width <= 7.0)
        );
    }

    #[test]
    fn unwrapped_words_are_measured_as_runs_instead_of_every_grapheme() {
        let spans = vec![ResolvedTextSpan::plain(
            Rc::from("Rich text resizing should stay responsive"),
            TextStyle::default(),
        )];
        let mut measurements = 0;

        let layout = layout_resolved_spans(&spans, 1_000.0, |text, _| {
            measurements += 1;
            text.len() as f32
        });

        assert_eq!(layout.line_count, 1);
        assert!(
            measurements <= 7,
            "layout performed {measurements} shaping measurements"
        );
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
