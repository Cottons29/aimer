pub mod basic_color;
pub mod color_trait;

use basic_color::Colors;
/// Represents a color in one of the formats supported by Aimer.
///
/// `Color` can store explicit RGB/RGBA channels, packed hexadecimal values,
/// grayscale values, HSL/HSLA components, named colors from [`Colors`], or a
/// transparent color. Conversions to packed ARGB values are provided through
/// [`ColorMixer::to_u32`].
#[derive(Clone, Copy, PartialEq, Debug)]
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
    Grayscale(u8, u8),

    /// Grayscale (alpha = 255)
    Gray8(u8),

    /// HSL color model
    Hsl(f32, f32, f32), // (hue 0-360, sat 0-1, light 0-1)

    /// HSLA
    Hsla(f32, f32, f32, f32),

    /// Named colors (nice for theming)
    Basic(Colors),

    /// Fully transparent
    Transparent,
}

impl Color {
    pub const RED: Self = Color::Hex(0xFFFF0000);
    pub const GREEN: Self = Color::Hex(0xFF00FF00);
    pub const BLUE: Self = Color::Hex(0xFF0000FF);
    pub const WHITE: Self = Color::Hex(0xFFFFFFFF);
    pub const BLACK: Self = Color::Hex(0xFF000000);
    pub const YELLOW: Self = Color::Hex(0xFFFFFF00);
    pub const CYAN: Self = Color::Hex(0xFF00FFFF);
    pub const MAGENTA: Self = Color::Hex(0xFFFF00FF);
    pub const GRAY: Self = Color::Hex(0xFF808080);
    pub const ORANGE: Self = Color::Hex(0xFFFFA500);
    pub const PURPLE: Self = Color::Hex(0xFF800080);
    pub const BROWN: Self = Color::Hex(0xFFA52A2A);
}

impl Color {


    pub const fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
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

    pub const fn to_u32(&self) -> u32 {
        match *self {
            Color::Rgba(r, g, b, a) => {
                ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            }
            Color::Rgb(r, g, b) => {
                ((0xFFu32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            }
            Color::Hex(rgb) => 0xFF000000 | (rgb & 0xFFFFFF),
            Color::HexA(rgba) => {
                let r = (rgba >> 24) & 0xFF;
                let g = (rgba >> 16) & 0xFF;
                let b = (rgba >> 8) & 0xFF;
                let a = rgba & 0xFF;
                (a << 24) | (r << 16) | (g << 8) | b
            }
            Color::Grayscale(v, a) => ((a as u32) << 24) | ((v as u32) << 16) | ((v as u32) << 8) | (v as u32),
            Color::Gray8(v) => (0xFF << 24) | ((v as u32) << 16) | ((v as u32) << 8) | (v as u32),
            Color::Basic(named) => named.to_u32(),
            Color::Hsl(h, s, l) => {
                let (r, g, b) = Self::hsl_to_rgb(h, s, l);
                (0xFF << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            }
            Color::Hsla(h, s, l, a) => {
                let (r, g, b) = Self::hsl_to_rgb(h, s, l);
                let alpha = (a * 255.0).round() as u8;
                ((alpha as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            }
            Color::Transparent => 0x00000000,
        }
    }

    /// Returns this color with its alpha channel replaced by `opacity`.
    ///
    /// `opacity` is interpreted as an 8-bit alpha value, where `0` is fully
    /// transparent and `255` is fully opaque. Color channels are preserved and
    /// non-RGBA variants are converted to an alpha-capable representation when
    /// needed.
    pub const fn with_opacity(self, opacity: u8) -> Self {
        match self {
            Color::Rgba(r, g, b, _) => Color::Rgba(r, g, b, opacity),
            Color::Rgb(r, g, b) => Color::Rgba(r, g, b, opacity),
            Color::Hex(rgb) => Color::HexA(((rgb & 0xFFFFFF) << 8) | (opacity as u32)),
            Color::HexA(rgba) => Color::HexA((rgba & 0xFFFFFF00) | (opacity as u32)),
            Color::Grayscale(v, _) => Color::Grayscale(v, opacity),
            Color::Gray8(v) => Color::Grayscale(v, opacity),
            Color::Hsl(h, s, l) => Color::Hsla(h, s, l, opacity as f32 / 255.0),
            Color::Hsla(h, s, l, _) => Color::Hsla(h, s, l, opacity as f32 / 255.0),
            Color::Basic(named) => Color::Basic(named.alpha(opacity)),
            Color::Transparent => Color::Rgba(0, 0, 0, opacity),
        }
    }

    /// Scales the brightness of this color by `strength`.
    ///
    /// This is equivalent to [`Color::multiply`]: RGB channels are multiplied by
    /// `strength`, clamped to valid channel values, and the alpha channel is
    /// preserved.
    pub const fn with_brightness(self, strength: f32) -> Self {
        self.multiply(strength)
    }

    /// Darkens this color by moving its RGB channels toward black.
    ///
    /// `amount` is clamped to `0.0..=1.0`, where `0.0` returns the original
    /// color and `1.0` returns black with the original alpha.
    pub const fn darken(self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        self.multiply(1.0 - amount)
    }

    /// Lightens this color by moving its RGB channels toward white.
    ///
    /// `amount` is clamped to `0.0..=1.0`, where `0.0` returns the original
    /// color and `1.0` returns white with the original alpha.
    pub const fn lighten(self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let (r, g, b, a) = self.to_rgba_components();

        Self::from_rgba_components(lerp_channel(r, 255, amount), lerp_channel(g, 255, amount), lerp_channel(b, 255, amount), a)
    }

    /// Returns this color with its alpha channel replaced by `alpha`.
    ///
    /// `alpha` is a normalized value clamped to `0.0..=1.0`, where `0.0` is
    /// fully transparent and `1.0` is fully opaque.
    pub const fn with_alpha(self, alpha: f32) -> Self {
        let (r, g, b, _) = self.to_rgba_components();

        Self::from_rgba_components(r, g, b, float_to_channel(alpha.clamp(0.0, 1.0) * 255.0))
    }

    /// Multiplies this color's RGB channels by `factor`.
    ///
    /// Negative factors are treated as `0.0`, resulting channels are clamped to
    /// `0..=255`, and the alpha channel is preserved.
    pub const fn multiply(self, factor: f32) -> Self {
        let factor = factor.max(0.0);
        let (r, g, b, a) = self.to_rgba_components();

        Self::from_rgba_components(
            float_to_channel(r as f32 * factor),
            float_to_channel(g as f32 * factor),
            float_to_channel(b as f32 * factor),
            a,
        )
    }

    /// Blends this color toward `other` by the interpolation value `t`.
    ///
    /// This is an alias for [`Color::lerp`].
    pub const fn blend(self, other: Color, t: f32) -> Self {
        self.lerp(other, t)
    }

    /// Linearly interpolates between this color and `other`.
    ///
    /// `t` is clamped to `0.0..=1.0`, where `0.0` returns this color and `1.0`
    /// returns `other`. RGB and alpha channels are interpolated independently.
    pub const fn lerp(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let (r1, g1, b1, a1) = self.to_rgba_components();
        let (r2, g2, b2, a2) = other.to_rgba_components();

        Self::from_rgba_components(lerp_channel(r1, r2, t), lerp_channel(g1, g2, t), lerp_channel(b1, b2, t), lerp_channel(a1, a2, t))
    }

    /// Inverts this color's RGB channels.
    ///
    /// Each RGB channel is replaced with `255 - channel`, while the alpha
    /// channel is preserved.
    pub const fn invert(self) -> Self {
        let (r, g, b, a) = self.to_rgba_components();

        Self::from_rgba_components(255 - r, 255 - g, 255 - b, a)
    }

    /// Increases this color's saturation by `amount`.
    ///
    /// `amount` is clamped to a minimum of `0.0`. RGB channels are moved away
    /// from their luminance value and clamped to valid channel values; alpha is
    /// preserved.
    pub const fn saturate(self, amount: f32) -> Self {
        let amount = amount.max(0.0);
        let (r, g, b, a) = self.to_rgba_components();
        let gray = luminance(r, g, b);

        Self::from_rgba_components(
            float_to_channel(gray + (r as f32 - gray) * (1.0 + amount)),
            float_to_channel(gray + (g as f32 - gray) * (1.0 + amount)),
            float_to_channel(gray + (b as f32 - gray) * (1.0 + amount)),
            a,
        )
    }

    /// Decreases this color's saturation by `amount`.
    ///
    /// `amount` is clamped to `0.0..=1.0`, where `0.0` returns the original
    /// color and `1.0` returns a grayscale color with the original alpha.
    pub const  fn desaturate(self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let (r, g, b, a) = self.to_rgba_components();
        let gray = float_to_channel(luminance(r, g, b));

        Self::from_rgba_components(lerp_channel(r, gray, amount), lerp_channel(g, gray, amount), lerp_channel(b, gray, amount), a)
    }

    /// Converts this color to grayscale while preserving alpha.
    ///
    /// The gray value is calculated from the RGB channels using luminance
    /// weights.
    pub const fn grayscale(self) -> Self {
        self.desaturate(1.0)
    }

    const fn to_rgba_components(self) -> (u8, u8, u8, u8) {
        let argb = self.to_u32();

        (((argb >> 16) & 0xFF) as u8, ((argb >> 8) & 0xFF) as u8, (argb & 0xFF) as u8, ((argb >> 24) & 0xFF) as u8)
    }

    const fn from_rgba_components(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color::Rgba(r, g, b, a)
    }
}

const fn float_to_channel(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

const fn lerp_channel(start: u8, end: u8, t: f32) -> u8 {
    float_to_channel(start as f32 + (end as f32 - start as f32) * t)
}

const fn luminance(r: u8, g: u8, b: u8) -> f32 {
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

#[allow(clippy::derivable_impls)]
impl Default for Color {
    fn default() -> Self {
        Self::Transparent
    }
}

impl From<Colors> for Color {
    fn from(value: Colors) -> Self {
        Self::Basic(value)
    }
}

impl From<Colors> for Option<Color> {
    fn from(value: Colors) -> Self {
        Some(value.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn with_opacity_rgb() {
        assert_eq!(Color::Rgb(10, 20, 30).with_opacity(128).to_u32(), 0x800A141E);
    }

    #[test]
    fn with_opacity_rgba_overrides_alpha() {
        assert_eq!(Color::Rgba(10, 20, 30, 255).with_opacity(0).to_u32(), 0x000A141E);
    }

    #[test]
    fn with_opacity_hex() {
        assert_eq!(Color::Hex(0x112233).with_opacity(0x80).to_u32(), 0x80112233);
    }

    #[test]
    fn with_opacity_hexa_overrides_alpha() {
        assert_eq!(Color::HexA(0x112233FF).with_opacity(0x80).to_u32(), 0x80112233);
    }

    #[test]
    fn with_opacity_gray() {
        assert_eq!(Color::Gray8(0x40).with_opacity(0x80).to_u32(), 0x80404040);
    }

    #[test]
    fn with_opacity_basic() {
        assert_eq!(Color::Basic(Colors::Red).with_opacity(0x80).to_u32(), 0x80FF0000);
    }

    #[test]
    fn with_opacity_transparent() {
        assert_eq!(Color::Transparent.with_opacity(0x80).to_u32(), 0x80000000);
    }

    #[test]
    fn with_opacity_hsl() {
        // Red HSL -> alpha 0.5 * 255 rounds to 128 (0x80)
        assert_eq!(Color::Hsl(0.0, 1.0, 0.5).with_opacity(128).to_u32(), 0x80FF0000);
    }

    #[test]
    fn darken_scales_rgb_only() {
        assert_eq!(Color::Rgba(100, 150, 200, 128).darken(0.5).to_u32(), 0x80324B64);
    }

    #[test]
    fn lighten_moves_rgb_toward_white() {
        assert_eq!(Color::Rgb(100, 150, 200).lighten(0.5).to_u32(), 0xFFB2CBE4);
    }

    #[test]
    fn with_alpha_accepts_normalized_alpha() {
        assert_eq!(Color::Rgb(10, 20, 30).with_alpha(0.5).to_u32(), 0x800A141E);
    }

    #[test]
    fn multiply_clamps_rgb_and_keeps_alpha() {
        assert_eq!(Color::Rgba(100, 150, 200, 128).multiply(2.0).to_u32(), 0x80C8FFFF);
    }

    #[test]
    fn blend_and_lerp_interpolate_channels() {
        let red = Color::Rgb(255, 0, 0);
        let blue = Color::Rgba(0, 0, 255, 0);

        assert_eq!(red.lerp(blue, 0.5).to_u32(), 0x80800080);
        assert_eq!(red.blend(blue, 0.5), red.lerp(blue, 0.5));
    }

    #[test]
    fn invert_keeps_alpha() {
        assert_eq!(Color::Rgba(10, 20, 30, 40).invert().to_u32(), 0x28F5EBE1);
    }

    #[test]
    fn saturate_pushes_channels_away_from_gray() {
        assert_eq!(Color::Rgb(100, 150, 200).saturate(0.5).to_u32(), 0xFF509BE6);
    }

    #[test]
    fn desaturate_moves_channels_toward_gray() {
        assert_eq!(Color::Rgb(100, 150, 200).desaturate(0.5).to_u32(), 0xFF7992AB);
    }

    #[test]
    fn grayscale_sets_rgb_to_luminance() {
        assert_eq!(Color::Rgb(100, 150, 200).grayscale().to_u32(), 0xFF8D8D8D);
    }
}
