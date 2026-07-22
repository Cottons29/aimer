use std::time::Duration;

use aimer::macros::widget;
use aimer::style::*;
use aimer::*;

pub fn start_animated_theme_example() {
    AimerApp::start(AnimatedThemeExample::new())
}

#[widget(Stateful)]
pub struct AnimatedThemeExample {}

impl AnimatedThemeExample {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct AnimatedThemeExampleState {
    is_dark: bool,
    updater: StateUpdater<Self>,
}

impl StatefulWidget for AnimatedThemeExample {
    type State = AnimatedThemeExampleState;

    fn create_state(&self) -> Self::State {
        AnimatedThemeExampleState {
            is_dark: false,
            updater: StateUpdater::empty(),
        }
    }
}

impl State<AnimatedThemeExample> for AnimatedThemeExampleState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AnimatedTheme::new()
            .data(if self.is_dark {
                ThemeData::dark()
            } else {
                ThemeData::light()
            })
            .duration(Duration::from_millis(400))
            .curve(Curve::EaseInOut)
            .child(ThemedPanel::new(self.is_dark, self.updater.clone()))
    }
}

#[widget(Stateless)]
#[derive(Clone)]
struct ThemedPanel {
    is_dark: bool,
    updater: StateUpdater<AnimatedThemeExampleState>,
}

impl ThemedPanel {
    fn new(is_dark: bool, updater: StateUpdater<AnimatedThemeExampleState>) -> Self {
        Self { is_dark, updater }
    }
}

impl StatelessWidget for ThemedPanel {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);
        let updater = self.updater.clone();

        Container::new()
            .color(theme.background_color)
            .child(
                Column::new()
                    .horizontal_alignment(BoxAlignment::Center)
                    .vertical_alignment(BoxAlignment::Center)
                    .children([
                        Text::new("AnimatedTheme")
                            .text_style(
                                TextStyle::new()
                                    .font_size(32)
                                    .color(theme.on_background_color),
                            )
                            .boxed(),
                        SizedBox::new()
                            .height(24)
                            .boxed(),
                        Text::new("Colors interpolate while the widget tree keeps its state.")
                            .text_style(
                                TextStyle::new()
                                    .font_size(18)
                                    .color(theme.on_background_color),
                            )
                            .boxed(),
                        SizedBox::new()
                            .height(32)
                            .boxed(),
                        Container::new()
                            .width(Dimension::Px(220.0))
                            .height(Dimension::Px(56.0))
                            .child(
                                Button::new()
                                    .on_press(move || {
                                        updater.set_state(|state| state.is_dark = !state.is_dark);
                                    })
                                    .decoration(
                                        BoxDecoration::new()
                                            .background_color(theme.primary_color)
                                            .border_radius(12),
                                    )
                                    .child(
                                        Text::new(if self.is_dark {
                                            "Switch to light theme"
                                        } else {
                                            "Switch to dark theme"
                                        })
                                        .text_align(TextAlign::MidCenter)
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(16)
                                                .color(theme.on_primary_color),
                                        ),
                                    ),
                            )
                            .boxed(),
                    ]),
            )
    }
}
