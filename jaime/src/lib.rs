pub mod animated;
mod animated_theme;
mod async_builder;
mod color_sync;
mod custom_animated_theme;
mod custom_font;
mod markdown_example;
mod panic_recovery;
pub mod routing;
mod scroll_and_row;
mod selectable_text;
mod starter;
pub mod stateful;
mod stateful_2;
mod svg_test;
mod test_animation;

use aimer::style::*;
#[allow(unused_imports)]
use aimer::*;
use aimer::{AimerApp, *};

#[allow(unused_imports)]
use crate::animated::start_my_animated_list;
use crate::custom_animated_theme::start_custom_animated_theme_example;
use crate::markdown_example::start_markdown_example;
#[allow(unused_imports)]
use crate::panic_recovery::start_panic_recovery_example;
use crate::scroll_and_row::test_scroll_and_row;
#[allow(unused_imports)]
use crate::stateful::start_counter;
use crate::svg_test::start_svg_test;
use crate::test_animation::TestFadingAnimation;

// this is the entry point of the app
#[main]
pub fn my_app() {
    // stateful_2::start_my_list();
    // start_counter();
    // AimerApp::start(Container::new().child(Row::new().children([
    //     Expanded::new().child(TestFadingAnimation),
    //     Expanded::new().child(TestFadingAnimation),
    // ])))
    // test_positioned()
    // async_builder::start_async_builder_example()
    // custom_animated_theme::start_custom_animated_theme_example()
    // test_scrollable()
    // test_scrollable_row()
    // start_markdown_example();
    // start_panic_recovery_example();
    // test_scroll_and_row();
    // start_svg_test();
    // panic_recovery::start_panic_recovery_example()
    // start_custom_animated_theme_example()
    test_text()
}

#[allow(unused)]
fn test_text() {
    AimerApp::start(
        Scrollable::new()
            .axis(ScrollAxis::Vertical)
            .child(Container::new()
        .child(Text::new(
            r#"
                English — Hello / Hi               Khmer — សួស្តី (Suosdei)               French — BonjourEnglish — Hello / Hi
                Spanish — Hola                            Portuguese — Olá                          Italian — Ciao
                German — Hallo                            Dutch — Hallo                             Swedish — Hej
                Norwegian — Hei                           Danish — Hej                              Finnish — Hei
                Icelandic — Halló                         Russian — Привет (Privet)                 Ukrainian — Привіт (Pryvit)
                Polish — Cześć                            Czech — Ahoj                              Slovak — Ahoj
                Hungarian — Szia                          Romanian — Salut                          Greek — Γεια σου (Yia sou)
                Turkish — Merhaba                         Arabic — مرحبا (Marhaban)                 Hebrew — שלום (Shalom)
                Persian — سلام (Salam)                    Hindi — नमस्ते (Namaste)                  Bengali — হ্যালো / নমস্কার
                Punjabi — ਸਤ ਸ੍ਰੀ ਅਕਾਲ                    Urdu — السلام علیکم                       Tamil — வணக்கம்
                Telugu — నమస్తే                           Kannada — ನಮಸ್ಕಾರ                         Malayalam — നമസ്കാരം
                Thai — สวัสดี                             Lao — ສະບາຍດີ                             Vietnamese — Xin chào
                Indonesian — Halo                         Malay — Hai / Halo                        Filipino — Kumusta
                Chinese (Mandarin) — 你好 (Nǐ hǎo)          Cantonese — 你好 (Néih hóu)                 Japanese — こんにちは (Konnichiwa)
                Korean — 안녕하세요 (Annyeonghaseyo)           Mongolian — Сайн байна уу                 Swahili — Jambo
                Zulu — Sawubona                           Afrikaans — Hallo                         Esperanto — Saluton
                Latin — Salve                             Hawaiian — Aloha                          Māori — Kia ora
                អរគុណ 你哈皮  With State 你好 きみなと  👉
"#)
            .text_style(TextStyle::new()
                .text_overflow(TextOverflow::Clip)
                .font_size(16)
                .color(Colors::White)
                .font_weight(FontWeight::Thin))
        )
    )
    )
}

#[allow(unused)]
fn test_positioned() {
    AimerApp::start(
        Container::new()
            .color(Color::WHITE)
            .child(
                Stack::new().children([
                    Positioned::new()
                        .top(80.0)
                        .left(80.0)
                        .child(
                            Container::new()
                                .box_decoration(
                                    BoxDecoration::new()
                                        .border(BoxBorder::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .stroke(Stroke::Px(30.0))
                                                .color(Colors::Black),
                                        ))
                                        .outline(BoxOutline::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .stroke(Stroke::Px(3.0))
                                                .color(Colors::Black),
                                        ))
                                        .border_radius((55, 6, 25, 6))
                                        .background_color(Colors::Red)
                                        .box_shadow(vec![
                                            BoxShadow::new()
                                                .color(Colors::Black.alpha(120))
                                                .blur(10.0)
                                                .inset(true),
                                        ]),
                                )
                                .width(Dimension::Px(400.0))
                                .height(Dimension::Px(400.0))
                                .child(
                                    Text::new("Hello, World!")
                                        .text_style(TextStyle::new().color(Colors::Black)),
                                ),
                        )
                        .boxed(),
                    Positioned::new()
                        .top(280.0)
                        .left(180.0)
                        .child(
                            Container::new()
                                .box_decoration(
                                    BoxDecoration::new()
                                        .border(BoxBorder::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .stroke(Stroke::Px(30.0))
                                                .color(Colors::Black),
                                        ))
                                        .outline(BoxOutline::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .stroke(Stroke::Px(3.0))
                                                .color(Colors::Black),
                                        ))
                                        .border_radius((55, 6, 25, 6))
                                        .background_color(Colors::Red)
                                        .box_shadow(vec![
                                            BoxShadow::new()
                                                .color(Colors::Black.alpha(120))
                                                .blur(10.0)
                                                .inset(true),
                                        ]),
                                )
                                .width(Dimension::Px(400.0))
                                .height(Dimension::Px(400.0))
                                .child(
                                    Text::new("Hello, World!")
                                        .text_style(TextStyle::new().color(Colors::Black)),
                                ),
                        )
                        .boxed(),
                ]),
            ),
    )
}

#[allow(unused)]
fn test_border_outline() {
    AimerApp::start(
        Container::new()
            .padding(LayoutSpacing::all(Spacing::Px(50)))
            .child(
                Container::new().child(
                    Container::new()
                        .padding(LayoutSpacing::all(Spacing::Px(10)))
                        .child(
                            TextField::new()
                                .padding(LayoutSpacing::all(Spacing::Px(10)))
                                .controller(TextFieldController::new())
                                .text_align(TextAlign::MidLeft)
                                .input_type(InputType::Text)
                                .prompt("Input any here....")
                                .decoration(
                                    BoxDecoration::new()
                                        .background_color(Colors::Gray.alpha(140))
                                        .border(BoxBorder::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .color(Colors::Black)
                                                .stroke(2),
                                        ))
                                        .outline(BoxOutline::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .color(Colors::Black)
                                                .stroke(2),
                                        )),
                                )
                                .hover_decoration(
                                    BoxDecoration::new()
                                        .background_color(Colors::Gray.alpha(70))
                                        .border(BoxBorder::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .color(Colors::Black)
                                                .stroke(2),
                                        ))
                                        .outline(BoxOutline::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .color(Colors::Green)
                                                .stroke(2),
                                        )),
                                )
                                .focus_decoration(
                                    BoxDecoration::new()
                                        .background_color(Colors::Gray.alpha(100))
                                        .border(BoxBorder::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .color(Colors::Green)
                                                .stroke(2),
                                        ))
                                        .outline(BoxOutline::all(
                                            BorderSlice::new()
                                                .style(BorderStyle::Solid)
                                                .color(Colors::Black)
                                                .stroke(2),
                                        )),
                                ),
                        ),
                ),
            ),
    )
}

#[allow(unused)]
pub fn test_scrollable() {
    let items: Vec<AnyWidget> = (0..1200)
        .map(|i| {
            let color = if i % 2 == 0 {
                Color::Rgb(100, 149, 237)
            } else {
                Color::Rgb(255, 160, 122)
            };
            if i == 5 {
                Container::new()
                    .padding(LayoutSpacing::all(Spacing::Px(10)))
                    .height(Dimension::Px(200.0))
                    .child(
                        Text::new(format!("Item {}", i))
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(Colors::Black),
                            ),
                    )
                    .boxed()
            } else {
                Container::new()
                    .margin(LayoutSpacing {
                        top: Spacing::Px(30),
                        ..Default::default()
                    })
                    .box_decoration(
                        BoxDecoration::new()
                            .border(BoxBorder::all(
                                BorderSlice::new()
                                    .style(BorderStyle::Solid)
                                    .stroke(Stroke::Px(1.0))
                                    .color(Colors::Black),
                            ))
                            .background_color(color),
                    )
                    .height(Dimension::Px(80.0))
                    .child(
                        Text::new(format!("Item {}", i))
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(Colors::Black),
                            ),
                    )
                    .boxed()
            }
        })
        .collect();

    let items_2: Vec<AnyWidget> = (0..1200)
        .map(|i| {
            let color = if i % 2 == 0 {
                Color::Rgb(100, 149, 237)
            } else {
                Color::Rgb(255, 160, 122)
            };
            if i == 5 {
                Container::new()
                    .padding(LayoutSpacing::all(Spacing::Px(10)))
                    .height(Dimension::Px(200.0))
                    .box_decoration(BoxDecoration::new().background_color(Colors::Green))
                    .child(
                        Text::new(format!("Item {}", i))
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(Colors::Black),
                            ),
                    )
                    .boxed()
            } else {
                Container::new()
                    .margin(LayoutSpacing {
                        top: Spacing::Px(30),
                        ..Default::default()
                    })
                    .box_decoration(
                        BoxDecoration::new()
                            .border(BoxBorder::all(
                                BorderSlice::new()
                                    .style(BorderStyle::Solid)
                                    .stroke(Stroke::Px(1.0))
                                    .color(Colors::Black),
                            ))
                            .background_color(color),
                    )
                    .height(Dimension::Px(80.0))
                    .child(
                        Text::new(format!("Item {}", i))
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(Colors::Black),
                            ),
                    )
                    .boxed()
            }
        })
        .collect();
    let content = Column::new()
        .horizontal_alignment(BoxAlignment::Center)
        .children(items);

    let content_2 = Column::new()
        .horizontal_alignment(BoxAlignment::Center)
        .children(items_2);
    let scrollbar = ScrollBar {
        track: ScrollTrack {
            width: Dimension::Px(2.0),
            color: Colors::Transparent,
            hover_color: Colors::Gray.alpha(120),
        },
        thumb: ScrollThumb {
            width: Dimension::Px(2.0),
            radius: Dimension::Px(4.0),
            color: Colors::Transparent,
            hover_color: Colors::Black,
            active_color: Colors::Black,
        },
        up_button: None,
        down_button: None,
    };
    let app = Container::new()
        .color(Color::WHITE)
        .child(
            Column::new()
                .children(vec![
                    Container::new()
                        .height(Dimension::Px(80.0))
                        .box_decoration(BoxDecoration::new().background_color(Colors::Green))
                        .child(
                            Text::new("This is header")
                                .text_align(TextAlign::MidCenter)
                                .text_style(
                                    TextStyle::new()
                                        .font_size(15)
                                        .color(Colors::Black),
                                ),
                        )
                        .boxed(),
                    Row::new()
                        .children(vec![
                            Container::new()
                                .padding(LayoutSpacing::horizontal(Spacing::Px(10)))
                                .child(
                                    Scrollable::new()
                                        .axis(ScrollAxis::Vertical)
                                        .vertical_scroll_bar(Some(scrollbar))
                                        .child(content),
                                )
                                .boxed(),
                            Container::new()
                                .padding(LayoutSpacing::horizontal(Spacing::Px(10)))
                                .child(
                                    Scrollable::new()
                                        .axis(ScrollAxis::Vertical)
                                        .vertical_scroll_bar(Some(scrollbar))
                                        .child(content_2),
                                )
                                .boxed(),
                        ])
                        .boxed(),
                    Container::new()
                        .height(Dimension::Px(80.0))
                        .box_decoration(BoxDecoration::new().background_color(Colors::Green))
                        .child(
                            Text::new("This is footer")
                                .text_align(TextAlign::MidCenter)
                                .text_style(
                                    TextStyle::new()
                                        .font_size(15)
                                        .color(Colors::Black),
                                ),
                        )
                        .boxed(),
                ])
                .boxed(),
        );

    AimerApp::start(app);
}
#[allow(unused)]
fn test_scrollable_row() {
    let items: Vec<AnyWidget> = (0..12000)
        .map(|i| {
            let color = if i % 2 == 0 {
                Color::Rgb(100, 149, 237)
            } else {
                Color::Rgb(255, 160, 122)
            };
            if i == 5 {
                Container::new()
                    .padding(LayoutSpacing::all(Spacing::Px(10)))
                    .margin(LayoutSpacing {
                        right: Spacing::Px(10),
                        ..Default::default()
                    })
                    .width(Dimension::Px(200.0))
                    .box_decoration(
                        BoxDecoration::new()
                            .border(BoxBorder::all(
                                BorderSlice::new()
                                    .style(BorderStyle::Solid)
                                    .stroke(Stroke::Px(1.0))
                                    .color(Colors::Black),
                            ))
                            .background_color(Colors::Green),
                    )
                    .child(
                        Text::new(format!("Item {}", i))
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(Colors::Black),
                            ),
                    )
                    .boxed()
            } else {
                Container::new()
                    .margin(LayoutSpacing {
                        right: Spacing::Px(10),
                        ..Default::default()
                    })
                    .box_decoration(
                        BoxDecoration::new()
                            .border(BoxBorder::all(
                                BorderSlice::new()
                                    .style(BorderStyle::Solid)
                                    .stroke(Stroke::Px(1.0))
                                    .color(Colors::Black),
                            ))
                            .background_color(color),
                    )
                    .width(Dimension::Px(80.0))
                    .child(
                        Text::new(format!("Item {}", i))
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(Colors::Black),
                            ),
                    )
                    .boxed()
            }
        })
        .collect();
    let content = Row::new()
        .vertical_alignment(BoxAlignment::Center)
        .children(items);
    let scrollbar = ScrollBar {
        track: ScrollTrack {
            width: Dimension::Px(2.0),
            color: Colors::Transparent,
            hover_color: Colors::Gray.alpha(120),
        },
        thumb: ScrollThumb {
            width: Dimension::Px(2.0),
            radius: Dimension::Px(4.0),
            color: Colors::Transparent,
            hover_color: Colors::Black,
            active_color: Colors::Black,
        },
        up_button: None,
        down_button: None,
    };
    let app = Container::new().child(
        Row::new()
            .children(vec![
                Container::new()
                    .width(Dimension::Px(80.0))
                    .box_decoration(BoxDecoration::new().background_color(Colors::Green))
                    .child(
                        Text::new("This is header")
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(Colors::Black),
                            ),
                    )
                    .boxed(),
                Container::new()
                    .padding(LayoutSpacing::all(Spacing::Px(10)))
                    .child(
                        Scrollable::new()
                            .axis(ScrollAxis::Horizontal)
                            .vertical_scroll_bar(Some(scrollbar))
                            .child(content),
                    )
                    .boxed(),
                Container::new()
                    .width(Dimension::Px(80.0))
                    .box_decoration(BoxDecoration::new().background_color(Colors::Green))
                    .child(
                        Text::new("This is footer")
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(Colors::Black),
                            ),
                    )
                    .boxed(),
            ])
            .boxed(),
    );
    AimerApp::start(app);
}

#[allow(unused)]
fn test_image() {
    AimerApp::start(
        Container::new()
            .padding(LayoutSpacing::all(Spacing::Percent(15)))
            .box_decoration(BoxDecoration::new().background_color(Colors::Black))
            .child(
                Container::new()
                    .box_decoration(
                        BoxDecoration::new()
                            .background_color(Color::Rgb(41, 31, 31))
                            .border_radius((55, 0, 55, 0))
                            .box_shadow(vec![
                                BoxShadow::new()
                                    .color(Colors::Gray.alpha(200))
                                    .blur(12.0)
                                    .spread(10.0)
                                    .offset_x(40.0)
                                    .offset_y(40.0),
                            ]),
                    )
                    .padding(LayoutSpacing::all(Spacing::Px(10)))
                    .child(
                        AssetImage::new("assets/my_image.png")
                            .fit(BoxFit::FitWidth)
                            .scale(1.1_f32),
                    ),
            ),
    )
}
