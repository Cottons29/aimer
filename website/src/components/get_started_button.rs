// ---------------------------------------------------------------------------
// Hoverable Get Started button
// ---------------------------------------------------------------------------

use aimer::style::{
    BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontWeight, TextAlign, TextStyle, Theme,
    ThemeData,
};
use aimer::{BuildContext, Svg, SvgDocument, Widget, widget, *};

const GITHUB_ICON_SVG: &[u8] = include_bytes!("../../assets/github-svgrepo-com.svg");

fn github_icon() -> Svg {
    Svg::new(
        SvgDocument::from_svg(GITHUB_ICON_SVG)
            .expect("the bundled GitHub icon SVG should be valid"),
    )
    .width(24)
    .height(24)
}

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
                            github_icon().boxed(),
                            SizedBox::new().width(20).boxed(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_github_icon_is_valid_svg() {
        assert!(SvgDocument::from_svg(GITHUB_ICON_SVG).is_ok());
    }
}
