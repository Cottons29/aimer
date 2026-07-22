use aimer::Dimension::Px;
use aimer::router::NavigatorController;
use aimer::style::{
    BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontWeight, LayoutSpacing, TextDecoration,
    TextStyle, Theme, ThemeData,
};
use aimer::{BuildContext, Container, StatelessWidget, Svg, SvgDocument, Text, Widget, widget, *};

use crate::components::app_shell::{AppShellState, WebsiteThemeMode};
use crate::router::AppRouter;

#[widget(Stateless)]
#[derive(Clone)]
pub struct HeaderSection {
    pub active_tab: usize,
    pub(crate) theme_mode: WebsiteThemeMode,
    pub(crate) theme_updater: StateUpdater<AppShellState>,
}

impl StatelessWidget for HeaderSection {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);
        let icon = SvgDocument::from_svg(self.theme_mode.toggle_icon())
            .expect("the bundled theme icon should be valid");

        let children = vec![
            SizedBox::new()
                .width(16)
                .boxed(),
            Text::new("Aimer")
                .text_style(
                    TextStyle::new()
                        .text_decoration(TextDecoration::Underline)
                        .font_weight(FontWeight::Bolder)
                        .font_size(24)
                        .color(theme.on_surface_color),
                )
                .boxed(),
            SizedBox::new()
                .width(16)
                .boxed(),
            Expanded::new()
                .child(
                    Container::new().child(
                        Row::new()
                            .horizontal_alignment(BoxAlignment::End)
                            .vertical_alignment(BoxAlignment::Center)
                            .gaps(LayoutSpacing::new().left(24))
                            .children(self.build_platform_button_list(ctx, &theme)),
                    ),
                )
                .boxed(),
            SizedBox::new()
                .width(24)
                .boxed(),
            Container::new()
                .width(36)
                .height(36)
                .box_child(
                    Button::new()
                        .on_press({
                            let updater = self.theme_updater.clone();
                            move || updater.set_state(AppShellState::toggle_theme)
                        })
                        .decoration(
                            BoxDecoration::new()
                                .border(BoxBorder::all(
                                    BorderSlice::new()
                                        .color(
                                            theme
                                                .on_background_color
                                                .with_alpha(0.1),
                                        )
                                        .style(BorderStyle::Solid)
                                        .stroke(2),
                                ))
                                .border_radius(8),
                        )
                        .child(
                            Row::new()
                                .horizontal_alignment(BoxAlignment::Center)
                                .vertical_alignment(BoxAlignment::Center)
                                .children([Svg::new(icon)
                                    .width(24)
                                    .height(24)
                                    .style(
                                        "#fill_theme",
                                        SvgStyle::new().fill(theme.on_background_color),
                                    )]),
                        ),
                ),
            SizedBox::new()
                .width(16)
                .boxed(),
        ];

        Container::new()
            .color(theme.surface_color)
            .height(Px(60.0))
            .box_decoration(
                BoxDecoration::new().border(
                    BoxBorder::new().bottom(
                        BorderSlice::new()
                            .stroke(Px(1.0))
                            .color(Color::GRAY.with_alpha(0.3))
                            .style(BorderStyle::Solid),
                    ),
                ),
            )
            .child(
                Row::new()
                    .vertical_alignment(BoxAlignment::Center)
                    .children(children),
            )
    }
}

impl HeaderSection {
    const SECTIONS: &[&str] = &["Home", "Blog"];

    /// Resolve a section index to the route it navigates to.
    fn route_for(index: usize) -> AppRouter {
        match index {
            1 => AppRouter::Blog,
            2 => AppRouter::Learn,
            _ => AppRouter::Home,
        }
    }

    fn build_platform_button_list(&self, ctx: &BuildContext, theme: &ThemeData) -> Vec<AnyWidget> {
        let selected = self.active_tab;
        // Reach the enclosing Navigator so header buttons can drive navigation.
        let navigator = NavigatorController::<AppRouter>::of(ctx);
        Self::SECTIONS
            .iter()
            .enumerate()
            .map({
                move |(i, l)| {
                    let index = i;
                    let is_selected = index == selected;
                    let font_weight = if selected == index {
                        FontWeight::Bolder
                    } else {
                        FontWeight::Normal
                    };

                    TextButton::new(*l)
                        .style(
                            TextStyle::new()
                                .font_size(20)
                                .color(if is_selected {
                                    theme.primary_color
                                } else {
                                    theme.on_surface_color
                                })
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
                                    theme.primary_color
                                } else {
                                    theme
                                        .primary_color
                                        .lighten(0.2)
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
