
use aimer::*;
use aimer::console::debug;
use aimer::macros::widget;
use aimer::router::NavigatorController;
use aimer::style::*;
use crate::routing::AppRouting;

#[widget(Stateless)]
#[derive(Clone)]
pub struct ProfilePage{
    name: String,
}

impl StatelessWidget for ProfilePage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        debug!("Building HomeWidget");
        Container!(
            // color: Colors::Green,
            // margin: LayoutSpacing!(top : Spacing::Px(10)),
            child: Column!(
                horizontal_alignment: BoxAlignment::Center,
                vertical_alignment: BoxAlignment::Center,
                gaps: LayoutSpacing!(top: Spacing::Px(40)),
                children: [
                    Text!(
                        format!("Hello, {}", self.name),
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle!(
                            color: Colors::Black,
                        )
                    ),

                    Row!(
                        gaps: LayoutSpacing!(right: Spacing::Px(10)),
                        children: [
                            Button!(
                                on_press:  {
                                    let navi = NavigatorController::<AppRouting>::of(ctx);
                                    move || {
                                        navi.pop();
                                        println!("Loading the response from example.com");
                                    }
                                },
                                decoration: BoxDecoration!(background_color: Colors::Blue),
                                child: Container!(
                                    width: 200,
                                    height: 50,
                                    child: Text!(
                                        "Back",
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
                                        navi.push(AppRouting::Settings);
                                    }
                                },
                                decoration: BoxDecoration!(background_color: Colors::Blue),
                                child: Container!(
                                    width: 200,
                                    height: 50,
                                    child: Text!(
                                        "Setting page",
                                        text_align: TextAlign::MidCenter,
                                        text_style: TextStyle!(
                                            color: Colors::White,
                                        )
                                    )
                                )
                            ),
                        ],
                    )
                ]
            )
        )
    }
}