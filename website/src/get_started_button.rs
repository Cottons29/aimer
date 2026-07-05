// ---------------------------------------------------------------------------
// Hoverable Get Started button
// ---------------------------------------------------------------------------

use aimer::style::{BoxDecoration, FontWeight, TextAlign, TextStyle};
use aimer::*;
use aimer::{widget, BuildContext, Widget};

#[widget(Stateless)]
#[derive(Clone)]
pub struct HoverableGetStartedButton {}

impl StatelessWidget for HoverableGetStartedButton {
    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        Container!(
            child: Button!(
                decoration: BoxDecoration!(
                    background_color: Colors::Black,
                    border_radius: 8,
                ),
                on_press: {
                    move || {
                        println!("Button pressed");
                        let url = "https://github.com/Cottons29/aimer";
                        if let Err(e) = webbrowser::open(url) {
                            eprintln!("Failed to open browser: {}", e);
                        }
                    }
                },
                child: Row!(
                    vertical_alignment: BoxAlignment::Center,
                    horizontal_alignment: BoxAlignment::Center,
                    children: [
                        Box::new(AssetImage!(
                            "assets/github-svgrepo-com.png",
                            width: 24,
                            height: 24,
                        )),

                        SizedBox!(width: 20),

                        Text!(
                            "Get Started!",
                            text_align: TextAlign::MidCenter,
                            text_style: TextStyle!(
                                color: Colors::White,
                                font_size: 18,
                                font_weight: FontWeight::Bold,
                                // text_decoration: TextDecoration::Underline,
                            )
                        )
                    ]
                )
            )
        )
    }
}
