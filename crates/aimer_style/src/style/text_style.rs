
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
