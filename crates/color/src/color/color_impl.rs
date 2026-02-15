use crate::color::{BasicColor, Color, color_trait::ColorMixer};

impl ColorMixer for Color {
    fn to_u32(&self) -> u32 {
        match *self {
            Color::Rgba(r, g, b, a) => {
                ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            }
            Color::Rgb(r, g, b) => ((0xFF as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
            Color::Hex(rgb) => 0xFF000000 | (rgb & 0xFFFFFF),
            Color::HexA(rgba) => {
                let r = (rgba >> 24) & 0xFF;
                let g = (rgba >> 16) & 0xFF;
                let b = (rgba >> 8) & 0xFF;
                let a = rgba & 0xFF;
                (a << 24) | (r << 16) | (g << 8) | b
            }
            Color::Gray(v, a) => ((a as u32) << 24) | ((v as u32) << 16) | ((v as u32) << 8) | (v as u32),
            Color::Gray8(v) => ((0xFF as u32) << 24) | ((v as u32) << 16) | ((v as u32) << 8) | (v as u32),
            Color::Basic(named) => named.to_u32(),
            Color::Hsl(h, s, l) => {
                let (r, g, b) = hsl_to_rgb(h, s, l);
                ((0xFF as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            }
            Color::Hsla(h, s, l, a) => {
                let (r, g, b) = hsl_to_rgb(h, s, l);
                let alpha = (a * 255.0).round() as u8;
                ((alpha as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            }
            Color::Transparent => 0x00000000,
        }
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r_prime, g_prime, b_prime) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r_prime + m) * 255.0).round() as u8,
        ((g_prime + m) * 255.0).round() as u8,
        ((b_prime + m) * 255.0).round() as u8,
    )
}

impl ColorMixer for BasicColor {
    fn to_u32(&self) -> u32 {
        match self {
            BasicColor::Red => 0xFFFF0000,
            BasicColor::Green => 0xFF00FF00,
            BasicColor::Blue => 0xFF0000FF,
            BasicColor::White => 0xFFFFFFFF,
            BasicColor::Black => 0xFF000000,
            BasicColor::Yellow => 0xFFFFFF00,
            BasicColor::Cyan => 0xFF00FFFF,
            BasicColor::Magenta => 0xFFFF00FF,
            BasicColor::Gray => 0xFF808080,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn test_hsl_red() {
        let color = Color::Hsl(0.0, 1.0, 0.5);
        // Red in ARGB: 0xFFFF0000
        assert_eq!(color.to_u32(), 0xFFFF0000);
    }

    #[test]
    fn test_hsl_green() {
        let color = Color::Hsl(120.0, 1.0, 0.5);
        // Green in ARGB: 0xFF00FF00
        assert_eq!(color.to_u32(), 0xFF00FF00);
    }

    #[test]
    fn test_hsl_blue() {
        let color = Color::Hsl(240.0, 1.0, 0.5);
        // Blue in ARGB: 0xFF0000FF
        assert_eq!(color.to_u32(), 0xFF0000FF);
    }

    #[test]
    fn test_hsl_white() {
        let color = Color::Hsl(0.0, 0.0, 1.0);
        // White in ARGB: 0xFFFFFFFF
        assert_eq!(color.to_u32(), 0xFFFFFFFF);
    }

    #[test]
    fn test_hsl_black() {
        let color = Color::Hsl(0.0, 0.0, 0.0);
        // Black in ARGB: 0xFF000000
        assert_eq!(color.to_u32(), 0xFF000000);
    }

    #[test]
    fn test_hsla_semi_transparent_red() {
        let color = Color::Hsla(0.0, 1.0, 0.5, 0.5);
        // Alpha 0.5 * 255 = 127.5 -> 128 (round) -> 0x80
        // ARGB: 0x80FF0000
        assert_eq!(color.to_u32(), 0x80FF0000);
    }
}
