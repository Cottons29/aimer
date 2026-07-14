use aimer::Dimension::Px;
use aimer::router::NavigatorController;
use aimer::style::{
    BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontWeight, LayoutSpacing, TextDecoration,
    TextStyle,
};
use aimer::*;
use aimer::{BuildContext, Container, State, StateUpdater, StatefulWidget, Text, Widget, widget};

use crate::router::AppRouter;

#[widget(Stateful)]
pub struct HeaderSection {
    pub show_logo: bool,
    /// Index of the active section (0 = Home, 1 = Docs, 2 = Learn), used to
    /// highlight the matching header button.
    pub active_tab: usize,
}

pub struct HeaderState {
    tab: usize,
    updater: StateUpdater<Self>,
    show_logo: bool,
}

impl StatefulWidget for HeaderSection {
    type State = HeaderState;

    fn create_state(&self) -> Self::State {
        Self::State {
            tab: self.active_tab,
            updater: StateUpdater::new(),
            show_logo: self.show_logo,
        }
    }
}

impl State<HeaderSection> for HeaderState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.show_logo = new.show_logo;
        self.tab = new.tab;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let children = vec![
            SizedBox::new().width(16).boxed(),
            if self.show_logo {
                Text::new("Aimer")
                    .text_style(
                        TextStyle::new()
                            .text_decoration(TextDecoration::Underline)
                            .font_weight(FontWeight::Bolder)
                            .font_size(24)
                            .color(Color::BLACK),
                    )
                    .boxed()
            } else {
                SizedBox::new().width(100).boxed()
            },
            SizedBox::new().width(16).boxed(),
            Expanded::new()
                .child(
                    Container::new().child(
                        Row::new()
                            .horizontal_alignment(BoxAlignment::End)
                            .vertical_alignment(BoxAlignment::Center)
                            .gaps(LayoutSpacing::new().left(24))
                            .children(Self::build_platform_button_list(self, ctx)),
                    ),
                )
                .boxed(),
            SizedBox::new().width(16).boxed(),
        ];

        Container::new()
            .color(Color::WHITE)
            .height(Px(60.0))
            .box_decoration(
                BoxDecoration::new().border(
                    BoxBorder::new().bottom(
                        BorderSlice::new()
                            .stroke(Px(1.0))
                            .color(Color::BLACK.with_opacity(48))
                            .style(BorderStyle::Solid),
                    ),
                ),
            )
            .child(Row::new().vertical_alignment(BoxAlignment::Center).children(children))
    }
}

impl HeaderState {
    const SECTIONS: &[&str] = &["Home", "Docs", "Learn"];

    /// Resolve a section index to the route it navigates to.
    fn route_for(index: usize) -> AppRouter {
        match index {
            1 => AppRouter::Docs,
            2 => AppRouter::Learn,
            _ => AppRouter::Home,
        }
    }

    fn build_platform_button_list(&self, ctx: &BuildContext) -> Vec<Box<dyn Widget>> {
        let selected = self.tab;
        // Reach the enclosing Navigator so header buttons can drive navigation.
        let navigator = NavigatorController::<AppRouter>::of(ctx);
        Self::SECTIONS
            .iter()
            .enumerate()
            .map({
                move |(i, l)| {
                    let index = i;
                    let is_selected = index == selected;
                    let font_weight =
                        if selected == index { FontWeight::Bolder } else { FontWeight::Normal };

                    TextButton::new(*l)
                        .style(
                            TextStyle::new()
                                .font_size(20)
                                .color(if is_selected { Color::BLUE } else { Color::BLACK })
                                .font_weight(font_weight)
                                .text_decoration(if is_selected {
                                    TextDecoration::Underline
                                } else {
                                    TextDecoration::None
                                }),
                        )
                        .hover_style(
                            TextStyle::new()
                                .font_size(20)
                                .color(if is_selected {
                                    Color::BLUE
                                } else {
                                    Color::BLUE.lighten(0.6)
                                })
                                .font_weight(font_weight)
                                .text_decoration(TextDecoration::Underline),
                        )
                        .on_press({
                            let navigator = navigator.clone();
                            move || {
                                println!("Tab {} pressed", index);
                                if index != selected {
                                    navigator.push(Self::route_for(index));
                                }
                            }
                        })
                        .boxed()
                }
            })
            .collect()
    }
}
