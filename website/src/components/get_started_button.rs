// ---------------------------------------------------------------------------
// Hoverable Get Started button
// ---------------------------------------------------------------------------

use aimer::style::{BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontWeight, TextAlign, TextStyle, Theme, ThemeData};
use aimer::{BuildContext, Widget, widget, *};

#[widget(Stateless)]
#[derive(Clone)]
pub struct HoverableGetStartedButton {}

impl StatelessWidget for HoverableGetStartedButton {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::copied(ctx);

        Container::new().child(
            Button::new()
                .decoration(
                    BoxDecoration::new()
                        .background_color(Color::BLACK)
                        .border(BoxBorder::all(
                            BorderSlice::new()
                                .color(theme.on_background_color)
                                .style(BorderStyle::Solid)
                                .stroke(2),
                        ))
                        .border_radius(8),
                )
                .on_press({
                    move || {
                        println!("Button pressed");
                        let url = "https://github.com/Cottons29/aimer";
                        if let Err(e) = webbrowser::open(url) {
                            eprintln!("Failed to open browser: {}", e);
                        }
                    }
                })
                .child(
                    Row::new()
                        .vertical_alignment(BoxAlignment::Center)
                        .horizontal_alignment(BoxAlignment::Center)
                        .children(vec![
                            Box::new(
                                AssetImage::new("assets/github-svgrepo-com.png")
                                    .width(24)
                                    .height(24),
                            ),
                            SizedBox::new()
                                .width(20)
                                .boxed(),
                            Text::new("Get Started!")
                                .text_align(TextAlign::MidCenter)
                                .text_style(
                                    TextStyle::new()
                                        .color(Color::WHITE)
                                        .font_size(18)
                                        .font_weight(FontWeight::Bold),
                                )
                                .boxed(),
                        ]),
                ),
        )
    }
}
