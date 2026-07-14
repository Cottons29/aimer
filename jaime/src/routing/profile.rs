use aimer::console::debug;
use aimer::macros::widget;
use aimer::router::NavigatorController;
use aimer::style::*;
use aimer::*;

use crate::routing::AppRouting;

#[widget(Stateless)]
#[derive(Clone)]
pub struct ProfilePage {
    name: String,
}

impl ProfilePage {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl StatelessWidget for ProfilePage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        debug!("Building HomeWidget");
        Container::new().child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Center)
                .vertical_alignment(BoxAlignment::Center)
                .gaps(LayoutSpacing { top: Spacing::Px(40), ..Default::default() })
                .children(vec![
                    Text::new(format!("Hello, {}", self.name))
                        .text_align(TextAlign::MidCenter)
                        .text_style(TextStyle::new().color(Colors::Black))
                        .boxed(),
                    Row::new()
                        .gaps(LayoutSpacing { right: Spacing::Px(10), ..Default::default() })
                        .children(vec![
                            Button::new()
                                .on_press({
                                    let navi = NavigatorController::<AppRouting>::of(ctx);
                                    move || {
                                        navi.pop();
                                        println!("Loading the response from example.com");
                                    }
                                })
                                .decoration(BoxDecoration::new().background_color(Colors::Blue))
                                .child(
                                    Container::new()
                                        .width(Dimension::Px(200.0))
                                        .height(Dimension::Px(50.0))
                                        .child(
                                            Text::new("Back")
                                                .text_align(TextAlign::MidCenter)
                                                .text_style(TextStyle::new().color(Colors::White)),
                                        ),
                                )
                                .boxed(),
                            Button::new()
                                .on_press({
                                    let navi = NavigatorController::<AppRouting>::of(ctx);
                                    move || {
                                        navi.push(AppRouting::Settings);
                                    }
                                })
                                .decoration(BoxDecoration::new().background_color(Colors::Blue))
                                .child(
                                    Container::new()
                                        .width(Dimension::Px(200.0))
                                        .height(Dimension::Px(50.0))
                                        .child(
                                            Text::new("Setting page")
                                                .text_align(TextAlign::MidCenter)
                                                .text_style(TextStyle::new().color(Colors::White)),
                                        ),
                                )
                                .boxed(),
                        ])
                        .boxed(),
                ]),
        )
    }
}
