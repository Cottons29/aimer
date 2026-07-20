use aimer_color::prelude::Color;
use aimer_provider::{ProviderContext, Snapshot};
use aimer_widget::base::BuildContext;

/// Semantic colors used by themed widgets.
///
/// Start with [`ThemeData::light`] or [`ThemeData::dark`], then replace
/// individual colors with the builder methods. The `on_*` colors are intended
/// for content drawn on top of the corresponding base color.
///
/// # Examples
///
/// ```
/// use aimer_color::prelude::Color;
/// use aimer_style::ThemeData;
///
/// let theme = ThemeData::light().primary_color(Color::RED)
///                               .on_primary_color(Color::WHITE);
///
/// assert_eq!(theme.primary_color, Color::RED);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeData {
    /// The primary accent color.
    pub primary_color: Color,
    /// The preferred content color on [`ThemeData::primary_color`].
    pub on_primary_color: Color,
    /// The color behind the main application content.
    pub background_color: Color,
    /// The preferred content color on [`ThemeData::background_color`].
    pub on_background_color: Color,
    /// The color of elevated or grouped surfaces.
    pub surface_color: Color,
    /// The preferred content color on [`ThemeData::surface_color`].
    pub on_surface_color: Color,
}

impl ThemeData {
    /// Creates the default light theme.
    pub const fn new() -> Self {
        Self::light()
    }

    /// Creates Aimer's built-in light theme.
    pub const fn light() -> Self {
        Self {
            primary_color: Color::BLUE,
            on_primary_color: Color::WHITE,
            background_color: Color::WHITE,
            on_background_color: Color::BLACK,
            surface_color: Color::WHITE,
            on_surface_color: Color::BLACK,
        }
    }

    /// Creates Aimer's built-in dark theme.
    pub const fn dark() -> Self {
        Self {
            primary_color: Color::Rgba(144, 202, 249, 255),
            on_primary_color: Color::BLACK,
            background_color: Color::Rgba(18, 18, 18, 255),
            on_background_color: Color::WHITE,
            surface_color: Color::Rgba(30, 30, 30, 255),
            on_surface_color: Color::WHITE,
        }
    }

    /// Sets the primary accent color.
    pub fn primary_color(mut self, color: Color) -> Self {
        self.primary_color = color;
        self
    }

    /// Sets the preferred content color on the primary color.
    pub fn on_primary_color(mut self, color: Color) -> Self {
        self.on_primary_color = color;
        self
    }

    /// Sets the main application background color.
    pub fn background_color(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    /// Sets the preferred content color on the background color.
    pub fn on_background_color(mut self, color: Color) -> Self {
        self.on_background_color = color;
        self
    }

    /// Sets the color of elevated or grouped surfaces.
    pub fn surface_color(mut self, color: Color) -> Self {
        self.surface_color = color;
        self
    }

    /// Sets the preferred content color on the surface color.
    pub fn on_surface_color(mut self, color: Color) -> Self {
        self.on_surface_color = color;
        self
    }

    /// Linearly interpolates every semantic color toward `other`.
    ///
    /// Values of `t` at or below `0.0` return `self`, while values at or above
    /// `1.0` return `other` exactly.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        if t <= 0.0 {
            return self;
        }
        if t >= 1.0 {
            return other;
        }
        Self {
            primary_color: self
                .primary_color
                .lerp(other.primary_color, t),
            on_primary_color: self
                .on_primary_color
                .lerp(other.on_primary_color, t),
            background_color: self
                .background_color
                .lerp(other.background_color, t),
            on_background_color: self
                .on_background_color
                .lerp(other.on_background_color, t),
            surface_color: self
                .surface_color
                .lerp(other.surface_color, t),
            on_surface_color: self
                .on_surface_color
                .lerp(other.on_surface_color, t),
        }
    }
}

impl Default for ThemeData {
    fn default() -> Self {
        Self::new()
    }
}

/// Accesses the nearest theme supplied by an [`crate::AnimatedTheme`] ancestor.
///
/// Use [`Theme::of`] while building themed widgets so they rebuild as the theme
/// animates. Use [`Theme::read`] when the caller only needs the current value
/// and should not subscribe to future changes.
pub struct Theme;

impl Theme {
    /// Returns the current theme and subscribes the building widget to theme
    /// changes.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`crate::AnimatedTheme`] ancestor or when called
    /// outside a widget build.
    pub fn of(context: &BuildContext) -> Snapshot<ThemeData> {
        context.watch::<ThemeData>()
    }

    /// Returns the current theme without subscribing the building widget to
    /// theme changes.
    ///
    /// # Panics
    ///
    /// Panics when there is no [`crate::AnimatedTheme`] ancestor.
    pub fn read(context: &BuildContext) -> Snapshot<ThemeData> {
        context.read::<ThemeData>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(color: Color) -> ThemeData {
        ThemeData::new()
            .primary_color(color)
            .on_primary_color(color)
            .background_color(color)
            .on_background_color(color)
            .surface_color(color)
            .on_surface_color(color)
    }

    #[test]
    fn lerp_preserves_endpoints() {
        let begin = theme(Color::Rgba(10, 20, 30, 40));
        let end = theme(Color::Rgba(110, 120, 130, 140));

        assert_eq!(begin.lerp(end, 0.0), begin);
        assert_eq!(begin.lerp(end, 1.0), end);
    }

    #[test]
    fn lerp_interpolates_every_semantic_color() {
        let begin = theme(Color::Rgba(0, 20, 40, 60));
        let end = theme(Color::Rgba(100, 120, 140, 160));
        let expected = theme(Color::Rgba(50, 70, 90, 110));

        assert_eq!(begin.lerp(end, 0.5), expected);
    }
}
