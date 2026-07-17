use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(test)]
use std::thread::{sleep, spawn};
use std::time::Duration;
// use std::time::Duration;

use aimer::animation::{AnimatedSwitcher, Curve};
use aimer::style::{FontWeight, LayoutSpacing, Spacing, TextDecoration, TextStyle};
use aimer::*;

use crate::utils::{app_padding, is_mobile, mobile_title};

#[widget(Stateful)]
pub struct SameLookingSection {
    pub key: Option<Key>,
}

pub static TEST_CLICKED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub static TEST_STATE_UPDATED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);

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
        SameLookingSectionState { current_index: 0, state: StateUpdater::new() }
    }
}

const PLATFORMS: &[&str] = &["macOS", "iOS", "Web", "Android"];
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
        #[cfg(test)]
        {
            let updater_2 = updater.clone();
            let is_clicked = TEST_CLICKED.load(Ordering::Relaxed);
            if !is_clicked {
                spawn(move || {
                    sleep(Duration::from_millis(150));
                    TEST_CLICKED.store(true, Ordering::Relaxed);
                    updater_2.set_state(|state| state.current_index = 1);
                });
            }
        }

        self.state = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        #[cfg(test)]
        {
            // no need to change this
            TEST_STATE_UPDATED.fetch_or(self.current_index == 1, Ordering::Relaxed);
            // no need to change this because i need to know the current index after resize
            CURRENT_INDEX.store(self.current_index, Ordering::Relaxed);
        }
        // eprintln!("Current index: {}", self.current_index);
        Container::new()
            .color(Color::WHITE)
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
                                        .color(Color::BLACK)
                                        .font_weight(FontWeight::Bolder)
                                        .text_decoration(TextDecoration::Underline),
                                ),
                            )
                            .boxed(),
                        SizedBox::new().height(24).boxed(),
                        Container::new()
                            .height(if is_mobile(ctx) { 250 } else { 450 })
                            .child(platform_image_switcher(self.current_index))
                            .boxed(),
                        SizedBox::new().height(40).boxed(),
                        Row::new()
                            .horizontal_alignment(BoxAlignment::Center)
                            .vertical_alignment(BoxAlignment::Center)
                            .gaps(LayoutSpacing::horizontal(Spacing::Px(8)))
                            .children(self.build_platform_button_list(ctx))
                            .boxed(),
                        SizedBox::new().height(40).boxed(),
                    ]),
            )
    }
}

impl SameLookingSectionState {
    fn build_platform_button_list(&self, _ctx: &BuildContext) -> Vec<Box<dyn Widget>> {
        let selected = self.current_index;
        PLATFORMS
            .iter()
            .enumerate()
            .map({
                let updater = self.state.clone();
                move |(i, l)| {
                    let index = i;
                    let is_selected = index == selected;
                    let font_weight =
                        if selected == index { FontWeight::Bolder } else { FontWeight::Normal };

                    TextButton::new(*l)
                        .style(
                            TextStyle::new()
                                .font_size(20)
                                .color(if is_selected { Color::BLUE } else { Color::BLACK })
                                .font_weight(font_weight)
                                .text_decoration(if is_selected {
                                    TextDecoration::Underline
                                } else {
                                    TextDecoration::None
                                }),
                        )
                        .hover_style(
                            TextStyle::new()
                                .font_size(20)
                                .color(if is_selected {
                                    Color::BLUE
                                } else {
                                    Color::BLUE.lighten(0.6)
                                })
                                .font_weight(font_weight)
                                .text_decoration(TextDecoration::Underline),
                        )
                        .on_press({
                            let updater = updater.clone();
                            move || {
                                // println!("Tab {} pressed", index);
                                if updater.read_state().current_index != index {
                                    updater.set_state(move |s| s.current_index = index);
                                }
                            }
                        })
                        .boxed()
                }
            })
            .collect()
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
        assert_eq!(platform_image(PLATFORM_IMAGE.len()), "assets/macos_screenshot.png");
    }

    #[test]
    fn platform_image_transition_has_stable_switcher_identity() {
        assert_eq!(
            Widget::key(&platform_image_switcher(0)),
            Some(Key::Value(PLATFORM_IMAGE_SWITCHER_KEY.to_owned()))
        );
    }
}
