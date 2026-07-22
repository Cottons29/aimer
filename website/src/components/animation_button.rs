use std::time::Duration;

use aimer::animation::{Animatable, Curve, ImplicitAnimatedBuilder, Rgba};
use aimer::callback::Callback;
use aimer::console::debug;
use aimer::style::{
    BoxDecoration, FontWeight, LayoutSpacing, Spacing, TextAlign, TextStyle, Theme, ThemeData,
};
use aimer::*;

pub(crate) const PLATFORMS: &[&str] = &["macOS", "iOS", "Web", "Android"];
const PLATFORM_BUTTON_TRANSITION_DURATION: Duration = Duration::from_millis(240);

#[widget(Stateful)]
pub struct AnimatedPlatformButtonList {
    pub key: Option<Key>,
    selected_index: usize,
    compact: bool,
    on_selected: Callback<usize, ()>,
}

impl AnimatedPlatformButtonList {
    pub fn new() -> Self {
        Self {
            key: None,
            selected_index: 0,
            compact: false,
            on_selected: Callback::default(),
        }
    }

    pub fn selected_index(mut self, selected_index: usize) -> Self {
        self.selected_index = selected_index;
        self
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    pub fn on_selected(mut self, on_selected: impl Into<Callback<usize, ()>>) -> Self {
        self.on_selected = on_selected.into();
        self
    }
}

impl Default for AnimatedPlatformButtonList {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AnimatedPlatformButtonListState {
    selected_index: usize,
    compact: bool,
    on_selected: Callback<usize, ()>,
    updater: StateUpdater<Self>,
}

impl StatefulWidget for AnimatedPlatformButtonList {
    type State = AnimatedPlatformButtonListState;

    fn create_state(&self) -> Self::State {
        AnimatedPlatformButtonListState {
            selected_index: self.selected_index,
            compact: self.compact,
            on_selected: self.on_selected.clone(),
            updater: StateUpdater::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PlatformButtonVisual {
    background_color: Color,
    foreground_color: Color,
    animation_key: String,
}

fn platform_button_visual(
    index: usize,
    is_selected: bool,
    theme: &ThemeData,
) -> PlatformButtonVisual {
    PlatformButtonVisual {
        background_color: if is_selected {
            theme.primary_color
        } else {
            theme
                .primary_color
                .lighten(0.38)
        },
        foreground_color: if is_selected {
            theme.on_primary_color
        } else {
            theme
                .primary_color
                .darken(0.35)
        },
        animation_key: format!("platform-button-{index}"),
    }
}

fn platform_button_animation_target(is_selected: bool) -> f32 {
    if is_selected { 1.0 } else { 0.0 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlatformButtonFrame {
    background_color: Rgba,
    foreground_color: Rgba,
    checkmark_color: Rgba,
    checkmark_gap: f32,
}

fn platform_button_frame(
    progress: f32,
    inactive_background: Rgba,
    active_background: Rgba,
    inactive_foreground: Rgba,
    active_foreground: Rgba,
) -> PlatformButtonFrame {
    let progress = progress.clamp(0.0, 1.0);
    let transparent_checkmark = Rgba {
        a: 0.0,
        ..active_foreground
    };
    PlatformButtonFrame {
        background_color: inactive_background.lerp(&active_background, progress),
        foreground_color: inactive_foreground.lerp(&active_foreground, progress),
        checkmark_color: transparent_checkmark.lerp(&active_foreground, progress),
        checkmark_gap: 1.0 + 4.0 * progress,
    }
}

impl State<AnimatedPlatformButtonList> for AnimatedPlatformButtonListState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.selected_index = new.selected_index;
        self.compact = new.compact;
        self.on_selected = new.on_selected.clone();
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        Row::new()
            .horizontal_alignment(BoxAlignment::Center)
            .vertical_alignment(BoxAlignment::Center)
            .gaps(LayoutSpacing::horizontal(Spacing::Px(2)))
            .box_children(self.build_platform_button_list(&ThemeData::of(ctx)))
    }
}

impl AnimatedPlatformButtonListState {
    fn build_platform_button_list(&self, theme: &ThemeData) -> Vec<AnyWidget> {
        PLATFORMS
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let compact = self.compact;
                let is_selected = index == self.selected_index;
                let visual = platform_button_visual(index, is_selected, theme);
                let active_background = Rgba::from_color(
                    &theme
                        .background_color
                        .invert(),
                );
                let inactive_background = Rgba::from_color(&Color::GRAY.with_opacity(70));
                let active_foreground = Rgba::from_color(&theme.on_primary_color);
                let inactive_foreground = Rgba::from_color(&Color::GRAY.with_opacity(90));
                let border_radius_active = if compact { 18 } else { 24 };
                let border_radius_inactive = if compact { 6 } else { 8 };
                let label = *label;
                let normal_width = if compact { 72f32 } else { 144f32 };
                let slected = normal_width * 1.3;

                let animated_surface = ImplicitAnimatedBuilder::new(
                    platform_button_animation_target(is_selected),
                    PLATFORM_BUTTON_TRANSITION_DURATION,
                    Curve::FastOutSlowIn,
                    move |progress| {
                        debug!(
                            "platform: {label}, selected: {is_selected}, progress: {progress:?}"
                        );
                        let border_radius =
                            border_radius_inactive.lerp(&border_radius_active, *progress);
                        let width = normal_width.lerp(&slected, *progress);
                        let frame = platform_button_frame(
                            *progress,
                            inactive_background,
                            active_background,
                            inactive_foreground,
                            active_foreground,
                        );
                        let content = Row::new()
                            .horizontal_alignment(BoxAlignment::Center)
                            .vertical_alignment(BoxAlignment::Center)
                            .children(vec![
                                if is_selected {
                                    Text::new("✓")
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(if compact { 18 } else { 22 })
                                                .color(
                                                    frame
                                                        .checkmark_color
                                                        .to_color(),
                                                )
                                                .font_weight(FontWeight::Bolder),
                                        )
                                        .boxed()
                                } else {
                                    SizedBox::new()
                                        .width(0)
                                        .boxed()
                                },
                                SizedBox::new()
                                    .width(frame.checkmark_gap)
                                    .boxed(),
                                Text::new(label)
                                    .text_align(TextAlign::MidCenter)
                                    .text_style(
                                        TextStyle::new()
                                            .font_size(if compact { 15 } else { 20 })
                                            .color(
                                                frame
                                                    .foreground_color
                                                    .to_color(),
                                            )
                                            .font_weight(FontWeight::Bolder),
                                    )
                                    .boxed(),
                            ]);

                        Container::new()
                            .width(width)
                            .height(if compact { 38 } else { 44 })
                            .box_decoration(
                                BoxDecoration::new()
                                    .background_color(
                                        frame
                                            .background_color
                                            .to_color(),
                                    )
                                    .border_radius(border_radius),
                            )
                            .child(content)
                    },
                )
                .key(format!("platform-button-animation-{index}"));
                let on_selected = self.on_selected.clone();

                Button::new()
                    .on_press(move || {
                        on_selected.call(index);
                    })
                    .child(animated_surface)
                    .key(visual.animation_key)
                    .boxed()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use aimer::style::ThemeData;
    use aimer::{StatefulWidget, Widget};

    use super::*;

    #[test]
    fn state_adopts_updated_selection_and_layout() {
        let mut state = AnimatedPlatformButtonList::new()
            .selected_index(0)
            .compact(false)
            .create_state();
        let updated = AnimatedPlatformButtonList::new()
            .selected_index(2)
            .compact(true)
            .create_state();

        state.adopt_config_from(&updated);

        assert_eq!(state.selected_index, 2);
        assert!(state.compact);
    }

    #[test]
    fn platform_button_list_exposes_distinct_sibling_keys() {
        let state = AnimatedPlatformButtonList::new().create_state();
        let buttons = state.build_platform_button_list(&ThemeData::default());
        let keys = buttons
            .iter()
            .map(|button| button.key())
            .collect::<Vec<_>>();

        assert_eq!(keys.len(), PLATFORMS.len());
        assert!(
            keys.iter()
                .all(Option::is_some)
        );
        assert_ne!(keys[0], keys[1]);
        assert_ne!(keys[1], keys[2]);
    }

    #[test]
    fn selected_platform_button_uses_active_pill_visuals() {
        let theme = ThemeData::default();
        let visual = platform_button_visual(1, true, &theme);

        assert_eq!(visual.background_color, theme.primary_color);
        assert_eq!(visual.foreground_color, theme.on_primary_color);
        assert_eq!(platform_button_animation_target(true), 1.0);
    }

    #[test]
    fn inactive_platform_button_uses_muted_pill_visuals() {
        let theme = ThemeData::default();
        let visual = platform_button_visual(1, false, &theme);

        assert_eq!(
            visual.background_color,
            theme
                .primary_color
                .lighten(0.38)
        );
        assert_eq!(
            visual.foreground_color,
            theme
                .primary_color
                .darken(0.35)
        );
        assert_eq!(platform_button_animation_target(false), 0.0);
    }

    #[test]
    fn platform_button_animation_identity_is_stable_per_platform() {
        let theme = ThemeData::default();
        let active = platform_button_visual(2, true, &theme);
        let inactive = platform_button_visual(2, false, &theme);
        let other = platform_button_visual(3, false, &theme);

        assert_eq!(active.animation_key, inactive.animation_key);
        assert_ne!(active.animation_key, other.animation_key);
    }

    #[test]
    fn platform_button_has_a_distinct_midpoint_frame() {
        let theme = ThemeData::default();
        let inactive_background = Rgba::from_color(&Color::GRAY.with_opacity(70));
        let active_background = Rgba::from_color(
            &theme
                .background_color
                .invert(),
        );
        let inactive_foreground = Rgba::from_color(&Color::GRAY.with_opacity(90));
        let active_foreground = Rgba::from_color(&theme.on_primary_color);

        let start = platform_button_frame(
            0.0,
            inactive_background,
            active_background,
            inactive_foreground,
            active_foreground,
        );
        let midpoint = platform_button_frame(
            0.5,
            inactive_background,
            active_background,
            inactive_foreground,
            active_foreground,
        );
        let end = platform_button_frame(
            1.0,
            inactive_background,
            active_background,
            inactive_foreground,
            active_foreground,
        );

        assert_ne!(midpoint.background_color, start.background_color);
        assert_ne!(midpoint.background_color, end.background_color);
        assert!(midpoint.checkmark_gap > start.checkmark_gap);
        assert!(midpoint.checkmark_gap < end.checkmark_gap);
        assert!(midpoint.checkmark_color.a > start.checkmark_color.a);
        assert!(midpoint.checkmark_color.a < end.checkmark_color.a);
    }
}
