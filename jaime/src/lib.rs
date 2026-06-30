pub mod animated;
mod color_sync;
pub mod routing;
mod starter;
pub mod stateful;
mod stateful_2;

#[allow(unused_imports)]
use crate::animated::start_my_animated_list;
use crate::color_sync::start_color_sync;
use crate::routing::state_router;
#[allow(unused_imports)]
use crate::stateful::start_counter;
use aimer::AimerApp;
use aimer::style::*;
use aimer::*;
#[allow(unused_imports)]
use aimer::*;

// this is the entry point of the app
#[main]
pub fn my_app() {
    // #[cfg(not(target_arch = "wasm32"))]
    start_counter();
    // state_router()
    // simply start the app with AimerApp::start
    // #[cfg(target_arch = "wasm32")]
    // test_positioned();
    // test_text()
    // test_scrollable()
    // test_scrollable_row()
    // stateful_2::start_my_list();
    // start_my_animated_list()
    // test_border_outline()
    // test_image()
    // start_color_sync()
}
#[allow(unused)]
fn test_text() {
    AimerApp::start(Scrollable!(
        axis: ScrollAxis::Vertical,
        child: Container!(
            // height: 1000.0,
            child: Text!(
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
"#,
                text_style: TextStyle!(
                    text_overflow: TextOverflow::Clip,
                    font_size: 16,
                    color: Colors::Black,
                    font_weight: FontWeight::Thin,
                )
            )
        )
    ))
}

#[allow(unused)]
fn test_positioned() {
    AimerApp::start(Container!(
        child: Stack!(
            children: [
                Positioned!(
                    // position: Position::Fixed,
                    top: 80.0,
                    left: 80.0,
                    child: Container!(
                        box_decoration: BoxDecoration!(
                            border: BoxBorder::all(
                                BorderSlice!(
                                    style: BorderStyle::Solid,
                                    stroke: Stroke::Px(30.0),
                                    color: Colors::Black,
                                )
                            ),
                            outline: BoxOutline::all(
                                BorderSlice!(
                                    style: BorderStyle::Solid,
                                    stroke: Stroke::Px(3.0),
                                    color: Colors::Black,
                                )
                            ),
                            border_radius: (55, 6, 25, 6),
                            background_color: Colors::Red,
                            box_shadow: BoxShadow!(
                                color: Colors::Black.alpha(120),
                                blur: 10.0,
                                inset: true,
                            )
                        ),
                        width: 400.0,
                        height: 400.0,
                        child: Text!(
                            "Hello, World!",
                            text_style: TextStyle!(
                                color: Colors::Black,
                            )
                        )
                    ),
                )
            ]
        )
    ))
}

#[allow(unused)]
fn test_border_outline() {
    AimerApp::start(Container!(
        padding: LayoutSpacing::all(Spacing::Px(50)),
        child: Container!(
            child: Container!(
                padding: LayoutSpacing::all(Spacing::Px(10)),
                child: TextField!(
                    padding: LayoutSpacing::all(Spacing::Px(10)),
                    controller: TextFieldController::new(),
                    text_align: TextAlign::MidLeft,
                    input_type: InputType::Text,
                    prompt: "Input any here....",
                    decoration: BoxDecoration!(
                        background_color: Colors::Gray.alpha(140),
                        border: BoxBorder::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                color: Colors::Black,
                                stroke: 2,

                            )
                        ),
                        outline: BoxOutline::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                color: Colors::Black,
                                stroke: 2,

                            )
                        ),
                    ),
                    hover_decoration: BoxDecoration!(

                        background_color: Colors::Gray.alpha(70),
                        border: BoxBorder::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                color: Colors::Black,
                                stroke: 2,

                            )
                        ),
                        outline: BoxOutline::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                color: Colors::Green,
                                stroke: 2,

                            )
                        ),
                    ),
                    focus_decoration: BoxDecoration!(
                        background_color: Colors::Gray.alpha(100),
                        border: BoxBorder::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                color: Colors::Green,
                                stroke: 2,

                            )
                        ),
                        outline: BoxOutline::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                color: Colors::Black,
                                stroke: 2,

                            )
                        ),
                    )
                )
            ),
        ),
    ))
}

#[allow(unused)]
pub fn test_scrollable() {
    let items: Vec<Box<dyn Widget>> = (0..1200)
        .map(|i| {
            let color = if i % 2 == 0 {
                Color::Rgb(100, 149, 237) // cornflower blue
            } else {
                Color::Rgb(255, 160, 122) // light salmon
            };
            if i == 5 {
                Box::new(Container!(
                    padding: LayoutSpacing::all( Spacing::Px(10)),
                    height: Dimension::Px(200.0),
                    child: Text!(
                        format!("Item {}", i),
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                )) as Box<dyn Widget>
            } else {
                Box::new(Container!(
                    // width: Dimension::Px(100.0),
                    margin: LayoutSpacing! (top: Spacing::Px(30)),
                    box_decoration: BoxDecoration!(
                        border: BoxBorder::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                stroke: Stroke::Px(1.0),
                                color: Colors::Black,
                            )
                        ),
                        background_color: color,
                    ),
                    height: Dimension::Px(80.0),

                    child: Text!(
                        format!("Item {}", i),
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                )) as Box<dyn Widget>
            }
        })
        .collect();

    let items_2: Vec<Box<dyn Widget>> = (0..1200)
        .map(|i| {
            let color = if i % 2 == 0 {
                Color::Rgb(100, 149, 237) // cornflower blue
            } else {
                Color::Rgb(255, 160, 122) // light salmon
            };
            if i == 5 {
                Box::new(Container!(
                    padding: LayoutSpacing::all( Spacing::Px(10)),
                    height: Dimension::Px(200.0),
                    box_decoration: BoxDecoration!(
                        background_color: Colors::Green,
                    ),
                    child: Text!(
                        format!("Item {}", i),
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                )) as Box<dyn Widget>
            } else {
                Box::new(Container!(
                    // width: Dimension::Px(100.0),
                    margin: LayoutSpacing! (top: Spacing::Px(30)),
                    box_decoration: BoxDecoration!(
                        border: BoxBorder::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                stroke: Stroke::Px(1.0),
                                color: Colors::Black,
                            )
                        ),
                        background_color: color,
                    ),
                    height: Dimension::Px(80.0),
                    child: Text!(
                        format!("Item {}", i),
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                )) as Box<dyn Widget>
            }
        })
        .collect();
    let content = Column! (
        horizontal_alignment: BoxAlignment::Center,
        // vertical_alignment: BoxAlignment::Center,
        children: items
    );

    let content_2 = Column! (
        horizontal_alignment: BoxAlignment::Center,
        // vertical_alignment: BoxAlignment::Center,
        children: items_2
    );
    let scrollbar = ScrollBar {
        track: ScrollTrack { width: Dimension::Px(2.0), color: Colors::Transparent, hover_color: Colors::Gray.alpha(120) },
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
    let app = Container!(
        child: Column!(
            children: [
                Container!(
                    height: Dimension::Px(80.0),
                    box_decoration: BoxDecoration!(
                        background_color: Colors::Green,
                    ),
                    child: Text!(
                        "This is header",
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                ),

                Row!(
                    children: [
                        Container!(
                            padding: LayoutSpacing::horizontal(Spacing::Px(10)),
                            child: Scrollable!(
                                axis: ScrollAxis::Vertical,
                                vertical_scroll_bar: scrollbar.clone(),
                                child: content,
                            ),
                        ),

                        Container!(
                            padding: LayoutSpacing::horizontal(Spacing::Px(10)),
                            child: Scrollable!(
                                axis: ScrollAxis::Vertical,
                                vertical_scroll_bar: scrollbar,
                                child: content_2,
                            ),
                        ),
                    ]
                ),
                Container!(
                    height: Dimension::Px(80.0),
                    box_decoration: BoxDecoration!(
                        background_color: Colors::Green,
                    ),
                    child: Text!(
                        "This is footer",
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                )
            ]
        )
    );
    AimerApp::start(app);
}
#[allow(unused)]
fn test_scrollable_row() {
    let items: Vec<Box<dyn Widget>> = (0..12000)
        .map(|i| {
            let color = if i % 2 == 0 {
                Color::Rgb(100, 149, 237) // cornflower blue
            } else {
                Color::Rgb(255, 160, 122) // light salmon
            };
            if i == 5 {
                Box::new(Container!(
                    padding: LayoutSpacing::all( Spacing::Px(10)),
                    margin: LayoutSpacing! (right: Spacing::Px(10)),
                    width: Dimension::Px(200.0),
                    box_decoration: BoxDecoration!(
                        border: BoxBorder::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                stroke: Stroke::Px(1.0),
                                color: Colors::Black,
                            )
                        ),
                        background_color: Colors::Green,
                    ),
                    child: Text!(
                        format!("Item {}", i),
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                )) as Box<dyn Widget>
            } else {
                Box::new(Container!(
                    // width: Dimension::Px(100.0),
                    margin: LayoutSpacing! (right: Spacing::Px(10)),
                    box_decoration: BoxDecoration!(
                        border: BoxBorder::all(
                            BorderSlice!(
                                style: BorderStyle::Solid,
                                stroke: Stroke::Px(1.0),
                                color: Colors::Black,
                            )
                        ),
                        background_color: color,
                    ),
                    width: Dimension::Px(80.0),
                    child: Text!(
                        format!("Item {}", i),
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                )) as Box<dyn Widget>
            }
        })
        .collect();
    let content = Row! (
        // horizontal_alignment: BoxAlignment::Center,
        vertical_alignment: BoxAlignment::Center,
        children: items
    );
    let scrollbar = ScrollBar {
        track: ScrollTrack { width: Dimension::Px(2.0), color: Colors::Transparent, hover_color: Colors::Gray.alpha(120) },
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
    let app = Container!(

        // width: Dimension::Px(400.0),
        // height: Dimension::Px(600.0),
        child: Row!(
            children: [
                Container!(
                    width: Dimension::Px(80.0),
                    box_decoration: BoxDecoration!(
                        background_color: Colors::Green,
                    ),
                    child: Text!(
                        "This is header",
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                ),

                Container!(
                    padding: LayoutSpacing::all(Spacing::Px(10)),
                    child: Scrollable!(
                        axis: ScrollAxis::Horizontal,
                        vertical_scroll_bar: scrollbar,
                        child: content,
                    ),
                ),
                Container!(
                    width: Dimension::Px(80.0),
                    box_decoration: BoxDecoration!(
                        background_color: Colors::Green,
                    ),
                    child: Text!(
                        "This is footer",
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle! {
                            font_size: 15,
                            color: Colors::Black,
                        }
                    )
                )
            ]
        )
    );
    AimerApp::start(app);
}

#[allow(unused)]
fn test_image() {
    AimerApp::start(Container!(
        padding: LayoutSpacing::all(Spacing::Percent(15)),
        box_decoration: BoxDecoration!(
            background_color: Colors::Black,
            // background_color: Colors::White,
        ),
        child: Container!(
            box_decoration: BoxDecoration!(
                background_color: Color::Rgb(41, 31, 31),
                border_radius: (55, 0, 55, 0),
                box_shadow: [
                    BoxShadow!(
                        color: Colors::Gray.alpha(200),
                        blur: 12.0,
                        spread : 10.0,
                        offset_x: 40.0,
                        offset_y: 40.0,
                        // offset_x: -10.0,
                        // offset_y: -10.0,
                        // inset: false
                    ),
                    // BoxShadow!(
                    //     color: Colors::Black,
                    //     blur: 8.0,
                    //     side: ShadowSide::Left
                    //     // inset: false
                    // ),
                    // BoxShadow!(
                    //     color: Colors::Green.alpha(120),
                    //     blur: 20.0,
                    //     spread: 30.0,
                    //     inset: false
                    // ),
                ]
            ),
            padding: LayoutSpacing::all(Spacing::Px(10)),
            child: AssetImage! (
                // size: Size{width: Dimension::Px(360.0), height: Dimension::Px(240.0)},
                // source: ImageSource::Network("https://cdn.pixabay.com/photo/2017/05/31/16/39/windows-2360920_1280.png".to_string()),
                // "https://img.freepik.com/free-vector/bird-colorful-gradient-design-vector_343694-2506.jpg?semt=ais_incoming&w=740&q=80",
                // "https://kinsta.com/wp-content/uploads/2019/08/jpg-vs-jpeg.jpg",
                // "/Users/cottons/Downloads/PNG_transparency_demonstration_1.png",
                // "https://media.istockphoto.com/id/814423752/photo/eye-of-model-with-colorful-art-make-up-close-up.jpg?s=612x612&w=0&k=20&c=l15OdMWjgCKycMMShP8UK94ELVlEGvt7GmB_esHWPYE=",
                // "https://www.evenlund.com/wp-content/uploads/2022/03/colortile.png",
                // "/Users/cottons/Downloads/PNG_transparency_demonstration_1.png",
                // "/Users/cottons/Downloads/2a-color-bars2.png",
                // "https://upload.wikimedia.org/wikipedia/commons/4/47/PNG_transparency_demonstration_1.png",
                "assets/my_image.png",
                // "https://upload.wikimedia.org/wikipedia/commons/6/66/SMPTE_Color_Bars.svg",
                // "https://t4.ftcdn.net/jpg/02/77/71/45/360_F_277714513_fQ0akmI3TQxa0wkPCLeO12Rx3cL2AuIf.jpg",
                fit: BoxFit::FitWidth,
                scale: 1.1,
                // keep_aspect_ratio: false,
                // loading_widget: SizedBox!(color: Colors::Green),
                // error_widget: SizedBox!(color: Colors::Red),
            )
        ),
    ))
}
