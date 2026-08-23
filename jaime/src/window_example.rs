use aimer::macros::widget;
use aimer::native::macos_windowing::MacosWindowing;
use aimer::style::*;
use aimer::*;

const SIDEBAR_WIDTH: f32 = 252.0;
const TITLEBAR_INSET: u32 = 68;

const STATIONS: [(&str, &str, Color); 6] = [
    ("01", "Aimer One", Color::Rgb(238, 68, 72)),
    ("HITS", "Daily Hits", Color::Rgb(52, 110, 235)),
    ("FOLK", "Open Country", Color::Rgb(231, 159, 31)),
    ("UNO", "Música Uno", Color::Rgb(207, 56, 126)),
    ("CLUB", "Aimer Club", Color::Rgb(39, 39, 42)),
    ("CHILL", "Chill", Color::Rgb(65, 145, 238)),
];

/// Starts a native-window showcase inspired by the supplied macOS media window.
///
/// The application keeps the standard traffic-light controls while extending
/// Aimer content into a transparent titlebar. `MacosWindowing::install` is
/// available on every target, so this example does not need platform-specific
/// conditional compilation; it simply becomes a no-op outside macOS.
pub fn start_window_example() {
    AimerApp::new()
        .window(
            WindowAttr::new()
                .title("Aimer Radio")
                .inner_size(1240, 780)
                .min_inner_size(900, 620),
        )
        .setup(|| {
            MacosWindowing::new()
                .titlebar_transparent(true)
                .title_hidden(true)
                .fullsize_content_view(true)
                .movable_by_window_background(true)
                .has_shadow(true)
                .accepts_first_mouse(true)
                .traffic_light_position(16.0, 14.0)
                .install();
        })
        .child(WindowShowcase::new())
        .run();
}

/// A dark media-library page that demonstrates full-size macOS window content.
#[derive(Clone)]
#[widget(Stateless)]
pub struct WindowShowcase {}

impl WindowShowcase {
    /// Creates the window showcase root widget.
    #[inline]
    pub fn new() -> Self {
        Self {}
    }
}

impl StatelessWidget for WindowShowcase {
    fn build(&self, _: &BuildContext) -> impl Widget {
        Container::new()
            .color(Color::Rgb(38, 37, 36))
            .child(Row::new().children([
                sidebar(),
                Expanded::new().child(library()).boxed(),
            ]))
    }
}

fn sidebar() -> AnyWidget {
    Container::new()
        .width(Dimension::Px(SIDEBAR_WIDTH))
        .height(Dimension::Percent(100.0))
        .padding(
            LayoutSpacing::new()
                .top(TITLEBAR_INSET)
                .right(16)
                .bottom(18)
                .left(16),
        )
        .color(Color::Rgb(44, 43, 42))
        .child(Column::new().children(vec![
            navigation_item("M", "Search", false),
            navigation_item("⌂", "Home", false),
            navigation_item("▦", "New", false),
            navigation_item("◉", "Radio", true),
            section_label("Library"),
            navigation_item("◷", "Recently Added", false),
            navigation_item("♬", "Artists", false),
            navigation_item("▣", "Albums", false),
            navigation_item("♪", "Songs", false),
            section_label("Playlists"),
            navigation_item("▦", "All Playlists", false),
            navigation_item("☆", "Favourite Songs", false),
            Expanded::new().child(ZeroSizedBox).boxed(),
            profile(),
        ]))
        .boxed()
}

fn navigation_item(icon: &'static str, label: &'static str, selected: bool) -> AnyWidget {
    let accent = Color::Rgb(255, 55, 95);
    let background = if selected {
        Color::Rgb(80, 78, 76)
    } else {
        Color::Rgba(0, 0, 0, 0)
    };

    Container::new()
        .height(Dimension::Px(42.0))
        .padding(LayoutSpacing::new().right(12).left(12))
        .box_decoration(
            BoxDecoration::new()
                .background_color(background)
                .border_radius(10),
        )
        .child(
            Row::new()
                .vertical_alignment(BoxAlignment::Center)
                .gaps(LayoutSpacing::all(Spacing::Px(13)))
                .children([
                    Container::new()
                        .width(Dimension::Px(25.0))
                        .child(
                            Text::new(icon)
                                .text_align(TextAlign::MidCenter)
                                .text_style(TextStyle::new().font_size(22).color(accent)),
                        )
                        .boxed(),
                    Text::new(label)
                        .text_style(
                            TextStyle::new()
                                .font_size(17)
                                .font_weight(if selected {
                                    FontWeight::Bold
                                } else {
                                    FontWeight::Value(500)
                                })
                                .color(Color::WHITE),
                        )
                        .boxed(),
                ]),
        )
        .boxed()
}

fn section_label(label: &'static str) -> AnyWidget {
    Container::new()
        .height(Dimension::Px(34.0))
        .padding(LayoutSpacing::new().top(12).left(8))
        .child(
            Text::new(label).text_style(
                TextStyle::new()
                    .font_size(13)
                    .font_weight(FontWeight::Bold)
                    .color(Color::Rgb(135, 132, 130)),
            ),
        )
        .boxed()
}

fn profile() -> AnyWidget {
    Container::new()
        .height(Dimension::Px(48.0))
        .child(
            Row::new()
                .vertical_alignment(BoxAlignment::Center)
                .gaps(LayoutSpacing::all(Spacing::Px(12)))
                .children([
                    Container::new()
                        .width(Dimension::Px(38.0))
                        .height(Dimension::Px(38.0))
                        .box_decoration(
                            BoxDecoration::new()
                                .background_color(Color::Rgb(122, 142, 192))
                                .border_radius(19),
                        )
                        .child(
                            Text::new("AI")
                                .text_align(TextAlign::MidCenter)
                                .text_style(TextStyle::new().font_size(15).color(Color::WHITE)),
                        )
                        .boxed(),
                    Text::new("Aimer Listener")
                        .text_style(
                            TextStyle::new()
                                .font_size(16)
                                .font_weight(FontWeight::Bold)
                                .color(Color::WHITE),
                        )
                        .boxed(),
                ]),
        )
        .boxed()
}

fn library() -> AnyWidget {
    Container::new()
        .height(Dimension::Percent(100.0))
        .padding(
            LayoutSpacing::new()
                .top(TITLEBAR_INSET)
                .right(28)
                .bottom(18)
                .left(32),
        )
        .color(Color::Rgb(38, 37, 36))
        .child(Column::new().children([
            Expanded::new()
                .child(Scrollable::new().child(library_content()))
                .boxed(),
            SizedBox::new().height(14).boxed(),
            player(),
        ]))
        .boxed()
}

fn library_content() -> AnyWidget {
    Column::new()
        .horizontal_alignment(BoxAlignment::Start)
        .children([
            Container::new()
                .height(Dimension::Px(66.0))
                .child(
                    Text::new("Radio")
                        .text_align(TextAlign::TopLeft)
                        .text_style(
                            TextStyle::new()
                                .font_size(38)
                                .font_weight(FontWeight::Bold)
                                .color(Color::Rgb(235, 234, 233)),
                        ),
                )
                .boxed(),
            Container::new()
                .height(Dimension::Px(166.0))
                .child(
                    Scrollable::new().axis(ScrollAxis::Horizontal).child(
                        Row::new()
                            .gaps(LayoutSpacing::all(Spacing::Px(18)))
                            .children(STATIONS.map(|station| station_card(station).boxed())),
                    ),
                )
                .boxed(),
            section_title("Latest Radio Episodes  ›"),
            episode_row(
                ("TIME CRISIS", "The Dog Days Are Over", Color::Rgb(82, 96, 82)),
                ("THE CREATIVE HOUR", "Side One, Track One", Color::Rgb(205, 211, 92)),
            ),
            SizedBox::new().height(14).boxed(),
            episode_row(
                ("FIVE ON FRIDAYS", "Shout-Out to the Liars", Color::Rgb(226, 92, 42)),
                ("SOULECTION", "Episode 742", Color::Rgb(131, 75, 39)),
            ),
            SizedBox::new().height(20).boxed(),
        ])
        .boxed()
}

fn station_card((mark, name, accent): (&'static str, &'static str, Color)) -> impl Widget {
    Container::new()
        .width(Dimension::Px(132.0))
        .height(Dimension::Px(150.0))
        .child(Column::new().children([
            Container::new()
                .width(Dimension::Px(132.0))
                .height(Dimension::Px(124.0))
                .box_decoration(
                    BoxDecoration::new()
                        .background_color(Color::Rgb(244, 244, 245))
                        .border_radius(22),
                )
                .child(
                    Text::new(mark)
                        .text_align(TextAlign::MidCenter)
                        .text_style(
                            TextStyle::new()
                                .font_size(if mark.len() > 2 { 27 } else { 52 })
                                .font_weight(FontWeight::Bold)
                                .color(accent),
                        ),
                )
                .boxed(),
            Container::new()
                .height(Dimension::Px(26.0))
                .child(
                    Text::new(name)
                        .text_align(TextAlign::MidCenter)
                        .text_style(
                            TextStyle::new()
                                .font_size(12)
                                .font_weight(FontWeight::Value(500))
                                .color(Color::Rgb(183, 181, 179)),
                        ),
                )
                .boxed(),
        ]))
}

fn section_title(title: &'static str) -> AnyWidget {
    Container::new()
        .height(Dimension::Px(50.0))
        .child(
            Text::new(title)
                .text_align(TextAlign::MidLeft)
                .text_style(
                    TextStyle::new()
                        .font_size(20)
                        .font_weight(FontWeight::Bold)
                        .color(Color::Rgb(220, 218, 216)),
                ),
        )
        .boxed()
}

fn episode_row(
    left: (&'static str, &'static str, Color),
    right: (&'static str, &'static str, Color),
) -> AnyWidget {
    Row::new()
        .gaps(LayoutSpacing::all(Spacing::Px(24)))
        .children([
            Expanded::new().child(episode(left)).boxed(),
            Expanded::new().child(episode(right)).boxed(),
        ])
        .boxed()
}

fn episode((show, title, accent): (&'static str, &'static str, Color)) -> AnyWidget {
    Container::new()
        .height(Dimension::Px(116.0))
        .child(
            Row::new()
                .vertical_alignment(BoxAlignment::Center)
                .gaps(LayoutSpacing::all(Spacing::Px(14)))
                .children([
                    Container::new()
                        .width(Dimension::Px(106.0))
                        .height(Dimension::Px(106.0))
                        .box_decoration(
                            BoxDecoration::new()
                                .background_color(accent)
                                .border_radius(8),
                        )
                        .child(
                            Text::new("ON AIR")
                                .text_align(TextAlign::MidCenter)
                                .text_style(
                                    TextStyle::new()
                                        .font_size(15)
                                        .font_weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        )
                        .boxed(),
                    Expanded::new()
                        .child(
                            Column::new()
                                .vertical_alignment(BoxAlignment::Center)
                                .children([
                                    Text::new(show)
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(12)
                                                .font_weight(FontWeight::Bold)
                                                .color(Color::Rgb(151, 148, 145)),
                                        )
                                        .boxed(),
                                    SizedBox::new().height(5).boxed(),
                                    Text::new(title)
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(16)
                                                .font_weight(FontWeight::Value(500))
                                                .color(Color::Rgb(232, 230, 228)),
                                        )
                                        .boxed(),
                                    SizedBox::new().height(5).boxed(),
                                    Text::new("A new mix for your library.")
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(13)
                                                .color(Color::Rgb(151, 148, 145)),
                                        )
                                        .boxed(),
                                ]),
                        )
                        .boxed(),
                    Text::new("•••")
                        .text_style(
                            TextStyle::new()
                                .font_size(15)
                                .color(Color::Rgb(215, 212, 209)),
                        )
                        .boxed(),
                ]),
        )
        .boxed()
}

fn player() -> AnyWidget {
    Container::new()
        .height(Dimension::Px(78.0))
        .padding(LayoutSpacing::new().right(22).left(22))
        .box_decoration(
            BoxDecoration::new()
                .background_color(Color::Rgb(77, 75, 72).with_alpha(0.96))
                .border_radius(30)
                .border(BoxBorder::all(
                    BorderSlice::new()
                        .style(BorderStyle::Solid)
                        .stroke(Stroke::Px(1.0))
                        .color(Color::Rgb(112, 109, 105)),
                )),
        )
        .child(
            Row::new()
                .vertical_alignment(BoxAlignment::Center)
                .gaps(LayoutSpacing::all(Spacing::Px(18)))
                .children([
                    Text::new("↶  ◀  ▶")
                        .text_style(
                            TextStyle::new()
                                .font_size(19)
                                .font_weight(FontWeight::Bold)
                                .color(Color::Rgb(194, 191, 188)),
                        )
                        .boxed(),
                    Expanded::new()
                        .child(
                            Column::new()
                                .vertical_alignment(BoxAlignment::Center)
                                .children([
                                    Text::new("HOW WE GOT HERE")
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(13)
                                                .font_weight(FontWeight::Bold)
                                                .color(Color::Rgb(177, 174, 171)),
                                        )
                                        .boxed(),
                                    Text::new("Rocket Hour  ·  Aimer Radio")
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(14)
                                                .color(Color::Rgb(228, 225, 222)),
                                        )
                                        .boxed(),
                                ]),
                        )
                        .boxed(),
                    Text::new("☷   ◉   🔊")
                        .text_style(TextStyle::new().font_size(18).color(Color::WHITE))
                        .boxed(),
                ]),
        )
        .boxed()
}

#[cfg(test)]
mod tests {
    use aimer::Widget;

    use super::WindowShowcase;

    #[test]
    fn window_showcase_is_constructible_as_a_widget() {
        fn assert_widget(_: impl Widget) {}

        assert_widget(WindowShowcase::new());
    }
}