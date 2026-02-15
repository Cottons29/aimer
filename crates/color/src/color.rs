pub mod color_trait;
pub mod color_impl;
pub mod basic_color;

use basic_color::BasicColor;
#[derive(Clone, Copy)]
pub enum Color {
    /// Red, Green, Blue, Alpha (0-255)
    Rgba(u8, u8, u8, u8),

    /// Red, Green, Blue (alpha = 255)
    Rgb(u8, u8, u8),

    /// Hex color like 0xRRGGBB
    Hex(u32),

    /// Hex with alpha like 0xRRGGBBAA
    HexA(u32),

    /// Grayscale + alpha
    Gray(u8, u8),

    /// Grayscale (alpha = 255)
    Gray8(u8),

    /// HSL color model
    Hsl(f32, f32, f32), // (hue 0-360, sat 0-1, light 0-1)

    /// HSLA
    Hsla(f32, f32, f32, f32),

    /// Named colors (nice for theming)
    Basic(BasicColor),

    /// Fully transparent
    Transparent,
}



