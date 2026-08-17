#![allow(dead_code, clippy::main_recursion)]

pub mod animated;
mod animated_theme;
mod async_builder;
mod color_sync;
mod custom_animated_theme;
mod custom_font;
pub mod drag_and_drop;
pub mod file_drop_zone;
mod floating;
mod focus_node_example;
mod http_request_button;
mod justify_content_example;
mod loading_animation;
mod markdown_example;
mod modal;
mod overflow_behavior_example;
mod panic_recovery;
mod resizable_example;
pub mod routing;
mod scroll_and_row;
mod selectable_text;
mod starter;
pub mod stateful;
mod stateful_2;
mod svg_test;
mod system_theme;
mod test_animation;
pub mod text_area_example;
pub mod text_field_example;
mod window_example;

#[allow(unused_imports)]
use aimer::style::*;
#[allow(unused_imports)]
use aimer::*;
#[allow(unused_imports)]
use aimer::{AimerApp, *};

#[allow(unused_imports)]
use crate::animated::start_my_animated_list;
#[allow(unused_imports)]
use crate::custom_animated_theme::start_custom_animated_theme_example;
#[allow(unused_imports)]
use crate::floating::start_floating_example;
#[allow(unused_imports)]
use crate::loading_animation::start_loading_animation_example;
#[allow(unused_imports)]
use crate::markdown_example::start_markdown_example;
#[allow(unused_imports)]
use crate::markdown_example::start_custom_markdown_example;
#[allow(unused_imports)]
use crate::modal::start_modal_example;
#[allow(unused_imports)]
use crate::overflow_behavior_example::start_overflow_behavior_example;
#[allow(unused_imports)]
use crate::panic_recovery::start_panic_recovery_example;
#[allow(unused_imports)]
use crate::justify_content_example::start_justify_content_example;
#[allow(unused_imports)]
use crate::resizable_example::start_resizable_example;
#[allow(unused_imports)]
use crate::scroll_and_row::test_scroll_and_row;
#[allow(unused_imports)]
use crate::stateful::start_counter;
#[allow(unused_imports)]
use crate::svg_test::start_svg_test;
#[allow(unused_imports)]
use crate::system_theme::start_system_theme_example;
#[allow(unused_imports)]
use crate::test_animation::TestFadingAnimation;
#[allow(unused_imports)]
use crate::window_example::start_window_example;

// this is the entry point of the app
#[main]
fn main() {
    // test_text();
    // stateful_2::start_my_list();
    // start_counter();
    // test_positioned()
    // async_builder::start_async_builder_example()
    // test_scrollable()
    // test_scrollable_row()
    // start_modal_example();
    // start_floating_example();
    // routing::tab_shell_app();
    // routing::state_router()
    // drag_and_drop::start_drag_and_drop_example();
    // file_drop_zone::start_file_drop_zone_example();
    // start_markdown_example();
    // start_custom_markdown_example();
    // start_panic_recovery_example();
    start_window_example();
    // start_resizable_example();
    // test_scroll_and_row();
    // start_svg_test();
    // panic_recovery::start_panic_recovery_example()
    // start_custom_animated_theme_example()
    // start_system_theme_example()
    // system_theme::start_system_theme_example();
    // test_text()
    // start_loading_animation_example()
    // floating::start_floating_example();
    // file_drop_zone::start_file_drop_zone_example()
    // drag_and_drop::start_drag_and_drop_example();
    // selectable_text::start_selectable_text_example();
    // text_area_example::start_text_area_example();
    // text_field_example::start_text_field_example();
    // focus_node_example::start_focus_node_example()
    // http_request_button::start_http_request_button()
    // start_overflow_behavior_example();
    // start_justify_content_example();
    // resizable_example::start_resizable_example();
    // justify_content_example::start_justify_content_example()
    // test_image()
}


#[allow(unused)]
fn test_text() {
    AimerApp::start(
        Scrollable::new()
            .vertical_scroll_bar(None)
            .axis(ScrollAxis::Vertical)
            .child(Container::new()
                .padding(LayoutSpacing::all(12).top(50))
                .child(Text::new(
                    r#"
你好吗
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
"#
                )
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
                                .controller(TextEditingController::new())
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
//
// #[allow(unused)]
// pub fn test_scrollable() {
//     // `list` keeps the data and maps it to children on demand, so the widget
//     // tree retains 120_000 `u32`s instead of 120_000 boxed containers.
//     //
//     // Every row is the same height, so the column predicts its scroll extent
//     // from a single probed row instead of measuring 120_000 of them, and
//     // re-checks that prediction against the rows it paints, recording the exact
//     // extent of any row that disagrees. That is what lets rows be materialized one
//     // viewport at a time: cold start builds a couple of dozen elements rather than
//     // 120_000, and a scroll rebuilds only the rows crossing the window edge. A row
//     // that leaves the window keeps its element for a while, so scrolling a few
//     // screens away and back preserves per-row state — none is used here.
//     //
//     // `.item_extent(Dimension::Px(270.0))` — the 240px row plus its 30px margin,
//     // the 12px gap being added by the container — would state the same extent up
//     // front and skip even the probe.
//     let content = Column::new()
//         .horizontal_alignment(BoxAlignment::Start)
//         .gaps(LayoutSpacing::new().bottom(12))
//         .list(0..120000u32)
//         // .item_extent(Dimension::Px(270.0))
//         .builder(|i| {
//             let i = *i;
//             let color = if i % 2 == 0 {
//                 Color::Rgb(100, 149, 237)
//             } else {
//                 Color::Rgb(255, 160, 122)
//             };
//             Container::new()
//                 .margin(LayoutSpacing {
//                     top: Spacing::Px(30),
//                     ..Default::default()
//                 })
//                 .box_decoration(
//                     BoxDecoration::new()
//                         .border(BoxBorder::all(
//                             BorderSlice::new()
//                                 .style(BorderStyle::Solid)
//                                 .stroke(Stroke::Px(1.0))
//                                 .color(Colors::Black),
//                         ))
//                         .background_color(color),
//                 )
//                 .height(Dimension::Px(240.0))
//                 .box_child(
//                     Text::new(format!("Item {}", i))
//                         .text_align(TextAlign::MidCenter)
//                         .text_style(
//                             TextStyle::new()
//                                 .font_size(15)
//                                 .color(Colors::Black),
//                         ),
//                 )
//         });
//
//     let scrollbar = ScrollBar {
//         track: ScrollTrack {
//             width: Dimension::Px(2.0),
//             color: Colors::Transparent,
//             hover_color: Colors::Gray.alpha(120),
//         },
//         thumb: ScrollThumb {
//             width: Dimension::Px(2.0),
//             radius: Dimension::Px(4.0),
//             color: Colors::Transparent,
//             hover_color: Colors::Black,
//             active_color: Colors::Black,
//         },
//         up_button: None,
//         down_button: None,
//     };
//     let app = Container::new()
//         .color(Color::WHITE)
//         .child(
//             Scrollable::new()
//                 .axis(ScrollAxis::Vertical)
//                 .child(content),
//         );
//
//     AimerApp::start(app);
// }
// #[allow(unused)]
// fn test_scrollable_row() {
//     let items: Vec<AnyWidget> = (0..12000)
//         .map(|i| {
//             let color = if i % 2 == 0 {
//                 Color::Rgb(100, 149, 237)
//             } else {
//                 Color::Rgb(255, 160, 122)
//             };
//             if i == 5 {
//                 Container::new()
//                     .padding(LayoutSpacing::all(Spacing::Px(10)))
//                     .margin(LayoutSpacing {
//                         right: Spacing::Px(10),
//                         ..Default::default()
//                     })
//                     .width(Dimension::Px(200.0))
//                     .box_decoration(
//                         BoxDecoration::new()
//                             .border(BoxBorder::all(
//                                 BorderSlice::new()
//                                     .style(BorderStyle::Solid)
//                                     .stroke(Stroke::Px(1.0))
//                                     .color(Colors::Black),
//                             ))
//                             .background_color(Colors::Green),
//                     )
//                     .child(
//                         Text::new(format!("Item {}", i))
//                             .text_align(TextAlign::MidCenter)
//                             .text_style(
//                                 TextStyle::new()
//                                     .font_size(15)
//                                     .color(Colors::Black),
//                             ),
//                     )
//                     .boxed()
//             } else {
//                 Container::new()
//                     .margin(LayoutSpacing {
//                         right: Spacing::Px(10),
//                         ..Default::default()
//                     })
//                     .box_decoration(
//                         BoxDecoration::new()
//                             .border(BoxBorder::all(
//                                 BorderSlice::new()
//                                     .style(BorderStyle::Solid)
//                                     .stroke(Stroke::Px(1.0))
//                                     .color(Colors::Black),
//                             ))
//                             .background_color(color),
//                     )
//                     .width(Dimension::Px(80.0))
//                     .child(
//                         Text::new(format!("Item {}", i))
//                             .text_align(TextAlign::MidCenter)
//                             .text_style(
//                                 TextStyle::new()
//                                     .font_size(15)
//                                     .color(Colors::Black),
//                             ),
//                     )
//                     .boxed()
//             }
//         })
//         .collect();
//     let content = Row::new()
//         .vertical_alignment(BoxAlignment::Start)
//         .horizontal_alignment(BoxAlignment::Start)
//         .children(items);
//     let scrollbar = ScrollBar {
//         track: ScrollTrack {
//             width: Dimension::Px(2.0),
//             color: Colors::Transparent,
//             hover_color: Colors::Gray.alpha(120),
//         },
//         thumb: ScrollThumb {
//             width: Dimension::Px(2.0),
//             radius: Dimension::Px(4.0),
//             color: Colors::Transparent,
//             hover_color: Colors::Black,
//             active_color: Colors::Black,
//         },
//         up_button: None,
//         down_button: None,
//     };
//     let app = Container::new().child(
//         Scrollable::new()
//             .axis(ScrollAxis::Horizontal)
//             .vertical_scroll_bar(Some(scrollbar))
//             .child(content),
//     );
//     AimerApp::start(app);
// }

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

#[cfg(test)]
mod text_editing_example_tests {
    use aimer::Widget;

    use crate::text_area_example::TextAreaExample;
    use crate::text_field_example::TextFieldExample;

    #[test]
    fn text_field_example_is_constructible_as_a_widget() {
        let example = TextFieldExample::new();

        assert_eq!(Widget::debug_name(&example), "TextFieldExample");
    }

    #[test]
    fn text_area_example_is_constructible_as_a_widget() {
        let example = TextAreaExample::new();

        assert_eq!(Widget::debug_name(&example), "TextAreaExample");
    }
}
