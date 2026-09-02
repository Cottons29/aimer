#[allow(unused_imports)]
use std::path::PathBuf;
#[allow(unused_imports)]
use std::sync::OnceLock;

#[allow(unused_imports)]
use aimer_cupid::canvas::CupidCanvas;
#[allow(unused_imports)]
use aimer_cupid::gpu_context::GpuContext;
#[allow(unused_imports)]
use aimer_cupid::renderer::Renderer;
#[allow(unused_imports)]
use aimer_cupid::text_pipeline::TextOverflowMode;
#[allow(unused_imports)]
use aimer_cupid::utilities::Color;
#[allow(unused_imports)]
use aimer_utils::{ExecTimes, debug};
#[allow(unused_imports)]
use winit::application::ApplicationHandler;
#[allow(unused_imports)]
use winit::event::{ElementState, WindowEvent};
#[allow(unused_imports)]
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
#[allow(unused_imports)]
use winit::window::{Window, WindowId};

static MY_EVENT_PROXY: OnceLock<EventLoopProxy<MyWindowEvent>> = OnceLock::new();

/// Known static text rendered every frame. Kept as a single source of truth so
/// it can be pre-shaped during init (Level 1 warm-up) and drawn from the warm
/// cache on the very first frame, avoiding the cold first-paint shaping stall.
const WELCOME_TEXT: &str = r#"
                English — Hello / Hi               Khmer — សួស្តី (Suosdei)               French — Bonjour
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
                Extended Latin — ÀÉÎÕÜ ß Æ Œ Ł Đ Þ Ǆ Ȝ ẞ      Greek — Ελληνικά: Καλημέρα κόσμε
                Cyrillic — Русский: Добрый день мир           Armenian — Հայերեն: Բարեւ աշխարհ
                Georgian — ქართული: გამარჯობა                 Ethiopic — ሰላም ዓለም
                Hebrew — עִבְרִית: שלום עולם                  Arabic — العَرَبِيَّة: مَرْحَبًا بِالعَالَم
                Persian — فارسی: سلام دنیا                     Urdu — اردو: السلام علیکم
                Devanagari — हिन्दी: नमस्ते दुनिया             Bengali — বাংলা: শুভ সকাল
                Gurmukhi — ਪੰਜਾਬੀ: ਸਤ ਸ੍ਰੀ ਅਕਾਲ                 Gujarati — ગુજરાતી: નમસ્તે દુનિયા
                Tamil — தமிழ்: வணக்கம் உலகம்                   Telugu — తెలుగు: నమస్కారం ప్రపంచం
                Kannada — ಕನ್ನಡ: ನಮಸ್ಕಾರ ಜಗತ್ತು                 Malayalam — മലയാളം: നമസ്കാരം ലോകം
                Sinhala — සිංහල: ආයුබෝවන් ලෝකය                 Thai — ไทย: สวัสดีชาวโลก
                Lao — ລາວ: ສະບາຍດີໂລກ                        Khmer — ខ្មែរ: សួស្តី\u{200B}ពិភពលោក
                Myanmar — မြန်မာ: မင်္ဂလာပါ ကမ္ဘာ               Tibetan — བོད་ཡིག: བཀྲ་ཤིས
                CJK — 中文: 你好世界　日本語: こんにちは世界　한국어: 안녕하세요 세계
                CJK punctuation — 「」『』【】（）［］〈〉《》、。，？！：；…・—〜￥
                Combining — é å ö ñ Ž Ā क् + ष् + त्र      Direction — LTR abc / RTL אבג / مرحبا
                Symbols — © ® ™ § ¶ † ‡ № ℗ ℃ ℉ → ← ↔ ⇧ ∞ ≈ ≠ ≤ ≥ √ ∑ ∆
                Emoji — 😀 😃 🥳 🚀 ❤️ ♥️ ☕️ ✈️ 👨‍👩‍👧‍👦 🏳️‍🌈 👍🏽 🇰🇭 🇯🇵 🇺🇸
                Variation selectors — ☎︎ ☎️ ✈︎ ✈️ ☕︎ ☕️       ZWJ — 👩‍💻 🧑‍🎨 🏃‍♂️
                Private-use probes — \u{E000} \u{F8FF} \u{F0000} (unsupported glyphs stay bounded)
                                    "#;

/// Focused shaping probes shown above the full Unicode showcase. Keep these
/// lines short enough to remain readable in the default demo window: each
/// script has a normal word, combining marks, and a pre-base or conjunct form
/// that exercises the owned GSUB/GPOS path when the face provides coverage.
const SOUTHEAST_ASIAN_TEXT: &str = r#"Thai      — ไทย: สวัสดีชาวโลก | เกาะ | กำลัง
Lao       — ລາວ: ສະບາຍດີໂລກ | ເກົາ | ກຳລັງ
Khmer     — ខ្មែរ: សួស្តីពិភពលោក | ស្តី | ក្រ
Myanmar   — မြန်မာ: မင်္ဂလာပါ ကမ္ဘာ | မေ | က္က
Combining — กั ก่ ก้ | ກິ ກ່ | កា ស៊ | ကိ ကု
Mixed     — ไทย / ລາວ / ខ្មែរ / မြန်မာ / Latin"#;

const SOUTHEAST_ASIAN_FONT_SIZE: f32 = 32.0;

const FONT_WEIGHT_SAMPLES: &[(u16, &str)] = &[
    (200, "200 Aa"),
    (300, "300 Aa"),
    (400, "400 Aa"),
    (500, "500 Aa"),
    (600, "600 Aa"),
    (700, "700 Aa"),
    (800, "800 Aa"),
];

/// Color-glyph probes include both layered vector and embedded-strike
/// candidates. The actual representation is selected by the resolved face;
/// the label makes a missing color fallback obvious in the demo.
const COLOR_GLYPH_SHOWCASE: &str = "😀 🥳 🚀 ❤️ 🏳️‍🌈 👍🏽";

/// Font sizes warmed at startup (Level 2 glyph-set pre-rasterization).
const WARM_FONT_SIZES: [f32; 3] = [20.0, 32.0, 44.0];

pub fn time_consume(func: impl FnOnce()) {
    let start = aimer_utils::AnimInstant::now();
    func();
    let elapsed = start.elapsed();
    println!("Time elapsed: {} ms", elapsed.as_millis());
}
#[cfg(not(target_arch = "wasm32"))]
struct App<'w> {
    gpu: Option<GpuContext<'w>>,
    renderer: Option<Renderer>,
    canvas: CupidCanvas,
    window: Option<Window>,
    texture_id: Option<u32>,
}
#[allow(dead_code)]
enum MyWindowEvent {
    FirstFrame,
}
#[cfg(not(target_arch = "wasm32"))]
impl<'w> App<'w> {
    fn new() -> Self {
        Self {
            gpu: None,
            renderer: None,
            canvas: CupidCanvas::new(),
            window: None,
            texture_id: None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'w> ApplicationHandler<MyWindowEvent> for App<'w> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let title = "Cupid Render Engine — Test";
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(800, 600));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;

            attrs
                .with_decorations(true)
                .with_titlebar_hidden(false)
                .with_titlebar_transparent(false)
                .with_title_hidden(false)
                .with_titlebar_buttons_hidden(false)
                .with_fullsize_content_view(false)
        };
        let window = event_loop.create_window(attrs).unwrap();
        // window.set_min_inner_size(Some(winit::dpi::LogicalSize::new(1500, 700)));
        window.set_title(title);

        let size = window.inner_size();

        // SAFETY: We store the window in self and the GpuContext borrows it.
        // The window outlives the GpuContext because we drop gpu before window.
        let window_ref: &'w Window = unsafe { &*(&window as *const Window) };
        let gpu = GpuContext::initialize(window_ref, size);

        debug!("Initializing GPU context and loading test image");
        let mut img_renderer = Renderer::new(&gpu.device, gpu.format);
        debug!("Initialized GPU context");

        // AOT-style warm-up: move the expensive text shaping/rasterization off
        // the first visible frame and behind init, so startup is "fast like a
        // compiled language" instead of paying the cold-cache 27–86 ms stall on
        // first paint.
        //
        // Level 2 — pre-rasterize the common ASCII glyph set at the font sizes
        // the app uses, filling the glyph atlas so even brand-new strings only
        // pay shaping (never glyph rasterization).
        img_renderer.warm_glyph_set(&gpu.device, &gpu.queue, &WARM_FONT_SIZES);
        // Level 1 — pre-shape and lay out the known static text at the size and
        // wrapping width it is drawn with, so it renders from the warm cache on
        // the very first frame. The wrap width mirrors the draw call below
        // (`inner_size().width - 60.0`).
        img_renderer.warm_text(
            &gpu.device,
            &gpu.queue,
            WELCOME_TEXT,
            44.0,
            size.width as f32 - 60.0,
        );
        img_renderer.warm_text(
            &gpu.device,
            &gpu.queue,
            SOUTHEAST_ASIAN_TEXT,
            SOUTHEAST_ASIAN_FONT_SIZE,
            size.width as f32 - 60.0,
        );
        img_renderer.warm_text(&gpu.device, &gpu.queue, COLOR_GLYPH_SHOWCASE, 22.0, 0.0);
        debug!("Text warm-up complete");
        let image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("image.png");
        debug!("Loading test image from {}", image_path.display());
        let img = image::open(&image_path)
            .unwrap_or_else(|e| panic!("Failed to load {}: {e}", image_path.display()))
            .into_rgba8();
        let (img_w, img_h) = img.dimensions();
        let tex_id = img_renderer.image_pipeline.upload_image(
            &gpu.device,
            &gpu.queue,
            img_w,
            img_h,
            img.as_raw(),
        );
        debug!("Uploaded image to GPU");

        self.texture_id = Some(tex_id);
        debug!("Test image uploaded");
        self.renderer = Some(img_renderer);
        debug!("Renderer initialized");
        self.gpu = Some(gpu);
        debug!("GPU context initialized");
        self.window = Some(window);
        debug!("Window initialized");
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        debug!("App resumed");
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: MyWindowEvent) {
        match event {
            MyWindowEvent::FirstFrame => {
                self.window.as_ref().unwrap().request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size);
                    // Ensure a frame is painted after the surface is reconfigured.
                    // On macOS, the initial request_redraw() in resumed() can be
                    // silently dropped if the window isn't fully on-screen yet.
                    // The first Resized event is the reliable signal that the
                    // window is visible and the surface is ready.
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }

            WindowEvent::MouseInput { state, .. } => {
                if ElementState::Pressed == state
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }

            WindowEvent::RedrawRequested => {
                let gpu = match &self.gpu {
                    Some(g) => g,
                    None => return,
                };
                let renderer = match &mut self.renderer {
                    Some(r) => r,
                    None => return,
                };

                let frame = match gpu.begin_frame() {
                    wgpu::CurrentSurfaceTexture::Success(f)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    _ => return,
                };

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let width = gpu.width();
                let height = gpu.height();

                // Build draw commands using CupidCanvas.
                record_demo_frame(&self.canvas, width as f32, height as f32);

                // Draw a blue background rect
                // self.canvas
                //     .fill_rect(20.0, 20.0, 300.0, 200.0, Color::new(0.2, 0.4, 0.8, 1.0),
                // [10.0; 4]);
                //
                // // Draw a red rect
                // self.canvas
                //     .fill_rect(50.0, 50.0, 150.0, 80.0, Color::red(), [20.0; 4]);
                //
                // // Draw a green rounded rect
                // self.canvas
                //     .fill_rect(200.0, 100.0, 180.0, 120.0, Color::green(), [20.0; 4]);
                //
                // // Draw a rect with border
                // self.canvas.fill_rect_with_border(
                //     420.0, 300.0, 160.0, 100.0,
                //     Color::red(),
                //     [12.0; 4],
                //     3.0,
                //     Color::new(0.2, 0.2, 0.8, 1.0),
                // );
                //
                // // Draw a border-only rect (transparent fill)
                // self.canvas.fill_rect_with_border(
                //     420.0, 420.0, 460.0, 480.0,
                //     Color::blue(),
                //     [28.0; 4],
                //     0.0,
                //     Color::red(),
                // );
                //
                // // Test clipping
                // self.canvas.set_clip(50.0, 400.0, 200.0, 100.0);
                // self.canvas.fill_rect(
                //     30.0, 380.0, 300.0, 150.0,
                //     Color::red(),
                //     [0.0; 4],
                // );
                //
                // // Test save/translate/restore
                // self.canvas.save();
                // self.canvas.translate(400.0, 50.0);
                // self.canvas.fill_rect(
                //     0.0, 0.0, 500.0, 450.0,
                //     Color::new(0.8, 0.2, 0.8, 1.0).set_alpha(128),
                //     [5.0; 4],
                // );
                // self.canvas.restore();

                // Draw text
                // self.canvas.draw_text(30.0, 250.0, "Hello from Cupid!", 32.0,
                // Color::black());
                //
                // self.canvas
                //     .draw_text(30.0, 300.0, "Wgpu-powered UI render engine", 20.0,
                // Color::black());

                // Mixed CJK + color emoji line — verifies fixes A (no first-frame
                // stall on CJK) and B/C (AppleColorEmoji renders alongside CJK).
                // self.canvas.draw_text(30.0, 340.0, "អរគុណ 你哈皮  With State 你好 きみなと
                // 👉", 44.0, Color::black()); self.canvas
                //     .draw_text(30.0, 740.0, "هَمْزَة عَلَى الأَلِفْ	", 44.0, Color::black());
                // Draw test image if available
                // if let Some(tex_id) = self.texture_id {
                //     self.canvas.draw_image(500.0, 200.0, 300.0, 300.0, tex_id);
                // }

                ExecTimes::print_time(|| {
                    renderer.render(
                        &gpu.device,
                        &gpu.queue,
                        &view,
                        width,
                        height,
                        gpu.is_srgb,
                        &self.canvas.draw_list(),
                    )
                });

                gpu.end_frame(frame);
                #[cfg(debug_assertions)]
                {
                    debug!(
                        "#############################>Time Consume<#######################################"
                    );
                    ExecTimes::cost_grouping();
                    debug!(
                        "##################################################################################"
                    )
                }
            }
            _ => {}
        }
    }

    // fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
    //     if self.frame_count > 0 {
    //         self.frame_count -= 1;
    //         debug!("Render Frame: {}", self.frame_count);
    //         self.window.as_ref().unwrap().request_redraw();
    //     }
    // }
}

fn record_demo_frame(canvas: &CupidCanvas, width: f32, height: f32) {
    canvas.begin_frame();

    // The renderer clears the swapchain to transparent. The demo text is black,
    // so an opaque light surface is required for it to be visible on every
    // platform and window alpha mode.
    let width = width.max(1.0);
    let height = height.max(1.0);
    canvas.fill_rect(0.0, 0.0, width, height, Color::white(), [0.0; 4]);

    // Keep both the layout box and the GPU clip inside the current surface.
    // `draw_text_wrapped` only supplied a width, so tall paragraphs and a
    // resized/narrow window could keep emitting glyphs outside the visible
    // surface. The inset also leaves room for glyph bearings and antialiasing.
    let text_width = (width - 60.0).max(1.0);
    let text_height = (height - 60.0).max(1.0);
    canvas.set_clip(30.0, 30.0, text_width, text_height);

    // Keep a dedicated SEA block visible while the complete showcase remains
    // below it. This makes pre-base vowels, combining marks, Khmer conjuncts,
    // and Myanmar fallback behavior easy to inspect without hunting through a
    // densely wrapped paragraph.
    canvas.draw_text(
        30.0,
        30.0,
        "Aimer — Southeast Asian shaping",
        22.0,
        Color::black(),
        400,
    );
    let sea_y = 62.0;
    let sea_height = (height * 0.40).min(250.0).max(1.0);
    canvas.draw_text_with_overflow(
        30.0,
        sea_y,
        SOUTHEAST_ASIAN_TEXT,
        SOUTHEAST_ASIAN_FONT_SIZE,
        Color::black(),
        text_width,
        sea_height,
        TextOverflowMode::Wrap,
        400,
    );

    let diagnostics_y = (sea_y + sea_height + 12.0).min(height - 1.0);
    canvas.draw_text(30.0, diagnostics_y, "Font weights", 18.0, Color::black(), 400);
    for (index, (weight, sample)) in FONT_WEIGHT_SAMPLES.iter().enumerate() {
        let column = index % 4;
        let row = index / 4;
        canvas.draw_text(
            30.0 + column as f32 * 86.0,
            diagnostics_y + 24.0 + row as f32 * 24.0,
            sample,
            18.0,
            Color::black(),
            *weight,
        );
    }

    let color_x = (width * 0.56).max(360.0).min(width - 180.0);
    canvas.draw_text(color_x, diagnostics_y, "Color glyphs / text", 18.0, Color::black(), 400);
    canvas.draw_text(color_x, diagnostics_y + 24.0, "Red", 17.0, Color::red(), 400);
    canvas.draw_text(
        color_x + 42.0,
        diagnostics_y + 24.0,
        "Green",
        17.0,
        Color::green(),
        400,
    );
    canvas.draw_text(
        color_x + 100.0,
        diagnostics_y + 24.0,
        "Blue",
        17.0,
        Color::blue(),
        400,
    );
    canvas.draw_text(
        color_x,
        diagnostics_y + 50.0,
        COLOR_GLYPH_SHOWCASE,
        22.0,
        Color::black(),
        400,
    );
    canvas.draw_text(
        color_x,
        diagnostics_y + 77.0,
        "COLR/CPAL + bitmap color",
        14.0,
        Color::new(0.25, 0.25, 0.25, 1.0),
        400,
    );

    let diagnostics_height = 100.0;
    let showcase_y = (diagnostics_y + diagnostics_height + 12.0).min(height - 1.0);
    let showcase_height = (height - showcase_y - 30.0).max(1.0);
    canvas.draw_text_with_overflow(
        30.0,
        showcase_y,
        WELCOME_TEXT,
        18.0,
        Color::black(),
        text_width,
        showcase_height,
        TextOverflowMode::Wrap,
        400,
    );
    canvas.clear_clip();
}

fn main() {
    let event_loop = EventLoop::<MyWindowEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");

    MY_EVENT_PROXY.set(event_loop.create_proxy()).ok();
    event_loop.set_control_flow(ControlFlow::Wait);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = App::new();
        event_loop.run_app(&mut app).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::record_demo_frame;
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_cupid::canvas::CupidCanvas;

    #[test]
    fn demo_records_an_opaque_background_before_text() {
        let canvas = CupidCanvas::new();

        record_demo_frame(&canvas, 800.0, 600.0);

        let draw_list = canvas.draw_list();
        assert!(matches!(
            draw_list.commands().first(),
            Some(DrawCommand::FillRect { color, .. })
                if color.a > 0 && color.r > 0 && color.g > 0 && color.b > 0
        ));
        assert!(draw_list.commands().iter().any(|command| matches!(
            command,
            DrawCommand::DrawText { draw_glyphs: true, .. }
        )));
        assert!(draw_list.commands().iter().any(|command| matches!(
            command,
            DrawCommand::PushClip { rect, .. }
                if rect.x >= 0.0 && rect.y >= 0.0 && rect.width <= 800.0 && rect.height <= 600.0
        )));
        assert!(matches!(
            draw_list.commands().last(),
            Some(DrawCommand::PopClip)
        ));
    }

    #[test]
    fn demo_records_color_and_multiple_weight_samples() {
        let canvas = CupidCanvas::new();

        record_demo_frame(&canvas, 800.0, 600.0);

        let draw_list = canvas.draw_list();
        let draw_texts = draw_list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::DrawText {
                    text,
                    color,
                    font_weight,
                    ..
                } => Some((text.as_ref(), *color, *font_weight)),
                _ => None,
            })
            .collect::<Vec<_>>();

        for weight in [200, 300, 400, 500, 600, 700, 800] {
            assert!(
                draw_texts.iter().any(|(_, _, recorded)| *recorded == weight),
                "the demo should record a {weight} weight sample"
            );
        }
        assert!(
            draw_texts.iter().any(|(text, _, _)| text.contains("😀")),
            "the demo should include a color-glyph probe"
        );
        assert!(
            draw_texts.iter().any(|(_, color, _)| {
                (color.r > 0 && color.g == 0 && color.b == 0)
                    || (color.g > 0 && color.r == 0 && color.b == 0)
                    || (color.b > 0 && color.r == 0 && color.g == 0)
            }),
            "the demo should include colored text samples"
        );
    }

    #[test]
    fn demo_text_wraps_within_the_inset_surface() {
        use super::{SOUTHEAST_ASIAN_FONT_SIZE, SOUTHEAST_ASIAN_TEXT, WELCOME_TEXT};
        use aimer_cupid::font::{FontFamily, FontStyle, FontWeight};
        use aimer_cupid::glyph_rasterizer::GlyphRasterizer;
        use aimer_cupid::text_layout::{layout_shaped_text, shape_text_styled};

        for (text, font_size) in [
            (WELCOME_TEXT, 18.0),
            (SOUTHEAST_ASIAN_TEXT, SOUTHEAST_ASIAN_FONT_SIZE),
        ] {
            let mut rasterizer = GlyphRasterizer::new();
            let shaped = shape_text_styled(
                &mut rasterizer,
                text,
                font_size,
                FontFamily::SANS_SERIF,
                FontWeight::Normal,
                FontStyle::Normal,
                None,
            );
            let glyphs = layout_shaped_text(&shaped, 30.0, 30.0, 740.0);

            let out_of_bounds = glyphs
                .iter()
                .filter(|glyph| glyph.x < 27.0 || glyph.x + glyph.width as f32 > 773.0)
                .map(|glyph| (glyph.codepoint, glyph.line_index, glyph.x, glyph.width))
                .collect::<Vec<_>>();
            assert!(out_of_bounds.is_empty(), "{out_of_bounds:?}");
        }
    }
}
