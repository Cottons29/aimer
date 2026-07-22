use std::hint::black_box;
use std::time::{Duration, Instant};

use aimer_cupid::font::{FontFamily, FontStyle, FontWeight};
use aimer_cupid::text_layout::shape_text_styled;
use aimer_cupid::text_pipeline::glyph_rasterizer::GlyphRasterizer;
use unicode_segmentation::UnicodeSegmentation;

const TARGET_CHARACTERS: usize = 2_000;
const DEFAULT_ITERATIONS: usize = 10;
const SAMPLE: &str =
    "Aimer shapes styled text into glyph runs before wrapping and painting the markdown document. ";

fn benchmark(iterations: usize, mut operation: impl FnMut()) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    start.elapsed()
}

fn average(duration: Duration, iterations: usize) -> Duration {
    duration / u32::try_from(iterations).expect("iteration count must fit in u32")
}

fn main() {
    #[cfg(not(debug_assertions))]
    panic!("run without --release to benchmark the unoptimized debug profile");

    let iterations = std::env::args()
        .nth(1)
        .map(|value| {
            value
                .parse()
                .expect("iterations must be a positive integer")
        })
        .unwrap_or(DEFAULT_ITERATIONS);
    assert!(iterations > 0, "iterations must be greater than zero");

    let text = SAMPLE
        .repeat(TARGET_CHARACTERS.div_ceil(SAMPLE.len()))
        .chars()
        .take(TARGET_CHARACTERS)
        .collect::<String>();

    let per_cluster = benchmark(iterations, || {
        let mut rasterizer = GlyphRasterizer::new();
        for cluster in text.graphemes(true) {
            black_box(rasterizer.shape_cluster_for_family(
                black_box(cluster),
                16.0,
                FontFamily::SANS_SERIF,
                FontWeight::Normal,
                FontStyle::Normal,
            ));
        }
    });
    let per_run = benchmark(iterations, || {
        let mut rasterizer = GlyphRasterizer::new();
        black_box(shape_text_styled(
            &mut rasterizer,
            black_box(&text),
            16.0,
            FontFamily::SANS_SERIF,
            FontWeight::Normal,
            FontStyle::Normal,
        ));
    });

    let per_cluster_average = average(per_cluster, iterations);
    let per_run_average = average(per_run, iterations);
    let speedup = per_cluster_average.as_secs_f64() / per_run_average.as_secs_f64();

    println!(
        "debug text shaping benchmark: {} characters, {iterations} iterations",
        text.chars().count()
    );
    println!("per-cluster average: {per_cluster_average:?}");
    println!("per-run average:     {per_run_average:?}");
    println!("speedup:             {speedup:.2}x");
}
