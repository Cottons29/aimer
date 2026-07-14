// use aimer::flex::{Column, };
use crate::routing::AppRouting;
use aimer::console::debug;
use aimer::macros::widget;
use aimer::router::NavigatorController;
use aimer::style::*;
use aimer::*;

#[widget(Stateless)]
#[derive(Clone)]
pub struct SettingPage {}

impl StatelessWidget for SettingPage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        debug!("Building SettingPage");

        Container::new().child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Center)
                .vertical_alignment(BoxAlignment::Center)
                .gaps(LayoutSpacing { top: Spacing::Px(40), ..Default::default() })
                .children(vec![
                    Text::new("Setting Page")
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
                                        navi.push(AppRouting::Profile { name: "Javier".into() });
                                    }
                                })
                                .decoration(BoxDecoration::new().background_color(Colors::Blue))
                                .child(
                                    Container::new()
                                        .width(Dimension::Px(200.0))
                                        .height(Dimension::Px(50.0))
                                        .child(
                                            Text::new("Profile Page")
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
