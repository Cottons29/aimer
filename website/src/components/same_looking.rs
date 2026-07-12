use crate::utils::{app_padding, is_mobile, mobile_title};
use aimer::style::{FontWeight, LayoutSpacing, Spacing, TextDecoration, TextStyle};
use aimer::*;

#[widget(Stateful)]
pub struct SameLookingSection {}

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
            state: StateUpdater::new()
        }
    }
}

const PLATFORMS: &[&str] = &["macOS", "iOS", "Web", "Android"];
const PLATFORM_IMAGE: &[&str] =
    &["assets/macos_screenshot.png", "assets/ios_screenshot.png", "assets/web_screenshot.png", "assets/android_screenshot.png"];

impl State<SameLookingSection> for SameLookingSectionState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.state = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        Container::new()
            .color(Color::WHITE)
            .padding(app_padding(ctx))
            .child(Column::new()
                .horizontal_alignment(BoxAlignment::Center)
                .children(vec![
                    Container::new()
                        .height(100)
                        .child(Text::new("Consistence Looking")
                            .text_style(TextStyle::new()
                                .font_size(mobile_title(ctx))
                                .color(Color::BLACK)
                                .font_weight(FontWeight::Bolder)
                                .text_decoration(TextDecoration::Underline)))
                        .boxed(),
                    SizedBox::new().height(24).boxed(),
                    Container::new()
                        .height(if is_mobile(ctx) { 250 } else { 450 })
                        .child(AssetImage::new(PLATFORM_IMAGE[self.current_index % PLATFORM_IMAGE.len()]))
                        .boxed(),
                    SizedBox::new().height(40).boxed(),
                    Row::new()
                        .horizontal_alignment(BoxAlignment::Center)
                        .vertical_alignment(BoxAlignment::Center)
                        .gaps(LayoutSpacing::horizontal(Spacing::Px(8)))
                        .children(self.build_platform_button_list(ctx))
                        .boxed(),
                    SizedBox::new().height(40).boxed(),
                ])
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
                    let font_weight = if selected == index { FontWeight::Bolder } else { FontWeight::Normal };

                    TextButton::new(*l)
                        .style(TextStyle::new()
                            .font_size(20)
                            .color(if is_selected { Color::BLUE } else { Color::BLACK })
                            .font_weight(font_weight)
                            .text_decoration(if is_selected {
                                TextDecoration::Underline
                            } else {
                                TextDecoration::None
                            }))
                        .hover_style(TextStyle::new()
                            .font_size(20)
                            .color(if is_selected { Color::BLUE } else { Color::BLUE.lighten(0.6) })
                            .font_weight(font_weight)
                            .text_decoration(TextDecoration::Underline))
                        .on_press({
                            let updater = updater.clone();
                            move || {
                                // println!("Tab {} pressed", index);
                                if updater.read_state().current_index != index {
                                    updater.set_state(
                                        move |s| {
                                        s.current_index = index
                                        }
                                    );
                                }
                            }
                        })
                        .boxed()
                }
            })
            .collect()
    }
}
