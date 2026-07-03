
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
    Value(u32)
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
    Value(f32)
}


#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
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
    BotRight
}

#[allow(dead_code)]
#[derive(aimer_macro::Constructor, Clone, Copy)]
pub struct TextStyle  {
    #[constructor(default = 13)]
    pub font_size: u32,
    #[constructor(default)]
    pub font_style : FontStyle,
    #[constructor(default)]
    pub font_weight: FontWeight,
    #[constructor(default = TextStyle::DEFAULT_TEXT_COLOR, into)]
    pub color: Color,
    #[constructor(default)]
    pub text_overflow: TextOverflow,
    #[constructor(default)]
    pub text_decoration: TextDecoration,
}

impl TextStyle {
    pub const DEFAULT_TEXT_COLOR: Color = Color::Basic(Colors::Black);
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
#[derive( Default, Clone, Copy)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
    Wrap,
    Value(u32)
}

#[cfg(test)]
mod tests {
    use super::FontWeight;

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
