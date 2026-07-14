use crate::routing::AppRouting;
use aimer::console::debug;
use aimer::macros::widget;
use aimer::router::NavigatorController;
use aimer::style::*;
use aimer::*;

#[widget(Stateless)]
#[derive(Clone)]
pub struct HomeWidget {}

impl StatelessWidget for HomeWidget {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        debug!("Building HomeWidget");
        Container::new().color(Colors::Green.into()).child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Center)
                .vertical_alignment(BoxAlignment::Center)
                .children(vec![
                    Text::new("Home Page")
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
                                        navi.push(AppRouting::Settings);
                                    }
                                })
                                .decoration(BoxDecoration::new().background_color(Colors::Blue))
                                .child(
                                    Container::new()
                                        .width(Dimension::Px(200.0))
                                        .height(Dimension::Px(50.0))
                                        .child(
                                            Text::new("Setting")
                                                .text_align(TextAlign::MidCenter)
                                                .text_style(TextStyle::new().color(Colors::White)),
                                        ),
                                )
                                .boxed(),
                            Button::new()
                                .on_press({
                                    let navi = NavigatorController::<AppRouting>::of(ctx);
                                    move || {
                                        navi.push(AppRouting::Profile { name: "John".to_string() });
                                    }
                                })
                                .decoration(BoxDecoration::new().background_color(Colors::Blue))
                                .child(
                                    Container::new()
                                        .width(Dimension::Px(200.0))
                                        .height(Dimension::Px(50.0))
                                        .child(
                                            Text::new("Profile")
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
