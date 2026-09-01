//! Scroll cost with real text content (the shape real apps scroll).
//!
//! ```text
//! cargo run -p aimer_laboratory --example scroll_text_profile --release --features aimer/frame-stats
//! ```

use std::hint::black_box;
use std::time::Instant;

use aimer::quiver::winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceId, MouseScrollDelta, TouchPhase, WindowEvent},
};
use aimer::style::LayoutSpacing;
use aimer::{
    AimerApp, AnyWidget, Color, Column, Container, HeadlessOptions, Scrollable, Text, Widget,
};

const FRAME_WIDTH: u32 = 800;
const FRAME_HEIGHT: u32 = 700;
const PARAGRAPHS: usize = 400;
const MEASURED: usize = 64;

fn paragraph(index: usize) -> AnyWidget {
    let body = format!(
        "Paragraph {index}: The retained layer fast path applies only when the whole \
         scrolling subtree is paint-stable, while a plain container records hit-test \
         bounds and therefore stays on the ordinary culled draw path. Text shaping is \
         cached per width, but every visible paragraph is still walked each frame."
    );
    Container::new()
        .color(if index % 2 == 0 {
            Color::WHITE
        } else {
            Color::Rgb(242, 242, 247)
        })
        .padding(LayoutSpacing::all(12))
        .box_child(Text::new(body).wrapped())
}

fn page() -> AnyWidget {
    Scrollable::new()
        .vertical_scroll_bar(None)
        .horizontal_scroll_bar(None)
        .child(Column::new().children((0..PARAGRAPHS).map(paragraph)))
        .boxed()
}

fn main() {
    let mut app = AimerApp::start_headless_with(
        page(),
        HeadlessOptions {
            size: PhysicalSize::new(FRAME_WIDTH, FRAME_HEIGHT),
            scale_factor: 1.0,
        },
    );

    for _ in 0..4 {
        app.render_frame();
    }

    aimer::frame_stats::reset_frame_content_stats();
    aimer::frame_stats::reset_frame_breakdown();

    let device_id = DeviceId::dummy();
    let mut times = Vec::with_capacity(MEASURED);
    for step in 0..MEASURED {
        let phase = if step == 0 {
            TouchPhase::Started
        } else {
            TouchPhase::Moved
        };
        app.send_window_event(WindowEvent::MouseWheel {
            device_id,
            delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -48.0)),
            phase,
        });
        let start = Instant::now();
        black_box(&mut app);
        app.render_frame();
        times.push(start.elapsed().as_secs_f64() * 1e6);
    }

    times.sort_by(f64::total_cmp);
    let p50 = times[times.len() / 2];
    let p95 = times[(times.len() * 95 / 100).min(times.len() - 1)];
    let content = aimer::frame_stats::frame_content_stats();
    println!(
        "text scroll: p50={p50:.2}us p95={p95:.2}us  cmds/frame={:.1} nodes/frame={:.1} \
         retained/frame={:.1} text-cmds/frame={:.1} text-miss/frame={:.1}",
        content.average_draw_commands(),
        content.average_drawn_nodes(),
        content.average_retained_layers(),
        content.average_text_commands(),
        content.average_text_cache_misses(),
    );
}
