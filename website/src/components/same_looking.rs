use std::time::Duration;

use aimer::animation::{AnimatedSwitcher, Curve};
use aimer::style::{FontWeight, TextDecoration, TextStyle, Theme, ThemeData};
use aimer::*;

use crate::components::animation_button::{AnimatedPlatformButtonList, PLATFORMS};
use crate::utils::{app_padding, is_mobile, mobile_title};

#[widget(Stateful)]
pub struct SameLookingSection {
    pub key: Option<Key>,
}

pub struct SameLookingSectionState {
    current_index: usize,
    state: StateUpdater<Self>,
}

impl StatefulWidget for SameLookingSection {
    type State = SameLookingSectionState;

    fn create_state(&self) -> Self::State {
        // The framework preserves the live state across parent rebuilds
        // (e.g. a window resize) by adopting it during reconciliation, so the
        // selected tab survives without any manual persistence — this only
        // needs to provide the initial value.
        SameLookingSectionState {
            current_index: 0,
            state: StateUpdater::new(),
        }
    }
}

const PLATFORM_IMAGE: &[&str] = &[
    "assets/macos_screenshot.png",
    "assets/ios_screenshot.png",
    "assets/web_screenshot.png",
    "assets/android_screenshot.png",
];
const PLATFORM_IMAGE_SWITCHER_KEY: &str = "platform-image-switcher";

fn platform_image(index: usize) -> &'static str {
    PLATFORM_IMAGE[index % PLATFORM_IMAGE.len()]
}

fn platform_image_switcher(index: usize) -> AnimatedSwitcher<AssetImage> {
    AnimatedSwitcher::new(
        Duration::from_millis(350),
        Curve::FastOutSlowIn,
        AssetImage::new(platform_image(index)),
    )
    .child_key(PLATFORMS[index % PLATFORMS.len()])
    .key(PLATFORM_IMAGE_SWITCHER_KEY)
}

impl State<SameLookingSection> for SameLookingSectionState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.state = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);
        // eprintln!("Current index: {}", self.current_index);
        Container::new()
            .color(theme.background_color)
            .padding(app_padding(ctx))
            .child(
                Column::new()
                    .horizontal_alignment(BoxAlignment::Center)
                    .children(vec![
                        Container::new()
                            .height(100)
                            .child(
                                Text::new("Consistence Looking").text_style(
                                    TextStyle::new()
                                        .font_size(mobile_title(ctx))
                                        .color(theme.on_background_color)
                                        .font_weight(FontWeight::Bolder)
                                        .text_decoration(TextDecoration::Underline),
                                ),
                            )
                            .boxed(),
                        SizedBox::new()
                            .height(24)
                            .boxed(),
                        Container::new()
                            .height(if is_mobile(ctx) { 250 } else { 450 })
                            .child(platform_image_switcher(self.current_index))
                            .boxed(),
                        SizedBox::new()
                            .height(40)
                            .boxed(),
                        AnimatedPlatformButtonList::new()
                            .selected_index(self.current_index)
                            .compact(is_mobile(ctx))
                            .on_selected({
                                let updater = self.state.clone();
                                move |index| {
                                    if updater
                                        .read_state()
                                        .current_index
                                        != index
                                    {
                                        updater.set_state(move |state| state.current_index = index);
                                    }
                                }
                            })
                            .boxed(),
                        SizedBox::new()
                            .height(40)
                            .boxed(),
                    ]),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_image_matches_selected_platform() {
        assert_eq!(platform_image(1), "assets/ios_screenshot.png");
    }

    #[test]
    fn platform_image_wraps_out_of_range_index() {
        assert_eq!(
            platform_image(PLATFORM_IMAGE.len()),
            "assets/macos_screenshot.png"
        );
    }

    #[test]
    fn platform_image_transition_has_stable_switcher_identity() {
        assert_eq!(
            Widget::key(&platform_image_switcher(0)),
            Some(Key::Value(PLATFORM_IMAGE_SWITCHER_KEY.to_owned()))
        );
    }
}
