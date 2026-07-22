use std::cell::Cell;
use std::rc::Rc;

use aimer::callback::VoidCallback;
use aimer::gesture::gesture_detector::GestureDetector;
use aimer::mouse_region::{MouseRegion, PointerState};
use aimer::style::{TextAlign, TextDecoration, TextStyle, Theme, ThemeData};
use aimer::{
    BuildContext, Color, Row, SizedBox, State, StateUpdater, StatefulWidget, Svg, SvgDocument,
    SvgStyle, Text, Widget, widget,
};

#[widget(Stateful)]
pub struct BlogBackButton {
    on_click: VoidCallback,
}

impl BlogBackButton {
    pub fn new() -> Self {
        Self {
            on_click: VoidCallback::default(),
        }
    }

    pub fn on_click(mut self, on_click: impl Into<VoidCallback>) -> Self {
        self.on_click = on_click.into();
        self
    }
}

impl Default for BlogBackButton {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BlogBackButtonState {
    is_hover: bool,
    on_click: VoidCallback,
    current_state: Rc<Cell<PointerState>>,
    updater: StateUpdater<Self>,
}

impl StatefulWidget for BlogBackButton {
    type State = BlogBackButtonState;

    fn create_state(&self) -> Self::State {
        Self::State {
            is_hover: false,
            on_click: self.on_click.clone(),
            current_state: Rc::default(),
            updater: StateUpdater::new(),
        }
    }
}

fn back_label_style(is_hover: bool, color: Color) -> TextStyle {
    TextStyle::new()
        .color(color)
        .text_decoration(if is_hover {
            TextDecoration::Underline
        } else {
            TextDecoration::None
        })
}

impl State<BlogBackButton> for BlogBackButtonState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);
        let document = SvgDocument::from_svg(include_bytes!("../../assets/back-svgrepo-com.svg"))
            .expect("the bundled SVG should be valid");

        MouseRegion::new()
            .on_hover_enter({
                let updater = self.updater.clone();
                move || updater.set_state(|state| state.is_hover = true)
            })
            .on_hover_exit({
                let updater = self.updater.clone();
                move || updater.set_state(|state| state.is_hover = false)
            })
            .current_state(self.current_state.clone())
            .child(
                GestureDetector::new()
                    .on_tap(self.on_click.clone())
                    .child(
                        Row::new().children([
                            Svg::new(document)
                                .style(
                                    "#back_button_body",
                                    SvgStyle::new().fill(theme.on_background_color),
                                )
                                .style(
                                    "#back_button_head",
                                    SvgStyle::new().fill(theme.on_background_color),
                                )
                                .width(16)
                                .height(16)
                                .boxed(),
                            SizedBox::new()
                                .width(8)
                                .boxed(),
                            Text::new("Back to blogs")
                                .text_align(TextAlign::MidCenter)
                                .text_style(back_label_style(
                                    self.is_hover,
                                    theme.on_background_color,
                                ))
                                .boxed(),
                        ]),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use aimer::style::TextDecorationLine;

    use super::*;

    #[test]
    fn back_label_is_not_underlined_when_not_hovered() {
        assert_eq!(
            back_label_style(false, Color::BLACK)
                .text_decoration
                .line,
            TextDecorationLine::NONE
        );
    }

    #[test]
    fn back_label_is_underlined_when_hovered() {
        assert_eq!(
            back_label_style(true, Color::BLACK)
                .text_decoration
                .line,
            TextDecorationLine::UNDERLINE
        );
    }
}
