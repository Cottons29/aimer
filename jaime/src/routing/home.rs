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
        Container!(
            color: Colors::Green,
            // margin: LayoutSpacing!(top : Spacing::Px(10)),
            child: Column!(
                horizontal_alignment: BoxAlignment::Center,
                vertical_alignment: BoxAlignment::Center,
                // gaps: LayoutSpacing!(top: Spacing::Px(40)),
                children: [
                    Text!(
                        "Home Page",
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle!(
                            color: Colors::Black,
                        )
                    ),

                    Row!(
                        gaps: LayoutSpacing!(right: Spacing::Px(10)),
                        children: [
                            Button!(
                                on_press: {
                                    let navi = NavigatorController::<AppRouting>::of(ctx);
                                    move || {
                                       navi.push(AppRouting::Settings);
                                    }
                                },
                                decoration: BoxDecoration!(background_color: Colors::Blue),
                                child: Container!(
                                    width: 200,
                                    height: 50,
                                    child: Text!(
                                        "Setting",
                                        text_align: TextAlign::MidCenter,
                                        text_style: TextStyle!(
                                            color: Colors::White,
                                        )
                                    )
                                )
                            ),
                            Button!(
                                on_press: {
                                    let navi = NavigatorController::<AppRouting>::of(ctx);
                                    move || {
                                        navi.push(AppRouting::Profile{name: "John".to_string()});
                                    }
                                },
                                decoration: BoxDecoration!(background_color: Colors::Blue),
                                child: Container!(
                                    width: 200,
                                    height: 50,
                                    child: Text!(
                                        "Profile",
                                        text_align: TextAlign::MidCenter,
                                        text_style: TextStyle!(
                                            color: Colors::White,
                                        )
                                    )
                                )
                            )
                        ]
                    )
                ]
            )
        )
    }
}
