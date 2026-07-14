use aimer_color::prelude::{Color, Colors};

#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
    ObliqueDeg(i32),
}
#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub enum FontWeight {
    VeryThin,
    Thin,
    #[default]
    Normal,
    Bold,
    Bolder,
    Value(u32),
}

impl FontWeight {
    /// Numeric CSS-style weight (100–900). 400 is normal, 700 is bold.
    pub fn numeric(self) -> u16 {
        match self {
            FontWeight::VeryThin => 100,
            FontWeight::Thin => 300,
            FontWeight::Normal => 400,
            FontWeight::Bold => 700,
            FontWeight::Bolder => 900,
            FontWeight::Value(v) => v.clamp(1, 1000) as u16,
        }
    }
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub enum LineHeight {
    #[default]
    Normal,
    Small,
    Huge,
    Value(f32),
}

/// The set of decoration lines to draw. Behaves like a small bit-set so several
/// lines (e.g. underline + line-through) can be combined without the awkward
/// `Combine(&'static [Self])` slice the old enum used.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TextDecorationLine(u8);

impl Default for TextDecorationLine {
    fn default() -> Self {
        Self::NONE
    }
}

#[allow(dead_code)]
impl TextDecorationLine {
    pub const NONE: Self = Self(0);
    pub const UNDERLINE: Self = Self(1 << 0);
    pub const OVERLINE: Self = Self(1 << 1);
    pub const LINE_THROUGH: Self = Self(1 << 2);
    /// Slants the glyphs (synthetic oblique) rather than drawing a line. Kept in
    /// this bit-set so it combines with the real lines (e.g. underline + italic).
    pub const ITALIC: Self = Self(1 << 3);

    pub const fn bits(self) -> u8 {
        self.0
    }

    /// True when every line in `other` is present in `self`.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for TextDecorationLine {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// The stroke style of a decoration line, mirroring the CSS `text-decoration-style`.
#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum TextDecorationStyle {
    #[default]
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

impl TextDecorationStyle {
    /// Stable numeric id handed to the render engine (kept in sync with the
    /// `text_decoration.wgsl` shader's `style` switch).
    pub const fn id(self) -> u32 {
        match self {
            TextDecorationStyle::Solid => 0,
            TextDecorationStyle::Double => 1,
            TextDecorationStyle::Dotted => 2,
            TextDecorationStyle::Dashed => 3,
            TextDecorationStyle::Wavy => 4,
        }
    }
}

/// Full text-decoration description: which lines, their stroke style, an optional
/// dedicated color (falling back to the text color), an optional thickness and a
/// vertical offset. Replaces the old on/off `Underline`-only enum.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct TextDecoration {
    pub line: TextDecorationLine,
    pub style: TextDecorationStyle,
    /// `None` inherits the text color.
    pub color: Option<Color>,
    /// `None` derives the thickness from the font size (~6%).
    pub thickness: Option<f32>,
    /// Extra vertical offset in logical pixels applied to the line (+ down).
    pub offset: f32,
}

#[allow(dead_code)]
#[allow(non_upper_case_globals)]
impl TextDecoration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn line(mut self, line: TextDecorationLine) -> Self {
        self.line = line;
        self
    }

    pub fn style(mut self, style: TextDecorationStyle) -> Self {
        self.style = style;
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = Some(thickness);
        self
    }

    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    /// No decoration. Kept as an associated constant so existing
    /// `TextDecoration::None` call sites keep working after the enum→struct change.
    pub const None: Self = Self {
        line: TextDecorationLine::NONE,
        style: TextDecorationStyle::Solid,
        color: None,
        thickness: None,
        offset: 0.0,
    };

    /// A plain solid underline (the previous default decoration). Kept as an
    /// associated constant for backward compatibility with `TextDecoration::Underline`.
    pub const Underline: Self = Self {
        line: TextDecorationLine::UNDERLINE,
        style: TextDecorationStyle::Solid,
        color: None,
        thickness: None,
        offset: 0.0,
    };

    pub const fn from_parts(line: TextDecorationLine, style: TextDecorationStyle) -> Self {
        Self { line, style, color: None, thickness: None, offset: 0.0 }
    }

    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub const fn with_thickness(mut self, thickness: f32) -> Self {
        self.thickness = Some(thickness);
        self
    }

    pub const fn with_offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }
}

impl Default for TextDecoration {
    fn default() -> Self {
        Self::None
    }
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub enum TextAlign {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    MidCenter,
    MidLeft,
    MidRight,
    BotLeft,
    BotCenter,
    BotRight,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct TextStyle {
    pub font_size: u32,
    pub font_style: FontStyle,
    pub font_weight: FontWeight,
    pub color: Color,
    pub text_overflow: TextOverflow,
    pub text_decoration: TextDecoration,
}

impl TextStyle {
    pub const DEFAULT_TEXT_COLOR: Color = Color::Basic(Colors::Black);

    pub fn new() -> Self {
        Self::default()
    }

    pub fn font_size(mut self, font_size: u32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn font_style(mut self, font_style: FontStyle) -> Self {
        self.font_style = font_style;
        self
    }

    pub fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = font_weight;
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    pub fn text_overflow(mut self, text_overflow: TextOverflow) -> Self {
        self.text_overflow = text_overflow;
        self
    }

    pub fn text_decoration(mut self, text_decoration: TextDecoration) -> Self {
        self.text_decoration = text_decoration;
        self
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 13,
            font_style: FontStyle::Normal,
            font_weight: FontWeight::Normal,
            color: Colors::Black.into(),
            text_overflow: TextOverflow::Clip,
            text_decoration: TextDecoration::None,
        }
    }
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
    Wrap,
    Value(u32),
}

#[cfg(test)]
mod tests {
    use super::{FontWeight, TextDecorationLine};

    // Guards the ITALIC bit: it must combine with real lines (e.g. underline)
    // without colliding, since the text widget reads it via `contains` to decide
    // whether to shear the glyphs.
    #[test]
    fn italic_line_bit_combines() {
        let both = TextDecorationLine::UNDERLINE | TextDecorationLine::ITALIC;
        assert!(both.contains(TextDecorationLine::ITALIC));
        assert!(both.contains(TextDecorationLine::UNDERLINE));
        assert!(!both.contains(TextDecorationLine::LINE_THROUGH));
        // Italic is a distinct bit, not overlapping any decoration line.
        assert_ne!(TextDecorationLine::ITALIC.bits(), 0);
        assert_eq!(TextDecorationLine::ITALIC.bits() & TextDecorationLine::UNDERLINE.bits(), 0);
        assert!(!TextDecorationLine::ITALIC.is_none());
    }

    // Guards the weight mapping and the ">= 600 renders bold" contract the
    // text pipeline relies on to trigger faux-bold double-strike.
    #[test]
    fn numeric_weight_and_bold_threshold() {
        assert_eq!(FontWeight::Normal.numeric(), 400);
        assert_eq!(FontWeight::Bold.numeric(), 700);
        assert_eq!(FontWeight::Value(650).numeric(), 650);

        // Normal / light weights stay below the bold threshold.
        assert!(FontWeight::Normal.numeric() < 600);
        assert!(FontWeight::Thin.numeric() < 600);
        assert!(FontWeight::VeryThin.numeric() < 600);
        // Bold and heavier cross it.
        assert!(FontWeight::Bold.numeric() >= 600);
        assert!(FontWeight::Bolder.numeric() >= 600);
    }
}
