use crate::color::{color_trait::ColorMixer, Color};

impl ColorMixer for Color {

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
