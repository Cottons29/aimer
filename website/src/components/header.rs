use aimer::Dimension::Px;
use aimer::router::NavigatorController;
use aimer::style::{BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontWeight, LayoutSpacing, TextDecoration, TextStyle};
use aimer::*;
use aimer::{widget, BuildContext, Container, State, StateUpdater, StatefulWidget, Text, Widget};

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
        Self::State { tab: self.active_tab, updater: StateUpdater::new(), show_logo: self.show_logo }
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
            SizedBox!(width: 16),
            if self.show_logo {
                Text!(
                    "Aimer",
                    text_style: TextStyle!(
                        text_decoration: TextDecoration::Underline,
                        font_weight: FontWeight::Bolder,
                        font_size: 24,
                        color: Color::BLACK,
                    ),
                )
            } else {
                SizedBox!(width: 100)
            },
            SizedBox!(width: 16),
            Expanded!(
                child: Container!(
                    child: Row!(
                        horizontal_alignment: BoxAlignment::End,
                        vertical_alignment: BoxAlignment::Center,
                        gaps:  LayoutSpacing!(
                            left: 24
                        ),
                        children: Self::build_platform_button_list(self, ctx)
                    )
                )
            ),
            SizedBox!(width: 16),
        ];

        Container!(
            color: Color::WHITE,
            height: Px(60.0),
            box_decoration: BoxDecoration!(
                border: BoxBorder!(
                    bottom: BorderSlice! (
                        stroke: Px(1.0),
                        color: Color::BLACK.with_opacity(48),
                        style: BorderStyle::Solid
                    ),
                )
            ),
            child: Row!(
                vertical_alignment: BoxAlignment::Center,
                children: children,
            )
        )
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
                    let font_weight = if selected == index { FontWeight::Bolder } else { FontWeight::Normal };

                    TextButton!(
                        *l,
                        style: TextStyle!(
                            font_size: 20,
                            color: if is_selected { Colors::Blue } else { Colors::Black },
                            font_weight: font_weight,
                            text_decoration: if is_selected {
                                TextDecoration::Underline
                            } else {
                                TextDecoration::None
                            },
                        ),
                        hover_style: TextStyle!(
                            font_size: 20,
                            color: if is_selected { Color::BLUE } else { Color::BLUE.lighten(0.6) },
                            font_weight: font_weight,
                            text_decoration: TextDecoration::Underline,
                        ),
                        on_press: {
                            let navigator = navigator.clone();
                            move || {
                                if index != selected {
                                    navigator.push(Self::route_for(index));
                                }
                            }
                        },
                    )
                }
            })
            .collect()
    }
}
