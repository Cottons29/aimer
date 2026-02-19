use color::prelude::Color;


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
#[derive(constructor::Constructor, Default, Clone)]
pub struct TextStyle  {
    #[constructor(default)]
    pub font_size: u32,
    #[constructor(default)]
    pub font_style : FontStyle,
    #[constructor(default)]
    pub font_weight: FontWeight,
    #[constructor(default, into)]
    pub color: Color
}
